//! `FreeBlock` (DynamicObject type 14): a dropped item lying in the world, with fall / water / hover physics,
//! priority-blockhead magnet, pickup offers, save dict and network blobs.
//!
//! `draw:` and the client-only ivars (cube, light*, offsets) are not ported.
//!
//! Numeric rules: every float is f32 unless the spec writes f64; `min(0.9, x)` forms are written as C `if`
//! chains through [`min_d`] (no `f32::min/max/clamp`); float-to-int casts use `fmath::cconv`.
//! RNG: `r01 = (float)rand() / 2147483648.0f`.

use std::any::Any;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

use crate::dynworld::state::DynamicWorld;
use crate::fmath::cconv::{f32_to_i16, f32_to_i32};
use crate::fmath::vector::Vector;
use crate::items::inventory_item::{InventoryItem, ItemRef, SubSlot};
use crate::items::predicates::{item_type_is_interaction_object, item_type_requires_ownership_to_remove};
use crate::objects::base::{
    base_dynamic_object_net_data, base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_cache_net_data, base_init_with_world_dynamic_world_save_dict_cache,
    base_remote_update, base_remove_from_macro_block, base_update_position, new_mutable_dict, padded_bytes,
    ub_null_tile_deref, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{self, alloc_obj, CacheRef, ObjRef};
use crate::plist::helpers::{data_from_property_list, property_list_from_data};
use crate::plist::value::{PlistDict, PlistValue};
use crate::rng::glibc_rand::r01_tagged;
use crate::types::item_type::ItemType;
use crate::types::netdata::{DynamicObjectNetData, FreeblockCreationNetData, FreeblockUpdateNetData};
use crate::types::structs::{IntPair, Vector2, WireStruct};
use crate::world::helpers::{
    tile_contains_door, tile_contains_trap_door, tile_is_air, tile_is_air_or_snow, tile_is_air_water_or_snow,
    tile_is_solid, tile_is_water,
};
use crate::world::state::{TileRef, World};
use crate::world::tiles::{tile_at_world_position, tile_at_world_position_loaded};

/// `DYNAMIC_OBJECT_TYPE_FREE_BLOCK`.
pub const DYNAMIC_OBJECT_TYPE_FREE_BLOCK: i32 = 14;
/// `ITEM_TIME_CRYSTAL`: sub-items of this type are dropped on every path.
pub const ITEM_TIME_CRYSTAL: i32 = 11;
/// `ITEM_PINE_CONE`.
pub const ITEM_PINE_CONE: i32 = 27;
/// `TILE_LAVA` type index.
pub const TILE_LAVA: u8 = 31;
/// Pole items: SANTA_HAT, FRUIT_HAT, SOUTH_POLE_BOOTS, GRASS_SKIRT.
pub const POLE_ITEMS: [i32; 4] = [142, 140, 143, 141];
/// Train items STEAM_TRAIN, PASSENGER_CAR, FREIGHT_CAR, HANDCAR.
const TRAIN_ITEMS: [i32; 4] = [0xcd, 0xd0, 0xce, 0xcc];

/// `FreeBlock: DynamicObject`.
pub static FREE_BLOCK_CLASS: ClassInfo =
    ClassInfo { name: "FreeBlock", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &[] };

/// `min(c, x)` as compiled: `c` when `c <= x`, else `x` (operand order matters for NaN only).
#[inline]
fn min_d(c: f64, x: f64) -> f64 {
    if c <= x {
        c
    } else {
        x
    }
}

fn is_pole_item(t: i32) -> bool {
    POLE_ITEMS.contains(&t)
}

/// `itemTypeIsInteractionObject(t) || train item || itemTypeRequiresOwnershipToRemove(t)` (pole items excluded).
fn is_special_non_pole(t: i32) -> bool {
    item_type_is_interaction_object(ItemType(t as u32))
        || TRAIN_ITEMS.contains(&t)
        || item_type_requires_ownership_to_remove(ItemType(t as u32))
}

fn mutable_array_of(items: Vec<PlistValue>) -> PlistValue {
    let mut v = PlistValue::mutable_array();
    if let PlistValue::Array(a) = &mut v {
        a.items = items;
    }
    v
}

/// Array of arrays of `[item saveData]`, items of type 11 skipped, empty slots kept.
fn sub_items_to_plist(subs: &[SubSlot]) -> PlistValue {
    let outer: Vec<PlistValue> = subs
        .iter()
        .map(|slot| {
            let inner: Vec<PlistValue> = slot
                .borrow()
                .iter()
                .filter(|it| it.borrow().item_type.0 as i32 != ITEM_TIME_CRYSTAL)
                .map(|it| PlistValue::data(it.borrow().save_data()))
                .collect();
            mutable_array_of(inner)
        })
        .collect();
    mutable_array_of(outer)
}

/// Rebuild sub-items from a `subItems` plist (array of arrays of NSData); type 11 skipped, every slot kept.
fn sub_items_from_plist(v: &PlistValue) -> Vec<SubSlot> {
    let mut out: Vec<SubSlot> = Vec::new();
    if let PlistValue::Array(a) = v {
        for slot in &a.items {
            let mut items: Vec<ItemRef> = Vec::new();
            if let PlistValue::Array(sa) = slot {
                for d in &sa.items {
                    if let PlistValue::Data(bytes) = d {
                        let item = InventoryItem::from_save_data(&bytes.bytes);
                        if item.borrow().item_type.0 as i32 != ITEM_TIME_CRYSTAL {
                            items.push(item);
                        }
                    }
                }
            }
            out.push(Rc::new(RefCell::new(items)));
        }
    }
    out
}

/// FreeBlock ivars 0x50..0xd0 (client-only ones omitted).
pub struct FreeBlockState {
    /// 0x50
    pub item_type: i32,
    /// 0x54
    pub data_a: u16,
    /// 0x56
    pub data_b: u16,
    /// 0x58
    pub hovers: bool,
    /// 0x5c, init 1.0; saved
    pub bounce_timer: f32,
    /// 0x60, positive = falling (`float_pos.y -= fall_speed`), init -0.05
    pub fall_speed: f32,
    /// 0x64, set in `init_sub_derived_objects`
    pub rotation: f32,
    /// 0x68 world time of creation / of last hover->drop
    pub creation_time: f64,
    /// 0x90 `None` unless a non-time-crystal sub item exists (primary init); always `Some` after save-load init
    pub sub_items: Option<Vec<SubSlot>>,
    /// 0x98
    pub x_velocity: f32,
    /// 0x9c horizontal drift speed in water
    pub x_velocity_water: f32,
    /// 0xa0
    pub y_velocity: f32,
    /// 0xb0 retained
    pub priority_blockhead: Option<ObjRef>,
    /// 0xb8
    pub dynamic_object_save_dict: Option<PlistDict>,
    /// 0xc0
    pub creation_sound_play_time: f32,
    /// 0xc4
    pub sound_type: i32,
    /// 0xc8 read from the dict key "paintColor"; no server use
    pub paint_color: i32,
}

impl FreeBlockState {
    /// All ivars zero as after `alloc`.
    fn zeroed() -> FreeBlockState {
        FreeBlockState {
            item_type: 0,
            data_a: 0,
            data_b: 0,
            hovers: false,
            bounce_timer: 0.0,
            fall_speed: 0.0,
            rotation: 0.0,
            creation_time: 0.0,
            sub_items: None,
            x_velocity: 0.0,
            x_velocity_water: 0.0,
            y_velocity: 0.0,
            priority_blockhead: None,
            dynamic_object_save_dict: None,
            creation_sound_play_time: 0.0,
            sound_type: 0,
            paint_color: 0,
        }
    }
}

/// `FreeBlock`.
pub struct FreeBlock {
    base: RefCell<DynamicObjectBase>,
    /// The FreeBlock ivars.
    pub st: RefCell<FreeBlockState>,
    world: Weak<World>,
    dw: Weak<DynamicWorld>,
}

/// Downcast helper used by Blockhead / DynamicWorld.
pub fn as_free_block(o: &ObjRef) -> Option<&FreeBlock> {
    o.as_any().downcast_ref::<FreeBlock>()
}

impl FreeBlock {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<FreeBlock> {
        let (w, d) = (Rc::downgrade(world), Rc::downgrade(dw));
        alloc_obj(|sw| FreeBlock {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            st: RefCell::new(FreeBlockState::zeroed()),
            world: w,
            dw: d,
        })
    }

    fn world(&self) -> Rc<World> {
        self.world.upgrade().expect("FreeBlock: world dropped")
    }

    fn dw(&self) -> Rc<DynamicWorld> {
        self.dw.upgrade().expect("FreeBlock: dynamic world dropped")
    }

    fn pos(&self) -> IntPair {
        self.base.borrow().pos
    }

    fn fp(&self) -> Vector2 {
        self.base.borrow().float_pos
    }

    fn set_fp(&self, v: Vector2) {
        self.base.borrow_mut().float_pos = v;
    }

    fn set_update_needs_to_be_sent(&self, v: bool) {
        self.base.borrow_mut().update_needs_to_be_sent = v;
    }

    /// `dynamicWorldChangedAtPos:pos objectType:[self objectType]` with the current `pos`.
    fn notify_changed(&self) {
        self.dw().dynamic_world_changed_at_pos_object_type(self.pos(), DYNAMIC_OBJECT_TYPE_FREE_BLOCK);
    }

    // ------------------------------------------------------------------ initialisers

    /// `-initSubDerivedObjects`: one `rand()` (rotation), `paintColor`; the DrawCube consumes no RNG.
    fn init_sub_derived_objects(&self) {
        let r = r01_tagged("free_block::init_sub_derived_objects");
        let rotation = (((r as f64) * 3.141592653589793f64) * 2.0f64) as f32;
        let mut st = self.st.borrow_mut();
        st.rotation = rotation;
        st.paint_color = st
            .dynamic_object_save_dict
            .as_ref()
            .and_then(|d| d.map.get(&"paintColor".to_string()))
            .map_or(0, |v| v.unsigned_int_value() as i32);
    }

    /// `-initWithWorld:dynamicWorld:atPosition:ofType:dataA:dataB:subItems:dynamicObjectSaveDict:cache:hovers:priorityBlockhead:`
    ///  (server drop).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        item_type: i32,
        data_a: u16,
        data_b: u16,
        sub_items: Option<&[SubSlot]>,
        dynamic_object_save_dict: Option<PlistDict>,
        cache: CacheRef,
        hovers: bool,
        priority_blockhead: Option<&ObjRef>,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        let dref: DynamicWorldRef = dw.clone();
        // the base initialiser always succeeds when the position is given
        let inited = base_init_with_world_dynamic_world_at_position_cache(&me_obj, &wref, &dref, pos, cache);
        debug_assert!(inited);
        {
            let mut st = me.st.borrow_mut();
            st.item_type = item_type;
            st.data_a = data_a;
            st.data_b = data_b;
            st.hovers = hovers;
            st.priority_blockhead = priority_blockhead.cloned();
            st.dynamic_object_save_dict = dynamic_object_save_dict;
            if let Some(subs) = sub_items {
                let mut result: Vec<SubSlot> = Vec::new();
                let mut has_items = false;
                for slot in subs {
                    let mut new_slot: Vec<ItemRef> = Vec::new();
                    for item in slot.borrow().iter() {
                        if item.borrow().item_type.0 as i32 != ITEM_TIME_CRYSTAL {
                            has_items = true;
                            new_slot.push(item.clone());
                        }
                    }
                    result.push(Rc::new(RefCell::new(new_slot)));
                }
                if has_items {
                    st.sub_items = Some(result);
                }
            }
        }
        if !hovers {
            // rand() call #1 (only if !hovers)
            let r = r01_tagged("free_block::new::jitter");
            let mut fp = me.fp();
            fp.x += r * 0.5f32 - 0.25f32;
            me.set_fp(fp);
        }
        {
            let mut st = me.st.borrow_mut();
            st.bounce_timer = 1.0f32;
            // (the original lowers this by another 0.1 when yVelocity is positive, which it never is yet)
            st.fall_speed = -0.05f32;
        }
        let mut fp = me.fp();
        fp.y = 0.5f32 + me.pos().y as f32;
        me.set_fp(fp);
        let now = world.world_time();
        {
            let mut st = me.st.borrow_mut();
            st.creation_time = now;
            st.y_velocity = 0.5f32; // (sic) 0.5, not zero
        }
        me.init_sub_derived_objects();
        Some(me_obj)
    }

    /// `-initWithWorld:dynamicWorld:saveDict:cache:` (load from save).
    pub fn from_save_dict(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        let dref: DynamicWorldRef = dw.clone();
        let inited = base_init_with_world_dynamic_world_save_dict_cache(&me_obj, &wref, &dref, save_dict, cache);
        debug_assert!(inited);
        let get = |k: &str| save_dict.map.get(&k.to_string());
        let fv = |k: &str| get(k).map_or(0.0f32, |v| v.float_value());
        {
            let mut st = me.st.borrow_mut();
            st.bounce_timer = fv("bounceTimer");
            st.fall_speed = fv("fallSpeed");
            st.creation_time = get("creationTime").map_or(0.0f64, |v| v.double_value());
            st.hovers = get("hovers").map_or(false, |v| v.bool_value());
            st.item_type = get("itemType").map_or(0, |v| v.int_value());
            st.data_a = get("dataA").map_or(0, |v| v.int_value()) as u16;
            st.data_b = get("dataB").map_or(0, |v| v.int_value()) as u16;
            st.sub_items = Some(get("subItems").map_or_else(Vec::new, sub_items_from_plist));
            st.dynamic_object_save_dict = match get("dynamicObjectSaveDict") {
                Some(PlistValue::Dict(d)) => Some(d.clone()),
                _ => None,
            };
        }
        me.set_fp(Vector2 { x: fv("floatPos[VX]"), y: fv("floatPos[VY]") });
        let pid = get("priorityBlockheadUinqueID").map_or(0, |v| v.int_value());
        if pid != 0 {
            let b = dw.blockhead_with_id_including_net(pid as i64 as u64);
            me.st.borrow_mut().priority_blockhead = b;
        }
        me.init_sub_derived_objects();
        let hovers = me.st.borrow().hovers;
        if hovers {
            let now = world.world_time();
            let (creation_time, item_type) = {
                let st = me.st.borrow();
                (st.creation_time, st.item_type)
            };
            if (now - creation_time) > 900.0f64
                && item_type != ITEM_PINE_CONE
                && !is_pole_item(item_type)
                && !item_type_is_interaction_object(ItemType(item_type as u32))
                && !TRAIN_ITEMS.contains(&item_type)
                && !item_type_requires_ownership_to_remove(ItemType(item_type as u32))
            {
                let over = ((now - creation_time) - 900.0f64) as f32;
                {
                    let mut st = me.st.borrow_mut();
                    st.hovers = false;
                    st.creation_time = now - (over as f64);
                }
                if over < 900.0f32 {
                    let mut sanity = 0;
                    while sanity < 16 {
                        let p = me.pos();
                        let t = tile_at_world_position_loaded(p.x, p.y - 1, world);
                        if t.is_none() || !tile_is_air(t) {
                            break;
                        }
                        let mut fp = me.fp();
                        fp.y -= 1.0f32;
                        me.set_fp(fp);
                        me_obj.update_position(&me_obj, IntPair { x: p.x, y: p.y - 1 });
                        sanity += 1;
                    }
                }
                me.set_update_needs_to_be_sent(true);
                me.notify_changed();
            }
        }
        Some(me_obj)
    }

    /// `-initWithWorld:dynamicWorld:cache:netData:`: the avoid-list variant with an empty temporary list.
    pub fn from_net_data(world: &Rc<World>, dw: &Rc<DynamicWorld>, cache: CacheRef, net_data: &[u8]) -> Option<ObjRef> {
        let mut avoid: VecDeque<u64> = VecDeque::new();
        Self::from_net_data_avoid_freeblock_dupe_object_ids(world, dw, cache, net_data, &mut avoid)
    }

    /// `-initWithWorld:dynamicWorld:cache:netData:avoidFreeblockDupeObjectIds:` (packet 0x14 on the
    /// server).
    pub fn from_net_data_avoid_freeblock_dupe_object_ids(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        cache: CacheRef,
        net_data: &[u8],
        avoid: &mut VecDeque<u64>,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        let dref: DynamicWorldRef = dw.clone();
        if !base_init_with_world_dynamic_world_cache_net_data(&me_obj, &wref, &dref, cache, net_data) {
            return None;
        }
        let nd = FreeblockCreationNetData::read_le(&padded_bytes(net_data, 0x38));
        let item_type = nd.item_type as i32;
        me.st.borrow_mut().item_type = item_type;
        if item_type == ITEM_TIME_CRYSTAL {
            me_obj.remove_from_macro_block(&me_obj);
            me_obj.dealloc(&me_obj);
            return None;
        }
        let hovers = nd.hovers != 0;
        {
            let mut st = me.st.borrow_mut();
            st.data_a = nd.data_a;
            st.data_b = nd.data_b;
            st.hovers = hovers;
            st.creation_sound_play_time = nd.creation_sound_play_time;
            st.sound_type = nd.sound_type as i32;
            // (sic) creationTime takes the sound-play-time value
            st.creation_time = nd.creation_sound_play_time as f64;
            st.x_velocity = (nd.x_velocity as i32 as f32) / 255.0f32;
            st.y_velocity = (nd.y_velocity as i32 as f32) / 255.0f32;
            // nd.fall_speed is NOT read: fall_speed stays 0.0
        }
        if !hovers {
            let r = r01_tagged("free_block::from_net_data::jitter");
            let mut fp = me.fp();
            fp.x += r * 0.5f32 - 0.25f32;
            me.set_fp(fp);
        }
        me.st.borrow_mut().bounce_timer = 1.0f32;
        let mut fp = me.fp();
        fp.y = 0.5f32 + me.pos().y as f32;
        me.set_fp(fp);
        if nd.has_extra_data != 0 {
            let extra: &[u8] = if net_data.len() > 0x38 { &net_data[0x38..] } else { &[] };
            let dict = property_list_from_data(extra);
            if let Some(d) = &dict {
                if let Some(saved) = d.object_for_key("subItems") {
                    me.st.borrow_mut().sub_items = Some(sub_items_from_plist(saved));
                }
                let sd = match d.object_for_key("dynamicObjectSaveDict") {
                    Some(PlistValue::Dict(sd)) => Some(sd.clone()),
                    _ => None,
                };
                me.st.borrow_mut().dynamic_object_save_dict = sd;
            }
            let sd = me.st.borrow().dynamic_object_save_dict.clone();
            if let Some(sd) = sd {
                if nd.priority_blockhead_uinque_id != 0 {
                    let dup_id = sd.map.get(&"uniqueID".to_string()).map_or(0u64, |v| v.unsigned_long_value());
                    let mut ok = true;
                    if !avoid.is_empty() && dup_id != 0 {
                        ok = !avoid.contains(&dup_id);
                    }
                    if !ok {
                        // virtual call: a hovering pole item reaches `pole_item_taken` (defect kept, 8)
                        me_obj.remove_from_macro_block(&me_obj);
                        me_obj.dealloc(&me_obj);
                        return None;
                    }
                    if dup_id != 0 {
                        avoid.push_back(dup_id);
                        if avoid.len() > 32 {
                            avoid.pop_front();
                        }
                    }
                }
            }
        }
        if !dw.is_client() {
            me.base.borrow_mut().is_net = false;
        }
        if nd.priority_blockhead_uinque_id != 0 {
            let b = dw.blockhead_with_id_including_net(nd.priority_blockhead_uinque_id);
            me.st.borrow_mut().priority_blockhead = b;
        }
        me.init_sub_derived_objects();
        Some(me_obj)
    }

    // ------------------------------------------------------------------ net blobs

    /// Shared builder of the 0x38-byte creation blob plus the (uncompressed) extra plist.
    #[allow(clippy::too_many_arguments)]
    fn build_creation_blob(
        dyn_nd: DynamicObjectNetData,
        priority_id: u64,
        item_type: i32,
        data_a: u16,
        data_b: u16,
        fall_speed: f32,
        x_velocity: f32,
        y_velocity: f32,
        creation_sound_play_time: f32,
        sound_type: i32,
        hovers: bool,
        dict: Option<&PlistDict>,
        sub_items: Option<&[SubSlot]>,
    ) -> Vec<u8> {
        let non_empty_subs = sub_items.map_or(false, |s| !s.is_empty());
        let has_extra = dict.is_some() || non_empty_subs;
        let nd = FreeblockCreationNetData {
            dynamic_object_net_data: dyn_nd,
            priority_blockhead_uinque_id: priority_id,
            item_type: item_type as u16,
            data_a,
            data_b,
            fall_speed: f32_to_i16(fall_speed * 255.0f32),
            x_velocity: f32_to_i16(x_velocity * 255.0f32),
            y_velocity: f32_to_i16(y_velocity * 255.0f32),
            creation_sound_play_time,
            sound_type: sound_type as u8,
            hovers: hovers as u8,
            has_extra_data: has_extra as u8,
            padding: [0; 5],
        };
        let mut data = nd.to_vec();
        if has_extra {
            let mut extra = new_mutable_dict();
            if let Some(d) = dict {
                extra.map.insert("dynamicObjectSaveDict".to_string(), PlistValue::Dict(d.clone()));
            }
            if let Some(s) = sub_items {
                if !s.is_empty() {
                    extra.map.insert("subItems".to_string(), sub_items_to_plist(s));
                }
            }
            if let Some(bytes) = data_from_property_list(&PlistValue::Dict(extra)) {
                data.extend_from_slice(&bytes);
            }
        }
        data
    }

    /// `+[FreeBlock creationNetDataForFreeblockAtPosition:ofType:dataA:dataB:subItems:dynamicObjectSaveDict:hovers:priorityBlockhead:soundType:creationSoundPlayTime:]`
    /// Client-branch helper, never reached on the dedicated server.
    #[allow(clippy::too_many_arguments)]
    pub fn creation_net_data_for_freeblock_at_position(
        pos: IntPair,
        item_type: i32,
        data_a: u16,
        data_b: u16,
        sub_items: Option<&[SubSlot]>,
        dynamic_object_save_dict: Option<&PlistDict>,
        hovers: bool,
        priority_blockhead: Option<&ObjRef>,
        sound_type: i32,
        creation_sound_play_time: f32,
    ) -> Vec<u8> {
        let dyn_nd = DynamicObjectNetData::new(u64::MAX, pos.x as u32, pos.y as u32, false);
        Self::build_creation_blob(
            dyn_nd,
            priority_blockhead.map_or(0, |b| b.unique_id()),
            item_type,
            data_a,
            data_b,
            0.0,
            0.0,
            0.0,
            creation_sound_play_time,
            sound_type,
            hovers,
            dynamic_object_save_dict,
            sub_items,
        )
    }

    // ------------------------------------------------------------------ accessors

    /// `-itemType`
    pub fn item_type(&self) -> i32 {
        self.st.borrow().item_type
    }
    /// `-setItemType:`
    pub fn set_item_type(&self, v: i32) {
        self.st.borrow_mut().item_type = v;
    }
    /// `-dataA`
    pub fn data_a(&self) -> u16 {
        self.st.borrow().data_a
    }
    /// `-setDataA:`
    pub fn set_data_a(&self, v: u16) {
        self.st.borrow_mut().data_a = v;
    }
    /// `-dataB`
    pub fn data_b(&self) -> u16 {
        self.st.borrow().data_b
    }
    /// `-setDataB:`
    pub fn set_data_b(&self, v: u16) {
        self.st.borrow_mut().data_b = v;
    }
    /// `-subItems` (same slots, no deep copy).
    pub fn sub_items(&self) -> Option<Vec<SubSlot>> {
        self.st.borrow().sub_items.clone()
    }
    /// `-priorityBlockhead`
    pub fn priority_blockhead(&self) -> Option<ObjRef> {
        self.st.borrow().priority_blockhead.clone()
    }
    /// `-dynamicObjectSaveDict`
    pub fn dynamic_object_save_dict(&self) -> Option<PlistDict> {
        self.st.borrow().dynamic_object_save_dict.clone()
    }
    /// `-hovers`
    pub fn hovers(&self) -> bool {
        self.st.borrow().hovers
    }
    /// `-setHovers:` (no dirty marking).
    pub fn set_hovers(&self, v: bool) {
        self.st.borrow_mut().hovers = v;
    }
    /// `-creationSoundPlayTime`
    pub fn creation_sound_play_time(&self) -> f32 {
        self.st.borrow().creation_sound_play_time
    }
    /// `-setCreationSoundPlayTime:`
    pub fn set_creation_sound_play_time(&self, v: f32) {
        self.st.borrow_mut().creation_sound_play_time = v;
    }
    /// `-soundType`
    pub fn sound_type(&self) -> i32 {
        self.st.borrow().sound_type
    }
    /// `-setSoundType:`
    pub fn set_sound_type(&self, v: i32) {
        self.st.borrow_mut().sound_type = v;
    }

    // ------------------------------------------------------------------ misc selectors

    /// `-priorityBlockheadCannotPickup`.
    pub fn priority_blockhead_cannot_pickup(&self, _me: &ObjRef) {
        self.st.borrow_mut().priority_blockhead = None;
        self.set_update_needs_to_be_sent(true);
        self.notify_changed();
    }

    /// `-falling`: `hovers ? NO: tileIsAirOrSnow(tile below)` (never loads).
    pub fn falling(&self) -> bool {
        if self.hovers() {
            return false;
        }
        let world = self.world();
        let p = self.pos();
        let t = tile_at_world_position(p.x, p.y - 1, &world.macro_tiles(), &world);
        tile_is_air_or_snow(t)
    }

    /// `-hitLava`: flag removed, then three `rand()` draws (z-jitter, vel.x, vel.y), then the empty
    /// particle call (the sound is a no-op).
    pub fn hit_lava(&self, me: &ObjRef) {
        me.set_needs_removed(me, true);
        let fp = self.fp();
        let hit_point = (fp.x, fp.y - 0.5f32, -0.3f32);
        let color = Vector::new4(0.4f32, 0.2f32, 0.2f32, 0.5f32);
        let emitter = crate::env::particle_emitter::ParticleEmitter::instance();
        // argument evaluation order: position jitter, velocity x, velocity y
        let z = hit_point.2 + (r01_tagged("free_block::hit_lava::z") - 0.5f32);
        let vx = r01_tagged("free_block::hit_lava::vx") - 0.5f32;
        let vy = r01_tagged("free_block::hit_lava::vy") * 0.5f32 + 0.5f32;
        emitter.add_particle_at_pos_velocity_color_gravity_type_life_scale(
            Vector::new(hit_point.0, hit_point.1, z),
            Vector::new(vx, vy, 0.0),
            color,
            0,
            1.0f32,
            12.0f32,
        );
    }

    /// `wrapDelta(a, b)` of `update`: shortest signed distance on the wrapped world.
    fn wrap_delta(&self, a: f32, b: f32) -> f32 {
        let w = self.world().world_width_macro();
        let d = a - b;
        let ww = w.wrapping_shl(5);
        if d < (ww / 2) as f32 {
            if (w.wrapping_mul(-32) / 2) as f32 <= d {
                d
            } else {
                d + ww as f32
            }
        } else {
            d - ww as f32
        }
    }

    /// `-update:accurateDT:isSimulation:`: the whole simulation (sections A to F of).
    fn update_impl(&self, me: &ObjRef, dt: f32) {
        if me.needs_removed() {
            return;
        }
        let world = self.world();
        let dw = self.dw();
        let now = world.world_time();
        let creation_time = self.st.borrow().creation_time;
        let t_el = (now as f32 as f64) - creation_time;
        let is_net = me.is_net();
        let item_type = self.item_type();

        // ---- A. lifetime / hover release (server-owned blocks only) ----
        if !is_net {
            if is_pole_item(item_type) || is_special_non_pole(item_type) {
                if t_el > 27000.0f64 && !self.hovers() {
                    me.set_needs_removed(me, true);
                    return;
                }
            } else if t_el <= 900.0f64 {
                if self.hovers() && (world.wind_strength() as f64) > 0.9f64 {
                    self.set_hovers(false);
                    self.set_update_needs_to_be_sent(true);
                    self.notify_changed();
                }
            } else {
                if !self.hovers() || item_type == ITEM_PINE_CONE {
                    me.set_needs_removed(me, true);
                    return;
                }
                self.set_hovers(false);
                self.st.borrow_mut().creation_time = world.world_time();
                self.set_update_needs_to_be_sent(true);
                self.notify_changed();
            }
        }

        // ---- B. current tile ----
        let p = self.pos();
        let mut tile: Option<TileRef> = tile_at_world_position(p.x, p.y, &world.macro_tiles(), &world);
        if tile.is_none() {
            if is_net {
                return;
            }
            tile = tile_at_world_position_loaded(p.x, p.y, &world);
        }

        // ---- C. horizontal / unstuck logic, only when no priorityBlockhead ----
        if self.priority_blockhead().is_none() {
            if !tile_is_solid(tile.clone()) || tile_contains_door(tile.clone()) || tile_contains_trap_door(tile.clone())
            {
                if !tile_is_water(tile.clone()) {
                    self.st.borrow_mut().x_velocity_water = 0.0;
                } else {
                    let p = self.pos();
                    let right = tile_at_world_position_loaded(p.x + 1, p.y, &world);
                    let right_is_water = tile_is_water(right.clone());
                    let xvw = self.st.borrow().x_velocity_water;
                    if right_is_water || xvw.abs() > 0.01f32 {
                        let slope = if right_is_water {
                            let r = right.as_ref().map_or(0u32, |t| t.partial_content_left() as u32);
                            let c = tile.as_ref().map_or(0u32, |t| t.partial_content_left() as u32);
                            r.wrapping_sub(c) as i32 as f32
                        } else {
                            0.0f32
                        };
                        let damp = min_d(0.5f64, (10.0f32 * dt) as f64);
                        let step = min_d(0.9f64, dt as f64);
                        let mut xvw = ((xvw as f64) * (1.0f64 - damp) - (slope as f64) * step) as f32;
                        self.st.borrow_mut().x_velocity_water = xvw;
                        if xvw.abs() >= 0.001f32 {
                            if xvw <= 0.0f32 {
                                let left = tile_at_world_position_loaded(p.x - 1, p.y, &world);
                                if tile_is_solid(left) && self.fp().x <= 0.5f32 + p.x as f32 {
                                    xvw = 0.0;
                                }
                            } else if tile_is_solid(right.clone()) && 0.5f32 + p.x as f32 <= self.fp().x {
                                xvw = 0.0;
                            }
                            self.st.borrow_mut().x_velocity_water = xvw;
                            if xvw.abs() >= 0.001f32 {
                                let mut fp = self.fp();
                                fp.x = ((fp.x as f64) + (xvw as f64) * min_d(0.9f64, dt as f64)) as f32;
                                self.set_fp(fp);
                                let nx = f32_to_i32(fp.x);
                                if nx != p.x {
                                    let t2 = tile_at_world_position_loaded(nx, p.y, &world);
                                    if !tile_is_solid(t2.clone()) {
                                        me.update_position(me, IntPair { x: nx, y: p.y });
                                    } else {
                                        if t2.as_ref().map_or(false, |t| t.type_index() == TILE_LAVA) {
                                            self.hit_lava(me);
                                            return;
                                        }
                                        let mut fp = self.fp();
                                        fp.x = p.x as f32 + 0.01f32;
                                        self.set_fp(fp);
                                    }
                                }
                            }
                        } else {
                            self.st.borrow_mut().x_velocity_water = 0.0;
                        }
                    }
                }
            } else {
                // embedded in a solid tile
                if tile.as_ref().map_or(false, |t| t.type_index() == TILE_LAVA) {
                    self.hit_lava(me);
                    return;
                }
                self.st.borrow_mut().fall_speed = 0.0;
                let mut moved = false;
                let p = self.pos();
                let free = |x: i32, y: i32| -> bool {
                    let t = tile_at_world_position_loaded(x, y, &world);
                    t.is_some()
                        && !tile_is_solid(t.clone())
                        && !tile_contains_door(t.clone())
                        && !tile_contains_trap_door(t)
                };
                if free(p.x, p.y + 1) {
                    let mut fp = self.fp();
                    fp.y += 1.0f32;
                    self.set_fp(fp);
                    me.update_position(me, IntPair { x: p.x, y: p.y + 1 });
                    moved = true;
                } else if free(p.x, p.y - 1) {
                    let mut fp = self.fp();
                    fp.y -= 1.0f32;
                    self.set_fp(fp);
                    me.update_position(me, IntPair { x: p.x, y: p.y - 1 });
                    moved = true;
                } else if free(p.x + 1, p.y) {
                    let mut fp = self.fp();
                    fp.x += 1.0f32;
                    self.set_fp(fp);
                    me.update_position(me, IntPair { x: p.x + 1, y: p.y });
                    moved = true;
                } else if free(p.x - 1, p.y) {
                    let mut fp = self.fp();
                    fp.x -= 1.0f32;
                    self.set_fp(fp);
                    me.update_position(me, IntPair { x: p.x - 1, y: p.y });
                    moved = true;
                }
                if moved {
                    self.notify_changed();
                }
                self.st.borrow_mut().x_velocity_water = 0.0;
            }
        }

        // ---- D. hover support check ----
        if self.hovers() && !is_net && !is_pole_item(item_type) {
            match &tile {
                Some(t) => {
                    if t.contents() == 0 {
                        self.set_hovers(false);
                        self.set_update_needs_to_be_sent(true);
                    }
                }
                // UB-PARITY: the original dereferences `tile->contents` of a NULL tile (SIGSEGV).
                None => ub_null_tile_deref(),
            }
        }

        // ---- E. vertical motion (no priority blockhead) or magnet (priority blockhead) ----
        let priority = self.priority_blockhead();
        if priority.is_none() {
            let a = min_d(0.9f64, dt as f64);
            let k1 = 1.0f64 - a * 1.0f64;
            let k = if k1 <= 0.0f64 { 0.0f64 } else { k1 };
            {
                let mut st = self.st.borrow_mut();
                st.x_velocity *= k as f32;
                st.y_velocity *= k as f32;
            }
            let is_water = tile_is_water(tile.clone());
            let mut falling = false;
            let mut air_below = false;
            let mut air_or_water_above = false;
            let p = self.pos();
            if !self.hovers() {
                if !is_water {
                    let below = tile_at_world_position_loaded(p.x, p.y - 1, &world);
                    air_below = tile_is_air_or_snow(below.clone());
                    if air_below || (0.25f32 + p.y as f32) < self.fp().y {
                        falling = true;
                    }
                    if !air_below && below.as_ref().map_or(false, |t| t.type_index() == TILE_LAVA) {
                        self.hit_lava(me);
                        return;
                    }
                } else {
                    let above = tile_at_world_position_loaded(p.x, p.y + 1, &world);
                    air_or_water_above = tile_is_air_water_or_snow(above);
                    if air_or_water_above || self.fp().y < 0.75f32 + p.y as f32 {
                        falling = true;
                    }
                }
            }
            if falling {
                let c = min_d(0.5f64, dt as f64);
                let mut fall_speed = ((self.st.borrow().fall_speed as f64) * (1.0f64 - c)) as f32;
                let in_air_or_above_surface = !tile_is_water(tile.clone())
                    || ((tile.as_ref().map_or(0u8, |t| t.partial_content_left()) as f32) / 255.0f32
                        <= self.fp().y - p.y as f32);
                if in_air_or_above_surface {
                    fall_speed = ((fall_speed as f64) + min_d(0.9f64, dt as f64) * 0.20000000298023224f64) as f32;
                    if 0.2f32 <= fall_speed {
                        fall_speed = 0.2f32;
                    }
                } else {
                    if 0.0f32 < fall_speed {
                        fall_speed = 0.9f32 * fall_speed;
                    }
                    fall_speed = ((fall_speed as f64) - min_d(0.9f64, dt as f64) * 0.10000000149011612f64) as f32;
                    if fall_speed <= -0.05f32 {
                        fall_speed = -0.05f32;
                    }
                }
                let mut fp = self.fp();
                fp.y -= fall_speed;
                self.set_fp(fp);
                self.st.borrow_mut().fall_speed = fall_speed;
                if (!is_water && air_below) || (is_water && air_or_water_above) {
                    if f32_to_i32(fp.y) != p.y {
                        let np = IntPair { x: p.x, y: f32_to_i32(fp.y) };
                        let t3 = tile_at_world_position_loaded(np.x, np.y, &world);
                        if !tile_is_solid(t3.clone()) {
                            me.update_position(me, np);
                        } else {
                            if t3.as_ref().map_or(false, |t| t.type_index() == TILE_LAVA) {
                                self.hit_lava(me);
                                return;
                            }
                            let mut fp = self.fp();
                            fp.y += fall_speed;
                            self.set_fp(fp);
                            self.st.borrow_mut().fall_speed = 0.0;
                        }
                    }
                } else if is_water || fp.y > 0.251f32 + p.y as f32 {
                    if is_water && fp.y >= 0.749f32 + p.y as f32 {
                        {
                            let mut st = self.st.borrow_mut();
                            st.fall_speed = 0.0;
                            st.bounce_timer = 0.0;
                        }
                        let mut fp = self.fp();
                        fp.y = 0.75f32 + p.y as f32;
                        self.set_fp(fp);
                        self.notify_changed();
                    }
                } else {
                    // landed on ground
                    {
                        let mut st = self.st.borrow_mut();
                        st.fall_speed = 0.0;
                        st.bounce_timer = 0.0;
                    }
                    let mut fp = self.fp();
                    fp.y = 0.25f32 + p.y as f32;
                    self.set_fp(fp);
                    self.notify_changed();
                }
            }
        } else {
            // `priority.is_none()` was false above, so it is set here.
            let pb = priority.expect("a priority blockhead");
            if !pb.needs_removed() {
                let bfp = pb.float_pos();
                let goal = Vector2 { x: bfp.x + 0.0f32, y: bfp.y + 0.5f32 };
                let cur = self.fp();
                if dt <= 0.5f32 {
                    let d1 = 1.0f32 - 1.0f32 * dt;
                    let damp = if d1 <= 0.0f32 { 0.0f32 } else { d1 };
                    let dx = self.wrap_delta(goal.x, cur.x);
                    let (xv, yv) = {
                        let mut st = self.st.borrow_mut();
                        st.x_velocity = st.x_velocity * damp + dx * dt * 2.0f32;
                        st.y_velocity = st.y_velocity * damp + (goal.y - cur.y) * dt * 2.0f32;
                        (st.x_velocity, st.y_velocity)
                    };
                    self.set_fp(Vector2 { x: dt * xv * 5.0f32 + cur.x, y: dt * yv * 5.0f32 + cur.y });
                } else {
                    self.set_fp(goal);
                }
                let fp = self.fp();
                let p = self.pos();
                if f32_to_i32(fp.x) != p.x || f32_to_i32(fp.y) != p.y {
                    me.update_position(me, IntPair { x: f32_to_i32(fp.x), y: f32_to_i32(fp.y) });
                    self.notify_changed();
                }
            } else {
                self.st.borrow_mut().priority_blockhead = None;
                // note: updateNeedsToBeSent is NOT set here
                self.notify_changed();
            }
        }

        // ---- F. pickup scan ----
        let host = crate::blockhead::blockhead::host();
        for bh in dw.local_and_disconnected_client_blockheads() {
            if let Some(pbh) = self.priority_blockhead() {
                if !handle::ptr_eq(&pbh, &bh) {
                    continue;
                }
            }
            let Some(b) = crate::blockhead::blockhead::as_blockhead(&bh) else { continue };
            let bp = bh.float_pos();
            let r = host.free_block_pickup_radius(b);
            let fp = self.fp();
            let dxw = self.wrap_delta(fp.x, bp.x);
            if dxw < r && (-r < self.wrap_delta(fp.x, bp.x)) && (bp.y - r) < fp.y && fp.y < ((1.5f32 + bp.y) + r) {
                // sound (gem pitch, delay, maxSoundPlay) is client presentation only
                let _ = host.pickup_freeblock_if_possible_in_tile_intentional(b, me, tile.clone(), false);
            }
        }
    }
}

impl DynamicObject for FreeBlock {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &FREE_BLOCK_CLASS
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    /// `-objectType`
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_FREE_BLOCK
    }
    /// `-shouldSaveEveryChangeInPosition`
    fn should_save_every_change_in_position(&self) -> bool {
        false
    }
    /// `-updatePosition:`: `freeblockPositionChanged:oldPos:` when the cell changed.
    fn update_position(&self, me: &ObjRef, new_position: IntPair) {
        let old = self.pos();
        base_update_position(me, new_position);
        if old.x != new_position.x || old.y != new_position.y {
            self.dw().freeblock_position_changed_old_pos(me, old);
        }
    }
    /// `-removeFromMacroBlock`
    fn remove_from_macro_block(&self, me: &ObjRef) {
        base_remove_from_macro_block(me);
        let (hovers, item) = {
            let st = self.st.borrow();
            (st.hovers, st.item_type)
        };
        if hovers && is_pole_item(item) {
            self.dw().pole_item_taken(item);
        }
    }
    /// `-getSaveDict`.
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = base_get_save_dict(me);
        let fp = self.fp();
        let st = self.st.borrow();
        d.map.insert("bounceTimer".to_string(), PlistValue::real_f32(st.bounce_timer));
        d.map.insert("fallSpeed".to_string(), PlistValue::real_f32(st.fall_speed));
        d.map.insert("floatPos[VX]".to_string(), PlistValue::real_f32(fp.x));
        d.map.insert("floatPos[VY]".to_string(), PlistValue::real_f32(fp.y));
        d.map.insert("itemType".to_string(), PlistValue::int(st.item_type as i64));
        d.map.insert("dataA".to_string(), PlistValue::int(st.data_a as i64));
        d.map.insert("dataB".to_string(), PlistValue::int(st.data_b as i64));
        d.map.insert("creationTime".to_string(), PlistValue::real_f64(st.creation_time));
        d.map.insert("hovers".to_string(), PlistValue::boolean(st.hovers));
        if let Some(subs) = &st.sub_items {
            d.map.insert("subItems".to_string(), sub_items_to_plist(subs));
        }
        if let Some(sd) = &st.dynamic_object_save_dict {
            d.map.insert("dynamicObjectSaveDict".to_string(), PlistValue::Dict(sd.clone()));
        }
        if let Some(p) = &st.priority_blockhead {
            d.map.insert("priorityBlockheadUinqueID".to_string(), PlistValue::int((p.unique_id() as i32) as i64));
        }
        d
    }
    /// `-updateNetDataForClient:`: 0x28 bytes, padding zero.
    fn update_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        let nd = base_dynamic_object_net_data(me);
        let st = self.st.borrow();
        let u = FreeblockUpdateNetData {
            dynamic_object_net_data: nd,
            priority_blockhead_uinque_id: st.priority_blockhead.as_ref().map_or(0, |p| p.unique_id()),
            fall_speed: f32_to_i16(st.fall_speed * 255.0f32),
            x_velocity: f32_to_i16(st.x_velocity * 255.0f32),
            y_velocity: f32_to_i16(st.y_velocity * 255.0f32),
            hovers: st.hovers as u8,
            padding: [0],
        };
        Some(u.to_vec())
    }
    /// `-creationNetDataForClient:`: 0x38 bytes + uncompressed XML plist when `hasExtraData`.
    fn creation_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        let nd = base_dynamic_object_net_data(me);
        let st = self.st.borrow();
        Some(Self::build_creation_blob(
            nd,
            st.priority_blockhead.as_ref().map_or(0, |p| p.unique_id()),
            st.item_type,
            st.data_a,
            st.data_b,
            st.fall_speed,
            st.x_velocity,
            st.y_velocity,
            st.creation_sound_play_time,
            st.sound_type,
            st.hovers,
            st.dynamic_object_save_dict.as_ref(),
            st.sub_items.as_deref(),
        ))
    }
    /// `-remoteUpdate:`
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        base_remote_update(me, net_data);
        let nd = FreeblockUpdateNetData::read_le(&padded_bytes(net_data, 0x28));
        if me.is_net() {
            let mut st = self.st.borrow_mut();
            st.fall_speed = (nd.fall_speed as i32 as f32) / 255.0f32;
            st.x_velocity = (nd.x_velocity as i32 as f32) / 255.0f32;
            st.y_velocity = (nd.y_velocity as i32 as f32) / 255.0f32;
            st.hovers = nd.hovers != 0;
        }
        let mut st = self.st.borrow_mut();
        if st.priority_blockhead.is_some() && nd.priority_blockhead_uinque_id == 0 {
            st.priority_blockhead = None;
        }
    }
    /// `-update:accurateDT:isSimulation:` (`accurateDT` and `isSimulation` are unused).
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, _accurate_dt: f32, _is_simulation: bool) {
        self.update_impl(me, dt);
    }
    /// `-dealloc`: Rust drops the retained members; `poleItemTaken` is NOT in dealloc.
    fn dealloc(&self, _me: &ObjRef) {}
}

#[cfg(test)]
pub(crate) mod testkit {
    //! Shared test harness for the physics_objects tests: a real `World` with flat ground and a real `DynamicWorld`.
    use super::*;
    use crate::dynworld::state::testutil::{dbs, scratch};
    use crate::dynworld::state::DynamicWorldInit;
    use crate::lighting::artificial_light::testkit::{install_rec, tile, world_with_blocks, Rec};

    /// macro tiles wide; rows y in 0..32 loaded; ground (type 6) for `y <= ground_y`, air above.
    pub fn flat(name: &str, ground_y: i32) -> (Rc<World>, Rc<DynamicWorld>, Rc<Rec>) {
        let rec = install_rec();
        let w = world_with_blocks(4, &[(0, 0), (1, 0), (2, 0), (3, 0)]);
        for x in 0..128 {
            for y in 0..=ground_y {
                tile(&w, x, y).update(|t| t.type_index = 6);
            }
        }
        let (main, dwdb) = dbs(name);
        let dir = scratch(&format!("{name}_save"));
        let dw = DynamicWorld::new(DynamicWorldInit {
            world: w.clone(),
            world_tile_loader: None,
            server: None,
            server_clients: None,
            cache: None,
            tree_density_noise_function: None,
            season_offset_noise_function: None,
            app_database: None,
            world_database: main,
            dynamic_object_database: dwdb,
            world_save_directory_override: Some(dir),
        });
        (w, dw, rec)
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;
    use crate::lighting::artificial_light::testkit::tile;
    use crate::rng::glibc_rand;

    fn mk(w: &Rc<World>, dw: &Rc<DynamicWorld>, x: i32, y: i32, item: i32, hovers: bool) -> ObjRef {
        FreeBlock::new(w, dw, IntPair { x, y }, item, 0, 0, None, None, None, hovers, None).unwrap()
    }

    fn fb(o: &ObjRef) -> &FreeBlock {
        as_free_block(o).unwrap()
    }

    /// : construction draws jitter + rotation (2 draws), 1 draw when hovering.
    #[test]
    fn construction_draws_and_initial_state() {
        let (w, dw, _r) = flat("fb_ctor", 5);
        glibc_rand::srand(1);
        let o = mk(&w, &dw, 10, 8, 5, false);
        let r1 = 1804289383i32;
        let r2 = 846930886i32;
        let j = (r1 as f32) / 2147483648.0f32;
        assert_eq!(o.float_pos().x, (10.5f32 + (j * 0.5f32 - 0.25f32)));
        assert_eq!(o.float_pos().y, 8.5f32);
        let rot = ((((r2 as f32) / 2147483648.0f32) as f64 * 3.141592653589793f64) * 2.0f64) as f32;
        assert_eq!(fb(&o).st.borrow().rotation, rot);
        assert_eq!(fb(&o).st.borrow().fall_speed, -0.05f32);
        assert_eq!(fb(&o).st.borrow().y_velocity, 0.5f32);
        assert_eq!(fb(&o).st.borrow().bounce_timer, 1.0f32);
        glibc_rand::srand(1);
        let _h = mk(&w, &dw, 12, 8, 5, true);
        assert_eq!(glibc_rand::rand(), r2);
        // time-crystal sub-items are dropped; hasItems false -> None
        let it = InventoryItem::new(ItemType(11), 0, 0, None, None);
        let slots: Vec<SubSlot> = vec![Rc::new(RefCell::new(vec![it]))];
        let o2 =
            FreeBlock::new(&w, &dw, IntPair { x: 3, y: 8 }, 5, 0, 0, Some(&slots), None, None, true, None).unwrap();
        assert!(fb(&o2).sub_items().is_none());
    }

    /// : a free block falls and lands at pos.y + 0.25 with fall_speed 0.
    #[test]
    fn falls_and_lands() {
        let (w, dw, _r) = flat("fb_fall", 5);
        let o = mk(&w, &dw, 10, 12, 5, true);
        fb(&o).set_hovers(false);
        let start = o.float_pos();
        let mut landed = false;
        for _ in 0..400 {
            o.update_accurate_dt_is_simulation(&o, 0.2, 0.2, false);
            if fb(&o).st.borrow().bounce_timer == 0.0 {
                landed = true;
                break;
            }
        }
        assert!(landed);
        assert!(o.float_pos().y < start.y);
        assert_eq!(o.pos().y, 6);
        assert_eq!(o.float_pos().y, 6.25f32);
        assert_eq!(fb(&o).st.borrow().fall_speed, 0.0);
        assert!(fb(&o).falling() == false);
    }

    /// Bit-exact spot check (item 15): fall_speed = -0.049, dt = 0.2 in air above the surface.
    #[test]
    fn fall_speed_bits() {
        let (w, dw, _r) = flat("fb_bits", 5);
        let o = mk(&w, &dw, 10, 20, 5, true);
        fb(&o).set_hovers(false);
        fb(&o).st.borrow_mut().fall_speed = -0.049f32;
        o.update_accurate_dt_is_simulation(&o, 0.2, 0.2, false);
        let dt = 0.2f32;
        let c = min_d(0.5, dt as f64);
        let mut fs = ((-0.049f32 as f64) * (1.0 - c)) as f32;
        fs = ((fs as f64) + min_d(0.9, dt as f64) * 0.20000000298023224f64) as f32;
        assert_eq!(fb(&o).st.borrow().fall_speed.to_bits(), fs.to_bits());
    }

    /// item 3: lava consumes exactly 3 rand() calls and flags needs_removed.
    #[test]
    fn lava_three_draws() {
        let (w, dw, _r) = flat("fb_lava", 5);
        tile(&w, 10, 5).update(|t| t.type_index = TILE_LAVA);
        let o = mk(&w, &dw, 10, 6, 5, true);
        fb(&o).set_hovers(false);
        fb(&o).st.borrow_mut().fall_speed = 0.0;
        let mut p = o.float_pos();
        p.y = 6.25;
        o.base().borrow_mut().float_pos = p;
        glibc_rand::srand(7);
        o.update_accurate_dt_is_simulation(&o, 0.2, 0.2, false);
        assert!(o.needs_removed());
        glibc_rand::srand(7);
        for _ in 0..3 {
            glibc_rand::rand();
        }
        let after = glibc_rand::rand();
        glibc_rand::srand(7);
        // the update consumed exactly 3 draws: replay and compare the next value
        let (w2, dw2, _r2) = flat("fb_lava2", 5);
        tile(&w2, 10, 5).update(|t| t.type_index = TILE_LAVA);
        glibc_rand::srand(1);
        let o2 = mk(&w2, &dw2, 10, 6, 5, true);
        fb(&o2).set_hovers(false);
        fb(&o2).st.borrow_mut().fall_speed = 0.0;
        let mut p = o2.float_pos();
        p.y = 6.25;
        o2.base().borrow_mut().float_pos = p;
        glibc_rand::srand(7);
        o2.update_accurate_dt_is_simulation(&o2, 0.2, 0.2, false);
        assert_eq!(glibc_rand::rand(), after);
    }

    /// item 4: unstick moves to the first free of (x,y+1), (x,y-1), (x+1,y), (x-1,y).
    #[test]
    fn unstick_order() {
        let (w, dw, _r) = flat("fb_unstick", 5);
        let o = mk(&w, &dw, 10, 5, 5, true);
        fb(&o).set_hovers(false);
        o.update_accurate_dt_is_simulation(&o, 0.2, 0.2, false);
        assert_eq!(o.pos(), IntPair { x: 10, y: 6 });
        assert_eq!(fb(&o).st.borrow().x_velocity_water, 0.0);
    }

    /// item 5: hover expiry. Pole items survive; others at t_el > 900 drop (hover) or vanish (not hovering).
    #[test]
    fn hover_rules() {
        let (w, dw, _r) = flat("fb_hover", 5);
        w.set_world_time(0.0);
        let pole = mk(&w, &dw, 20, 10, 142, true);
        let plain = mk(&w, &dw, 24, 10, 5, true);
        let loose = mk(&w, &dw, 28, 10, 5, false);
        // give the hovering ones support so section D does not drop them
        tile(&w, 24, 10).update(|t| t.contents = 3);
        w.set_world_time(1000.0);
        pole.update_accurate_dt_is_simulation(&pole, 0.2, 0.2, false);
        assert!(!pole.needs_removed());
        assert!(fb(&pole).hovers());
        plain.update_accurate_dt_is_simulation(&plain, 0.2, 0.2, false);
        assert!(!plain.needs_removed());
        assert!(!fb(&plain).hovers());
        assert!(plain.update_needs_to_be_sent());
        assert_eq!(fb(&plain).st.borrow().creation_time, 1000.0);
        loose.update_accurate_dt_is_simulation(&loose, 0.2, 0.2, false);
        assert!(loose.needs_removed());
        w.set_world_time(28000.0);
        pole.update_accurate_dt_is_simulation(&pole, 0.2, 0.2, false);
        assert!(!pole.needs_removed()); // still hovering
        fb(&pole).set_hovers(false);
        pole.update_accurate_dt_is_simulation(&pole, 0.2, 0.2, false);
        assert!(pole.needs_removed());
    }

    /// item 7 and 8: save dict keys, round trip, creation / update blob sizes.
    #[test]
    fn save_round_trip_and_blobs() {
        let (w, dw, _r) = flat("fb_save", 5);
        let it = InventoryItem::new(ItemType(5), 0, 0, None, None);
        let slots: Vec<SubSlot> = vec![Rc::new(RefCell::new(vec![it]))];
        let mut sd = new_mutable_dict();
        sd.map.insert("uniqueID".to_string(), PlistValue::int(77));
        let o = FreeBlock::new(&w, &dw, IntPair { x: 40, y: 9 }, 9, 3, 4, Some(&slots), Some(sd), None, true, None)
            .unwrap();
        let d = o.get_save_dict(&o);
        for k in [
            "bounceTimer",
            "fallSpeed",
            "floatPos[VX]",
            "floatPos[VY]",
            "itemType",
            "dataA",
            "dataB",
            "creationTime",
            "hovers",
            "subItems",
            "dynamicObjectSaveDict",
            "floatPos",
            "pos_x",
            "pos_y",
            "uniqueID",
        ] {
            assert!(d.map.get(&k.to_string()).is_some(), "missing {k}");
        }
        assert!(d.map.get(&"priorityBlockheadUinqueID".to_string()).is_none());
        let r = FreeBlock::from_save_dict(&w, &dw, &d, None).unwrap();
        assert_eq!(fb(&r).item_type(), 9);
        assert_eq!(fb(&r).data_a(), 3);
        assert_eq!(fb(&r).data_b(), 4);
        assert!(fb(&r).hovers());
        assert_eq!(fb(&r).sub_items().unwrap().len(), 1);
        assert_eq!(r.float_pos(), o.float_pos());
        // saved without sub-items: Some(empty) after load
        let o2 = mk(&w, &dw, 41, 9, 9, true);
        let d2 = o2.get_save_dict(&o2);
        assert!(d2.map.get(&"subItems".to_string()).is_none());
        let r2 = FreeBlock::from_save_dict(&w, &dw, &d2, None).unwrap();
        assert!(fb(&r2).sub_items().unwrap().is_empty());
        // blobs
        let c = o.creation_net_data_for_client(&o, None).unwrap();
        assert!(c.len() > 0x38);
        assert_eq!(c[0x32], 1);
        assert!(c[0x33..0x38].iter().all(|b| *b == 0));
        let c2 = o2.creation_net_data_for_client(&o2, None).unwrap();
        // a free block with a priority blockhead names it
        let prio: ObjRef = crate::objects::base::testutil::TestObj::new();
        prio.base().borrow_mut().unique_id = 31337;
        fb(&o2).st.borrow_mut().priority_blockhead = Some(prio);
        let c3 = o2.creation_net_data_for_client(&o2, None).unwrap();
        assert_ne!(c3, c2);
        fb(&o2).st.borrow_mut().priority_blockhead = None;
        assert_eq!(c2.len(), 0x38);
        assert_eq!(c2[0x32], 0);
        let u = o.update_net_data_for_client(&o, None).unwrap();
        assert_eq!(u.len(), 0x28);
        assert_eq!(u[0x26], 1);
        // net init from the creation blob: item type 11 is rejected, normal one accepted (is_net forced false)
        let n = FreeBlock::from_net_data(&w, &dw, None, &c).unwrap();
        assert_eq!(fb(&n).item_type(), 9);
        assert!(!n.is_net());
        assert_eq!(fb(&n).sub_items().unwrap().len(), 1);
        let mut bad = c2.clone();
        bad[0x20] = 11;
        bad[0x21] = 0;
        assert!(FreeBlock::from_net_data(&w, &dw, None, &bad).is_none());
    }

    /// item 9: dupe suppression with the 32-entry window.
    #[test]
    fn dupe_list() {
        let (w, dw, _r) = flat("fb_dupe", 5);
        let mut sd = new_mutable_dict();
        sd.map.insert("uniqueID".to_string(), PlistValue::int(77));
        let blob = FreeBlock::creation_net_data_for_freeblock_at_position(
            IntPair { x: 30, y: 9 },
            9,
            0,
            0,
            None,
            Some(&sd),
            false,
            Some(&mk(&w, &dw, 1, 9, 5, true)),
            0,
            0.0,
        );
        let mut avoid = VecDeque::new();
        assert!(FreeBlock::from_net_data_avoid_freeblock_dupe_object_ids(&w, &dw, None, &blob, &mut avoid).is_some());
        assert!(FreeBlock::from_net_data_avoid_freeblock_dupe_object_ids(&w, &dw, None, &blob, &mut avoid).is_none());
        assert_eq!(avoid.len(), 1);
        for i in 0..33u64 {
            let mut sd2 = new_mutable_dict();
            sd2.map.insert("uniqueID".to_string(), PlistValue::int(1000 + i as i64));
            let b2 = FreeBlock::creation_net_data_for_freeblock_at_position(
                IntPair { x: 30, y: 9 },
                9,
                0,
                0,
                None,
                Some(&sd2),
                false,
                Some(&mk(&w, &dw, 1, 9, 5, true)),
                0,
                0.0,
            );
            assert!(FreeBlock::from_net_data_avoid_freeblock_dupe_object_ids(&w, &dw, None, &b2, &mut avoid).is_some());
        }
        assert_eq!(avoid.len(), 32);
        // id 77 was pushed out of the window: accepted again
        assert!(FreeBlock::from_net_data_avoid_freeblock_dupe_object_ids(&w, &dw, None, &blob, &mut avoid).is_some());
    }

    #[test]
    fn wrap_delta_values() {
        let (w, dw, _r) = flat("fb_wrap", 5);
        let o = mk(&w, &dw, 10, 8, 5, true);
        let f = fb(&o);
        assert_eq!(f.wrap_delta(1.0, 127.0), 2.0);
        assert_eq!(f.wrap_delta(127.0, 1.0), -2.0);
        assert_eq!(f.wrap_delta(5.0, 3.0), 2.0);
    }

    use crate::blockhead::blockhead::Blockhead;
    use crate::world::misc::{install_deps, NullDeps, WorldOpsDepsStub};
    use crate::world::state::Weather;

    fn bh_at(id: u64, x: f32, y: f32) -> ObjRef {
        let b = alloc_obj(Blockhead::alloc);
        b.base.borrow_mut().unique_id = id;
        b.base.borrow_mut().float_pos = Vector2 { x, y };
        b
    }

    fn water(w: &Rc<World>, x: i32, y: i32, fill: u8) {
        tile(w, x, y).update(|t| {
            t.type_index = 3;
            t.partial_content_left = fill;
        });
    }

    struct Windy;
    impl WorldOpsDepsStub for Windy {
        fn weather_wind_strength(&self) -> f32 {
            1.0
        }
    }

    /// Sub-item plumbing: time crystals are dropped, odd entries are skipped, every slot is kept.
    #[test]
    fn sub_item_filters() {
        let (w, dw, _r) = flat("fb_subs", 5);
        let crystal = InventoryItem::new(ItemType(ITEM_TIME_CRYSTAL as u32), 0, 0, None, None);
        let stone = InventoryItem::new(ItemType(5), 0, 0, None, None);
        let slots: Vec<SubSlot> =
            vec![Rc::new(RefCell::new(vec![crystal.clone(), stone])), Rc::new(RefCell::new(vec![]))];
        let o =
            FreeBlock::new(&w, &dw, IntPair { x: 10, y: 9 }, 9, 0, 0, Some(&slots), None, None, true, None).unwrap();
        assert_eq!(fb(&o).sub_items().unwrap().len(), 2);
        assert_eq!(fb(&o).sub_items().unwrap()[0].borrow().len(), 1);
        // only time crystals: no sub-items at all
        let only: Vec<SubSlot> = vec![Rc::new(RefCell::new(vec![crystal.clone()]))];
        let none =
            FreeBlock::new(&w, &dw, IntPair { x: 12, y: 9 }, 9, 0, 0, Some(&only), None, None, true, None).unwrap();
        assert!(fb(&none).sub_items().is_none());
        // the plist form skips crystals and keeps empty slots
        let plist = sub_items_to_plist(&slots);
        let back = sub_items_from_plist(&plist);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].borrow().len(), 1);
        // a slot that is no array, an entry that is no data, a time crystal entry
        let mut crystal_plist = sub_items_to_plist(&[Rc::new(RefCell::new(vec![]))]);
        if let PlistValue::Array(a) = &mut crystal_plist {
            a.items.push(PlistValue::int(1));
            a.items.push(mutable_array_of(vec![PlistValue::int(2), PlistValue::data(crystal.borrow().save_data())]));
        }
        let b2 = sub_items_from_plist(&crystal_plist);
        assert_eq!(b2.len(), 3);
        assert!(b2[1].borrow().is_empty() && b2[2].borrow().is_empty());
        assert!(sub_items_from_plist(&PlistValue::int(3)).is_empty());
    }

    /// Setters, getters and the priority hand-off.
    #[test]
    fn accessors() {
        let (w, dw, _r) = flat("fb_acc", 5);
        let o = mk(&w, &dw, 10, 9, 9, true);
        let f = fb(&o);
        f.set_item_type(12);
        f.set_data_a(3);
        f.set_data_b(4);
        f.set_creation_sound_play_time(1.5);
        f.set_sound_type(2);
        assert_eq!(
            (f.item_type(), f.data_a(), f.data_b(), f.creation_sound_play_time(), f.sound_type()),
            (12, 3, 4, 1.5, 2)
        );
        assert!(f.dynamic_object_save_dict().is_none());
        assert_eq!(o.class_info().name, "FreeBlock");
        // hovering blocks do not "fall"; a loose one over air does
        assert!(!f.falling());
        f.set_hovers(false);
        assert!(f.falling());
        // the priority blockhead can give up
        f.st.borrow_mut().priority_blockhead = Some(bh_at(1, 0.0, 0.0));
        o.set_update_needs_to_be_sent(false);
        f.priority_blockhead_cannot_pickup(&o);
        assert!(f.priority_blockhead().is_none() && o.update_needs_to_be_sent());
    }

    /// Loading a saved block: its priority blockhead is looked up, and a block that hovered for over fifteen
    /// minutes drops (sliding down through the air) unless it is an item that is allowed to hover forever.
    #[test]
    fn loading_old_hovering_blocks() {
        let (w, dw, _r) = flat("fb_load_old", 5);
        w.set_world_time(0.0);
        let o = mk(&w, &dw, 10, 20, 9, true);
        let mut d = o.get_save_dict(&o);
        // a priority blockhead that is online
        let who = bh_at(42, 10.0, 20.0);
        dw.blockheads.borrow_mut().push(who.clone());
        d.map.insert("priorityBlockheadUinqueID".to_string(), PlistValue::int(42));
        let r = FreeBlock::from_save_dict(&w, &dw, &d, None).unwrap();
        assert!(fb(&r).priority_blockhead().is_some());
        d.map.remove(&"priorityBlockheadUinqueID".to_string());
        // an hour later the block lets go and slides down to the ground (at most 16 tiles)
        w.set_world_time(1000.0);
        let dropped = FreeBlock::from_save_dict(&w, &dw, &d, None).unwrap();
        assert!(!fb(&dropped).hovers());
        assert!(dropped.update_needs_to_be_sent());
        assert_eq!(dropped.pos().y, 6);
        // much later: too old for the slide (the 900 s head start has passed twice over)
        w.set_world_time(5000.0);
        let late = FreeBlock::from_save_dict(&w, &dw, &d, None).unwrap();
        assert!(!fb(&late).hovers());
        assert_eq!(late.pos().y, 20);
        // blocks that may hover forever keep doing it
        for item in [ITEM_PINE_CONE, 142, 205, 0xcd] {
            let mut dd = d.clone();
            dd.map.insert("itemType".to_string(), PlistValue::int(item as i64));
            let keep = FreeBlock::from_save_dict(&w, &dw, &dd, None).unwrap();
            assert!(fb(&keep).hovers(), "item {item}");
        }
        // and one with something under it stops its slide
        tile(&w, 30, 18).update(|t| t.type_index = 6);
        let mut d2 = d.clone();
        d2.map.insert("pos_x".to_string(), PlistValue::int(30));
        d2.map.insert("floatPos[VX]".to_string(), PlistValue::real_f32(30.5));
        w.set_world_time(1000.0);
        let stopped = FreeBlock::from_save_dict(&w, &dw, &d2, None).unwrap();
        assert_eq!(stopped.pos().y, 19);
    }

    /// Net images: odd extra data, duplicate detection without ids, the priority blockhead lookup, and a replica at
    /// an unloaded tile.
    #[test]
    fn net_init_variants() {
        let (w, dw, _r) = flat("fb_net_variants", 5);
        let who = bh_at(55, 10.0, 9.0);
        dw.blockheads.borrow_mut().push(who.clone());
        let mut sd = new_mutable_dict();
        sd.map.insert("paintColor".to_string(), PlistValue::int(3));
        // a priority blockhead with a save dict without an id: never a duplicate
        let blob = FreeBlock::creation_net_data_for_freeblock_at_position(
            IntPair { x: 30, y: 9 },
            9,
            0,
            0,
            None,
            Some(&sd),
            false,
            Some(&who),
            0,
            0.0,
        );
        let mut avoid: VecDeque<u64> = VecDeque::new();
        avoid.push_back(5);
        let a = FreeBlock::from_net_data_avoid_freeblock_dupe_object_ids(&w, &dw, None, &blob, &mut avoid).unwrap();
        assert_eq!(avoid.len(), 1);
        assert!(fb(&a).priority_blockhead().is_some());
        assert_eq!(fb(&a).st.borrow().paint_color, 3);
        // extra data that does not parse as a plist is ignored
        let mut junk = blob.clone();
        junk.truncate(0x38);
        junk.extend_from_slice(b"not a plist");
        assert!(FreeBlock::from_net_data(&w, &dw, None, &junk).is_some());
        // a save dict that is no dictionary
        let mut extra = new_mutable_dict();
        extra.map.insert("dynamicObjectSaveDict".to_string(), PlistValue::int(1));
        let mut odd = blob.clone();
        odd.truncate(0x38);
        odd.extend_from_slice(&data_from_property_list(&PlistValue::Dict(extra)).unwrap());
        let o = FreeBlock::from_net_data(&w, &dw, None, &odd).unwrap();
        assert!(fb(&o).dynamic_object_save_dict().is_none());
        // a head-only image claiming extra data has nothing to read
        let mut headless = blob.clone();
        headless.truncate(0x38);
        assert!(FreeBlock::from_net_data(&w, &dw, None, &headless).is_some());
        // a replica at an unloaded tile is refused
        let far = FreeBlock::creation_net_data_for_freeblock_at_position(
            IntPair { x: 30, y: 400 },
            9,
            0,
            0,
            None,
            None,
            true,
            None,
            0,
            0.0,
        );
        assert!(FreeBlock::from_net_data(&w, &dw, None, &far).is_none());
    }

    /// Lifetimes: special items outlive ordinary ones; wind lets a hovering block go.
    #[test]
    fn lifetimes_and_wind() {
        let (w, dw, _r) = flat("fb_life", 5);
        w.set_world_time(0.0);
        // a chest item (interaction object item) that is not hovering and is older than 27000 s goes away
        let chest = mk(&w, &dw, 10, 9, 0x41f, false);
        w.set_world_time(30000.0);
        chest.update_accurate_dt_is_simulation(&chest, 0.2, 0.2, false);
        w.set_world_time(0.0);
        let train = mk(&w, &dw, 14, 9, 0xcd, false);
        w.set_world_time(30000.0);
        train.update_accurate_dt_is_simulation(&train, 0.2, 0.2, false);
        assert!(train.needs_removed());
        // wind above 0.9 releases a young hovering block
        install_deps(Rc::new(Windy));
        *w.weather.borrow_mut() = Some(Rc::new(Weather {}));
        w.set_world_time(100.0);
        let young = mk(&w, &dw, 18, 9, 5, true);
        tile(&w, 18, 9).update(|t| t.contents = 3);
        assert!(fb(&young).hovers());
        young.update_accurate_dt_is_simulation(&young, 0.2, 0.2, false);
        assert!(!fb(&young).hovers());
        assert!(young.update_needs_to_be_sent());
        install_deps(Rc::new(NullDeps));
        *w.weather.borrow_mut() = None;
    }

    /// Embedded in the ground the block climbs out; lava burns it; replicas leave tiles that are not loaded alone.
    #[test]
    fn embedded_blocks() {
        let (w, dw, _r) = flat("fb_embedded", 5);
        // the cell above is solid, the one below is free? no: wedge it in a 1-tile pit and let it find the sides
        for (x, solid_above, solid_below) in [(10, true, true), (14, true, true)] {
            let _ = (solid_above, solid_below);
            tile(&w, x, 8).update(|t| t.type_index = 6);
            tile(&w, x, 9).update(|t| t.type_index = 6);
            tile(&w, x, 10).update(|t| t.type_index = 6);
        }
        // right neighbour free
        let o = mk(&w, &dw, 10, 9, 9, true);
        fb(&o).set_hovers(false);
        // (x,y+1) solid, (x,y-1) solid: the right-hand cell (11,9) is free
        o.update_accurate_dt_is_simulation(&o, 0.2, 0.2, false);
        assert_eq!(o.pos(), IntPair { x: 11, y: 9 });
        // left only
        tile(&w, 17, 9).update(|t| t.type_index = 6);
        tile(&w, 17, 8).update(|t| t.type_index = 6);
        tile(&w, 17, 10).update(|t| t.type_index = 6);
        tile(&w, 18, 9).update(|t| t.type_index = 6);
        let l = mk(&w, &dw, 17, 9, 9, true);
        fb(&l).set_hovers(false);
        l.update_accurate_dt_is_simulation(&l, 0.2, 0.2, false);
        assert_eq!(l.pos(), IntPair { x: 16, y: 9 });
        // below only
        tile(&w, 20, 9).update(|t| t.type_index = 6);
        tile(&w, 20, 10).update(|t| t.type_index = 6);
        tile(&w, 19, 9).update(|t| t.type_index = 6);
        tile(&w, 21, 9).update(|t| t.type_index = 6);
        let d = mk(&w, &dw, 20, 9, 9, true);
        fb(&d).set_hovers(false);
        d.update_accurate_dt_is_simulation(&d, 0.2, 0.2, false);
        assert_eq!(d.pos(), IntPair { x: 20, y: 8 });
        // buried in lava
        tile(&w, 25, 9).update(|t| t.type_index = TILE_LAVA);
        let lava = mk(&w, &dw, 25, 9, 9, true);
        fb(&lava).set_hovers(false);
        lava.update_accurate_dt_is_simulation(&lava, 0.2, 0.2, false);
        assert!(lava.needs_removed());
        // a replica whose tile is not loaded does nothing
        let src = mk(&w, &dw, 30, 9, 9, true);
        let nd = src.creation_net_data_for_client(&src, None).unwrap();
        let rep = FreeBlock::from_net_data(&w, &dw, None, &nd).unwrap();
        rep.base().borrow_mut().is_net = true;
        rep.base().borrow_mut().pos = IntPair { x: 30, y: 400 };
        rep.update_accurate_dt_is_simulation(&rep, 0.2, 0.2, false);
        assert!(!rep.needs_removed());
    }

    /// Blocks in and over water: drifting with the surface slope, sinking, resting on the surface or the bottom.
    #[test]
    fn blocks_in_water() {
        let (w, dw, _r) = flat("fb_water", 5);
        for x in 10..=20 {
            water(&w, x, 6, 200);
            water(&w, x, 7, 100 + (x as u8) * 5);
        }
        // a sloping surface pushes the block to the lower side
        let o = mk(&w, &dw, 12, 7, 9, true);
        fb(&o).set_hovers(false);
        for _ in 0..40 {
            o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false);
        }
        assert!(fb(&o).st.borrow().x_velocity_water != 0.0 || o.pos().x != 12);
        // next to a wall the drift stops: right wall
        tile(&w, 31, 7).update(|t| t.type_index = 6);
        for x in 28..=30 {
            water(&w, x, 7, 255);
        }
        water(&w, 29, 7, 0);
        let r = mk(&w, &dw, 30, 7, 9, true);
        fb(&r).set_hovers(false);
        fb(&r).st.borrow_mut().x_velocity_water = 3.0;
        r.base().borrow_mut().float_pos = Vector2 { x: 30.9, y: 7.8 };
        r.update_accurate_dt_is_simulation(&r, 0.1, 0.1, false);
        // left wall
        tile(&w, 39, 7).update(|t| t.type_index = 6);
        for x in 40..=42 {
            water(&w, x, 7, 255);
        }
        let l = mk(&w, &dw, 40, 7, 9, true);
        fb(&l).set_hovers(false);
        fb(&l).st.borrow_mut().x_velocity_water = -3.0;
        l.base().borrow_mut().float_pos = Vector2 { x: 40.2, y: 7.8 };
        l.update_accurate_dt_is_simulation(&l, 0.1, 0.1, false);
        // drifting into lava
        tile(&w, 51, 7).update(|t| t.type_index = TILE_LAVA);
        for x in 48..=50 {
            water(&w, x, 7, 255);
        }
        let lv = mk(&w, &dw, 50, 7, 9, true);
        fb(&lv).set_hovers(false);
        fb(&lv).st.borrow_mut().x_velocity_water = 6.0;
        lv.base().borrow_mut().float_pos = Vector2 { x: 50.2, y: 7.8 };
        lv.update_accurate_dt_is_simulation(&lv, 0.5, 0.5, false);
        assert!(lv.needs_removed());
        // drifting into a plain wall: pushed back to its edge
        tile(&w, 61, 7).update(|t| t.type_index = 6);
        for x in 58..=60 {
            water(&w, x, 7, 255);
        }
        let wl = mk(&w, &dw, 60, 7, 9, true);
        fb(&wl).set_hovers(false);
        fb(&wl).st.borrow_mut().x_velocity_water = 6.0;
        wl.base().borrow_mut().float_pos = Vector2 { x: 60.2, y: 7.8 };
        wl.update_accurate_dt_is_simulation(&wl, 0.5, 0.5, false);
        // a tiny drift stops dead
        water(&w, 80, 7, 255);
        water(&w, 81, 7, 255);
        let tiny = mk(&w, &dw, 80, 7, 9, true);
        fb(&tiny).set_hovers(false);
        fb(&tiny).st.borrow_mut().x_velocity_water = 0.0005;
        tiny.update_accurate_dt_is_simulation(&tiny, 0.2, 0.2, false);
        assert_eq!(fb(&tiny).st.borrow().x_velocity_water, 0.0);
        // over water: sinks at the water's own pace, rests at 0.75 into the tile
        water(&w, 70, 6, 255);
        water(&w, 70, 7, 255);
        let s = mk(&w, &dw, 70, 10, 9, true);
        fb(&s).set_hovers(false);
        for _ in 0..300 {
            s.update_accurate_dt_is_simulation(&s, 0.1, 0.1, false);
        }
        assert!(s.float_pos().y < 10.0);
    }

    /// A falling block that runs into a solid cell below, into lava, and one landing in a shallow pool; huge frames.
    #[test]
    fn landing_variants() {
        let (w, dw, _r) = flat("fb_land", 5);
        // fast fall onto a roof one cell below the block
        tile(&w, 10, 9).update(|t| t.type_index = 6);
        let o = mk(&w, &dw, 10, 11, 9, true);
        fb(&o).set_hovers(false);
        o.base().borrow_mut().float_pos = Vector2 { x: 10.5, y: 10.0 };
        fb(&o).st.borrow_mut().fall_speed = 0.2;
        for _ in 0..10 {
            o.update_accurate_dt_is_simulation(&o, 0.5, 0.5, false);
        }
        assert!(!o.needs_removed());
        // falls into a lava cell
        tile(&w, 20, 9).update(|t| t.type_index = TILE_LAVA);
        let lv = mk(&w, &dw, 20, 12, 9, true);
        fb(&lv).set_hovers(false);
        lv.base().borrow_mut().float_pos = Vector2 { x: 20.5, y: 10.05 };
        fb(&lv).st.borrow_mut().fall_speed = 0.2;
        for _ in 0..20 {
            lv.update_accurate_dt_is_simulation(&lv, 0.5, 0.5, false);
        }
        assert!(lv.needs_removed());
        // a huge frame length is clamped by the min()s
        let big = mk(&w, &dw, 30, 20, 9, true);
        fb(&big).set_hovers(false);
        big.update_accurate_dt_is_simulation(&big, 2.0, 2.0, false);
        // a pool: the block settles at 0.75 into the surface cell
        for x in 40..=42 {
            water(&w, x, 6, 255);
            water(&w, x, 7, 60);
        }
        let pool = mk(&w, &dw, 41, 9, 9, true);
        fb(&pool).set_hovers(false);
        for _ in 0..200 {
            pool.update_accurate_dt_is_simulation(&pool, 0.1, 0.1, false);
        }
        assert!(pool.float_pos().y > 6.0);
    }

    /// Hover support and the pickup scan.
    #[test]
    fn hover_support_and_pickup_scan() {
        let (w, dw, _r) = flat("fb_pickup", 5);
        // a hovering block over empty contents lets go
        let h = mk(&w, &dw, 10, 9, 9, true);
        w.set_world_time(0.0);
        h.update_accurate_dt_is_simulation(&h, 0.2, 0.2, false);
        assert!(!fb(&h).hovers());
        // blockheads nearby: the one with priority is asked, others are skipped
        let near = bh_at(1, 30.5, 9.0);
        let other = bh_at(2, 30.5, 9.0);
        dw.blockheads.borrow_mut().extend([near.clone(), other.clone()]);
        let f = mk(&w, &dw, 30, 9, 9, false);
        f.update_accurate_dt_is_simulation(&f, 0.2, 0.2, false);
        fb(&f).st.borrow_mut().priority_blockhead = Some(near.clone());
        f.update_accurate_dt_is_simulation(&f, 0.2, 0.2, false);
        // an object in the list that is no blockhead is skipped
        let plain: ObjRef = crate::objects::base::testutil::TestObj::new();
        dw.blockheads.borrow_mut().push(plain);
        fb(&f).st.borrow_mut().priority_blockhead = None;
        f.update_accurate_dt_is_simulation(&f, 0.2, 0.2, false);
    }

    /// A priority blockhead pulls the block in; one that is gone releases it.
    #[test]
    fn priority_magnet() {
        let (w, dw, _r) = flat("fb_magnet", 5);
        let who = bh_at(9, 20.5, 12.0);
        let o = mk(&w, &dw, 10, 9, 9, false);
        fb(&o).st.borrow_mut().priority_blockhead = Some(who.clone());
        for _ in 0..30 {
            o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false);
        }
        assert!(o.float_pos().x > 12.0);
        // a long frame jumps straight to the goal
        o.update_accurate_dt_is_simulation(&o, 1.0, 1.0, false);
        assert_eq!(o.float_pos(), Vector2 { x: 20.5, y: 12.5 });
        // the blockhead leaves
        who.base().borrow_mut().needs_removed = true;
        o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false);
        assert!(fb(&o).priority_blockhead().is_none());
    }

    /// Saving the priority blockhead, remote updates, hovering poles being taken.
    #[test]
    fn save_priority_remote_update_and_poles() {
        let (w, dw, _r) = flat("fb_misc", 5);
        let who = bh_at(13, 10.0, 9.0);
        let o = mk(&w, &dw, 10, 9, 9, true);
        fb(&o).st.borrow_mut().priority_blockhead = Some(who);
        let d = o.get_save_dict(&o);
        assert_eq!(d.map.get(&"priorityBlockheadUinqueID".to_string()).unwrap().int_value(), 13);
        // the net image says: nobody has priority any more
        let mut upd = o.update_net_data_for_client(&o, None).unwrap();
        upd[0x18..0x20].copy_from_slice(&0u64.to_le_bytes());
        o.remote_update(&o, &upd);
        assert!(fb(&o).priority_blockhead().is_none());
        // a replica takes the motion fields
        let c = o.creation_net_data_for_client(&o, None).unwrap();
        let rep = FreeBlock::from_net_data(&w, &dw, None, &c).unwrap();
        rep.base().borrow_mut().is_net = true;
        let upd2 = FreeblockUpdateNetData { fall_speed: 255, hovers: 0, ..FreeblockUpdateNetData::default() };
        rep.remote_update(&rep, &upd2.to_vec());
        assert_eq!(fb(&rep).st.borrow().fall_speed, 1.0);
        // taking a hovering pole item tells the dynamic world
        let pole = mk(&w, &dw, 14, 9, 142, true);
        pole.remove_from_macro_block(&pole);
        let loose = mk(&w, &dw, 18, 9, 142, false);
        loose.remove_from_macro_block(&loose);
    }

    /// Saved blocks that were not hovering stay as they are; updating a removed block does nothing; a block in a
    /// column whose tiles are not loaded is looked up through the loader.
    #[test]
    fn saved_loose_blocks_removed_blocks_and_unloaded_tiles() {
        let (w, dw, _r) = flat("fb_misc2", 5);
        w.set_world_time(5000.0);
        let loose = mk(&w, &dw, 10, 9, 9, false);
        let d = loose.get_save_dict(&loose);
        let again = FreeBlock::from_save_dict(&w, &dw, &d, None).unwrap();
        assert!(!fb(&again).hovers() && again.pos().y == 9);
        loose.base().borrow_mut().needs_removed = true;
        loose.update_accurate_dt_is_simulation(&loose, 0.2, 0.2, false);
        // a hovering block above the loaded rows: the hover check meets a missing tile
        let far = mk(&w, &dw, 30, 400, 9, true);
        far.update_accurate_dt_is_simulation(&far, 0.2, 0.2, false);
        // a block in water with nothing to drift to
        water(&w, 50, 7, 255);
        let calm = mk(&w, &dw, 50, 7, 9, true);
        fb(&calm).set_hovers(false);
        calm.update_accurate_dt_is_simulation(&calm, 0.2, 0.2, false);
        assert_eq!(fb(&calm).st.borrow().x_velocity_water, 0.0);
        // just under the surface mark of a water cell with a roof: it is set to rest at the surface
        water(&w, 60, 7, 255);
        tile(&w, 60, 8).update(|t| t.type_index = 6);
        let rest = mk(&w, &dw, 60, 7, 9, true);
        fb(&rest).set_hovers(false);
        rest.base().borrow_mut().float_pos = Vector2 { x: 60.5, y: 7.7495 };
        fb(&rest).st.borrow_mut().fall_speed = -0.01;
        rest.update_accurate_dt_is_simulation(&rest, 0.01, 0.01, false);
        assert_eq!(rest.float_pos().y, 7.75);
    }
}
