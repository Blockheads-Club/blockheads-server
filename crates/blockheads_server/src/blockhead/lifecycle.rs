//! `Blockhead` construction, net/save serialisation and teardown.
//!

use std::cell::RefCell;
use std::rc::Rc;

use crate::fmath::cconv::f32_to_i32;
use crate::foundation::exception::{range_exception_beyond_size, range_exception_index};
use crate::items::colors::{default_color_for_item_type, mixed_color_for_index};
use crate::items::food::item_type_from_food_type;
use crate::items::inventory_item::{InventoryItem, ItemRef, SubSlot};
use crate::items::misc::{default_data_a_and_b_for_item_type, get_new_options_from_old};
use crate::items::predicates::item_type_is_sowable;
use crate::objects::base::{
    base_init_with_world_dynamic_world_at_position_cache, base_init_with_world_dynamic_world_cache_net_data,
    base_init_with_world_dynamic_world_save_dict_cache, new_mutable_dict, padded_bytes, DynamicObject, DynamicWorldRef,
    WorldRef,
};
use crate::objects::handle::{alloc_obj, CacheRef, ObjRef};
use crate::plist::helpers::{data_from_property_list, property_list_with_data_raw};
use crate::plist::value::{PlistDict, PlistValue};
use crate::rng::glibc_rand::{r01_from, rand_tagged};
use crate::types::enums::FoodType;
use crate::types::item_type::ItemType;
use crate::types::netdata::{BlockheadCreationData, BlockheadPreviewDataNew, BlockheadUpdateData};
use crate::types::structs::{BlockheadSkinOptions, IntPair, TradeMission, WindowInfo, WireStruct};
use crate::world::helpers::{tile_contains_door, tile_contains_trap_door, tile_is_air_or_snow, tile_is_solid};
use crate::world::state::{TileRef, World};
use crate::world::tiles::tile_at_world_position_loaded;

use super::blockhead::{host, it, Blockhead};

/// `BlockheadCraftableItemObject` as the new-character constructor sees it (`name`, `skinOptions`). The real
/// class (workbench module, R-2) implements this.
pub trait BlockheadCraftableItemObjectLike {
    /// `-name`
    fn name(&self) -> Option<String>;
    /// `-skinOptions`
    fn skin_options(&self) -> BlockheadSkinOptions;
}

fn tile_at(world: &World, p: IntPair) -> Option<TileRef> {
    tile_at_world_position_loaded(p.x, p.y, world)
}

/// `viewRadius = (int)max(wi.screenWidth, wi.screenHeight) / 46`.
pub fn view_radius_for_world(world: &World) -> i32 {
    let wi = world.window_info().unwrap_or(WindowInfo {
        screen_width: 1024.0,
        screen_height: 768.0,
        half_screen_width: 512.0,
        half_screen_height: 384.0,
        padding_left: 0.0,
        padding_right: 0.0,
        padding_top: 0.0,
        padding_bottom: 0.0,
    });
    let m = if wi.screen_width > wi.screen_height { wi.screen_width } else { wi.screen_height };
    f32_to_i32(m) / 46
}

fn set_squares(f: &mut super::blockhead::BlockheadFields, p: IntPair) {
    f.next_to_square = p;
    f.to_square = p;
    f.prev_from_square = p;
    f.from_square = p;
}

/// `vec4(0.2f) + mixedColorForIndex(colorIndex, type) * 0.8f` component-wise f32.
fn tinted_color(color_index: u16, item_type: ItemType) -> [f32; 4] {
    let m = mixed_color_for_index(color_index, item_type).v;
    [0.2f32 + m[0] * 0.8f32, 0.2f32 + m[1] * 0.8f32, 0.2f32 + m[2] * 0.8f32, 0.2f32 + m[3] * 0.8f32]
}

fn new_item(t: u32, a: u16, b: u16) -> ItemRef {
    InventoryItem::new(ItemType(t), a, b, None, None)
}

impl Blockhead {
    // ================================================================================================
    // ================================================================================================

    /// `-initSubDerivedStuffStuff`: the server-relevant part (client shaders/textures are skipped). Exactly one
    /// `rand()`: `randomIdleWait = ((float)rand() / 2147483648.0f) * 10.0f + 5.0f` in f32 (range [5,15]
    /// inclusive).
    pub fn init_sub_derived_stuff_stuff(&self) {
        let r = r01_from(rand_tagged("blockhead/init_sub_derived_stuff_stuff"));
        self.bh.borrow_mut().random_idle_wait = r * 10.0f32 + 5.0f32;
        // `if [dynamicWorld isClient]`: CLIENT-ONLY (dead on server): thisFramePickupRequests / unconfirmedPickups
        // stay None.
    }

    /// `-updateClothingCubes` / `-updateSkin`: CLIENT-ONLY, server no-ops.
    pub fn update_clothing_cubes(&self) {}
    /// See [`Blockhead::update_clothing_cubes`].
    pub fn update_skin(&self) {}

    fn alloc_linked(world: &Rc<World>) -> (Rc<Blockhead>, ObjRef) {
        let rc = alloc_obj(Blockhead::alloc);
        rc.bh.borrow_mut().world_rc = Rc::downgrade(world);
        let me: ObjRef = rc.clone();
        (rc, me)
    }

    // ================================================================================================
    // : new character
    // ================================================================================================

    /// `-initWithWorld:dynamicWorld:atPosition:cache:blockheadNumber:craftableItemObject:uniqueID:`.
    /// `unique_id == u64::MAX` keeps the id assigned by the base init.
    #[allow(clippy::too_many_arguments)]
    pub fn init_with_world_dynamic_world_at_position_cache_blockhead_number_craftable_item_object_unique_id(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        pos: IntPair,
        cache: CacheRef,
        blockhead_number: i32,
        craftable_item_object: Option<&dyn BlockheadCraftableItemObjectLike>,
        unique_id: u64,
    ) -> Option<ObjRef> {
        let (rc, me) = Self::alloc_linked(world);
        let wref: WorldRef = world.clone();
        // [super initWithWorld:dynamicWorld:atPosition:cache:] (always succeeds)
        base_init_with_world_dynamic_world_at_position_cache(&me, &wref, dw, pos, cache).then_some(())?;
        if unique_id != u64::MAX {
            rc.base.borrow_mut().unique_id = unique_id;
        }
        {
            // the ARGUMENT value, not self.pos (spec 4.5)
            let mut f = rc.bh.borrow_mut();
            set_squares(&mut f, pos);
            f.travel_fraction = 1.0f32;
            f.travel_speed = 4.0f32;
            let t = tile_at(world, pos);
            f.from_tile = t;
            f.to_tile = t;
            f.view_radius = view_radius_for_world(world);
        }

        // spawn relocation, only if the start tile is solid (tileIsSolid(NULL) is NO)
        let from_tile = rc.bh.borrow().from_tile;
        if tile_is_solid(from_tile) {
            let mut found = false;
            for _n in 0..4 {
                let np = {
                    let mut b = rc.base.borrow_mut();
                    b.pos = IntPair { x: b.pos.x, y: b.pos.y.wrapping_add(1) };
                    b.pos
                };
                let t = tile_at(world, np);
                {
                    let mut f = rc.bh.borrow_mut();
                    set_squares(&mut f, np);
                    f.from_tile = t;
                    f.to_tile = t;
                }
                if t.is_some() && tile_is_air_or_snow(t) {
                    found = true;
                    break;
                }
            }
            if !found {
                let p = world.start_portal_pos();
                rc.base.borrow_mut().pos = p;
                let t = tile_at(world, p);
                {
                    let mut f = rc.bh.borrow_mut();
                    set_squares(&mut f, p);
                    f.from_tile = t;
                    f.to_tile = t;
                }
                if t.is_none() {
                    me.remove_from_macro_block(&me);
                    return None;
                }
            }
        }
        // floatPos and renderPos are NOT updated here (spec 4.5 NOTE).

        {
            let mut f = rc.bh.borrow_mut();
            f.save_item_slots_array = Vec::new();
            f.selected_tool_index = 0;
            f.name = craftable_item_object.and_then(|c| c.name());
            // nil craftableItemObject: the original copies 20 uninitialised bytes; zeros here.
            f.skin_options = craftable_item_object.map(|c| c.skin_options()).unwrap_or_default();
            f.state.health = 1.0f32;
            f.state.happiness = 1.0f32;
            f.state.fullness = 1.2f32;
            f.state.energy = 1.2f32;
            f.state.environment = 1.0f32;
            f.action_queue = Vec::new();
            f.inventory_items = Vec::with_capacity(8);
            for _ in 0..8 {
                f.inventory_items.push(Rc::new(RefCell::new(Vec::with_capacity(99))));
                // `[dynamicWorld isClient]` is NO on the server: an empty array per slot
                f.save_item_slots_array.push(Vec::new());
            }
            // slot 0 = clothing container
            let clothing = new_item(1, 0, 0);
            f.inventory_items[0].borrow_mut().push(clothing);
            f.inventory_changed_this_frame_slots[0] = true;
        }

        // starter items
        let h = host();
        let rules = world.custom_rules();
        let allowed = |k: usize| rules.custom_rules_enabled == 0 || rules.spawn_item_types[k] == 2;
        if !world.expert_mode() {
            let add = |item: ItemRef, slot: i32| {
                h.add_item_to_inventory_flash_disable_warp_check_force_slot_index(&rc, &item, false, true, slot);
            };
            // tool, slot 1
            if allowed(0) {
                match blockhead_number {
                    0 => add(new_item(16, 0, 0), 1),
                    1 => {
                        let r = r01_from(rand_tagged("blockhead/init_new/starter_tool")) as f64;
                        let t = if 0.33 > r {
                            43
                        } else if 0.66 > r {
                            49
                        } else {
                            40
                        };
                        add(new_item(t, 0, 0), 1);
                    }
                    2 => {
                        let r = r01_from(rand_tagged("blockhead/init_new/starter_tool")) as f64;
                        let t = if 0.5 > r { 66 } else { 67 };
                        add(new_item(t, 0, 0), 1);
                    }
                    _ => {
                        let r = r01_from(rand_tagged("blockhead/init_new/starter_tool")) as f64;
                        let t = if 0.5 > r { 93 } else { 90 };
                        add(new_item(t, 0, 0), 1);
                    }
                }
            }
            // food x5, slot 2
            if allowed(1) {
                let foods = world.distance_ordered_food_types.borrow().clone();
                let idx = match blockhead_number {
                    0 => 0usize,
                    1 => 1,
                    2 => 2,
                    _ => 3,
                };
                let t = item_type_from_food_type(FoodType(foods[idx] as u32));
                let (a, b) = if blockhead_number == 0 {
                    if t.0 == 91 {
                        (0u16, 0u16)
                    } else {
                        (127u16, 127u16)
                    }
                } else {
                    let (a, b) = default_data_a_and_b_for_item_type(t);
                    (a as u16, b as u16)
                };
                for _ in 0..5 {
                    add(new_item(t.0, a, b), 2);
                }
            }
            // basket, slot 3
            if allowed(2) {
                add(new_item(12, 0, 0), 3);
            }
            // light x10, slot 4
            if allowed(3) {
                let lt = match blockhead_number {
                    0 => 0,
                    1 | 2 => 47,
                    _ => 150,
                };
                if lt != 0 {
                    for _ in 0..10 {
                        add(new_item(lt, 0, 0), 4);
                    }
                }
            }
        }
        // custom spawn loop (runs after the above, also when expertMode)
        if rules.custom_rules_enabled != 0 {
            for i in 0..7usize {
                let t = rules.spawn_item_types[i];
                let n_raw = rules.spawn_item_counts[i] as i32;
                let n = if n_raw < 99 { n_raw } else { 99 };
                if t == 2 || t == 5 || n <= 0 {
                    continue;
                }
                let ab: u16 = if item_type_is_sowable(ItemType(t as u32)) { 127 } else { 0 };
                for _ in 0..n {
                    let item = new_item(t as u32, ab, ab);
                    h.add_item_to_inventory_flash_disable_warp_check_force_slot_index(
                        &rc,
                        &item,
                        false,
                        true,
                        (i + 1) as i32,
                    );
                }
            }
        }
        rc.init_sub_derived_stuff_stuff();
        Some(me)
    }

    // ================================================================================================
    // : load from save
    // ================================================================================================

    /// `-initWithWorld:dynamicWorld:saveDict:savedInventorySlots:cache:repositionOnLoadFailure:clientSaveDir:clientLocallySavedDict:`.
    /// `saved_inventory_slots` is a `PlistValue::Array` of 8 arrays of `PlistValue::Data`.
    #[allow(clippy::too_many_arguments)]
    pub fn init_with_world_dynamic_world_save_dict_saved_inventory_slots_cache_reposition_on_load_failure_client_save_dir_client_locally_saved_dict(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        save_dict: &PlistDict,
        saved_inventory_slots: Option<&PlistValue>,
        cache: CacheRef,
        reposition_on_load_failure: bool,
        client_save_dir: Option<&str>,
        _client_locally_saved_dict: Option<&PlistValue>,
    ) -> Option<ObjRef> {
        let (rc, me) = Self::alloc_linked(world);
        let wref: WorldRef = world.clone();
        // (the base initialiser always succeeds for a save dictionary)
        base_init_with_world_dynamic_world_save_dict_cache(&me, &wref, dw, save_dict, cache).then_some(())?;
        let get = |k: &str| save_dict.map.get(&k.to_string());
        let h = host();
        {
            let pos = rc.pos();
            let mut f = rc.bh.borrow_mut();
            set_squares(&mut f, pos);
            f.travel_fraction = 1.0f32;
            f.travel_speed = 4.0f32;
            let t = tile_at(world, pos);
            f.from_tile = t;
            f.to_tile = t;
            if let Some(PlistValue::Data(d)) = get("state") {
                f.state.load_from_save_blob(&d.bytes);
            }
            if let Some(PlistValue::Data(d)) = get("tradeMissionData") {
                let mut raw = f.current_trade_mission.to_vec();
                let n = d.bytes.len().min(raw.len());
                raw[..n].copy_from_slice(&d.bytes[..n]);
                f.current_trade_mission = TradeMission::read_le(&raw);
            }
        }

        // ---- position validation
        let (from_tile, pos, anim) = {
            let f = rc.bh.borrow();
            (f.from_tile, rc.pos(), f.state.animation_type)
        };
        let bad_tile = from_tile.is_none()
            || pos.y > 0x3fe
            || (tile_is_solid(from_tile)
                && !tile_contains_door(from_tile)
                && !tile_contains_trap_door(from_tile)
                && from_tile.map(|t| t.type_index()) != Some(4)
                && from_tile.map(|t| t.contents()) != Some(0x67));
        let riding = from_tile.is_some() && matches!(anim, 27 | 28 | 29 | 30 | 33);
        if bad_tile && !riding {
            if !reposition_on_load_failure {
                me.remove_from_macro_block(&me);
                return None;
            }
            let mut found = false;
            let base_pos = rc.pos();
            'search: for k in 0..16i32 {
                let cands = [
                    IntPair { x: base_pos.x, y: base_pos.y.wrapping_add(k).wrapping_add(1) },
                    IntPair { x: base_pos.x, y: base_pos.y.wrapping_sub(k).wrapping_sub(1) },
                    IntPair { x: base_pos.x.wrapping_add(k).wrapping_add(1), y: base_pos.y },
                    IntPair { x: base_pos.x.wrapping_sub(k).wrapping_sub(1), y: base_pos.y },
                ];
                for cand in cands {
                    let t = tile_at(world, cand);
                    // `isClientBlockheadBeingControlledByServer` is still NO here: `tile->light != 0`
                    if let Some(tr) = t {
                        if tile_is_air_or_snow(t) && rc.tile_is_lit_for_self_at_pos_impl(&tr.get(), cand) {
                            rc.base.borrow_mut().pos = cand;
                            {
                                let mut f = rc.bh.borrow_mut();
                                set_squares(&mut f, cand);
                                f.from_tile = t;
                                f.to_tile = t;
                            }
                            found = true;
                            break 'search;
                        }
                    }
                }
            }
            if found {
                let ts = rc.bh.borrow().to_square;
                let fp = crate::types::structs::Vector2 { x: (0.5f64 + ts.x as f64) as f32, y: ts.y as f32 };
                rc.base.borrow_mut().float_pos = fp;
                rc.bh.borrow_mut().render_pos = fp;
                // a lightBlockIndex lookup is performed here and discarded (no effect)
            } else {
                let p = world.start_portal_pos();
                rc.base.borrow_mut().pos = p;
                let t = tile_at(world, p);
                let ts;
                {
                    let mut f = rc.bh.borrow_mut();
                    set_squares(&mut f, p);
                    f.from_tile = t;
                    f.to_tile = t;
                    ts = f.to_square;
                }
                let fp = crate::types::structs::Vector2 { x: (0.5f64 + ts.x as f64) as f32, y: ts.y as f32 };
                rc.base.borrow_mut().float_pos = fp;
                rc.bh.borrow_mut().render_pos = fp;
                if t.is_none() {
                    me.remove_from_macro_block(&me);
                    return None;
                }
            }
        }

        // ---- loading of the surrounding area
        // (the server-client light block lookup of the original needs `isNet` or the controlled flag, and the object
        // is neither at this point: the index stays -1 and a loaded character is never a replica)
        let p = rc.pos();
        h.fully_load_if_needed_around_pos_client_light_block_index_for_blockhead(p, -1, Some(&me));
        {
            let p = rc.pos();
            let mut b = rc.base.borrow_mut();
            if f32_to_i32(b.float_pos.x) != p.x || f32_to_i32(b.float_pos.y) != p.y {
                b.float_pos.x = 0.5f32 + p.x as f32;
                b.float_pos.y = p.y as f32;
            }
            let fp = b.float_pos;
            drop(b);
            let mut f = rc.bh.borrow_mut();
            f.render_pos = fp;
            f.client_save_dir = client_save_dir.map(|s| s.to_string());
            if f.client_save_dir.is_some() {
                f.is_client_blockhead_being_controlled_by_server = true;
                f.client_double_time_unlocked = get("doubleTimeUnlocked").map_or(false, |v| v.bool_value());
            }
        }

        // ---- view radius and inventory
        {
            let mut f = rc.bh.borrow_mut();
            f.view_radius = view_radius_for_world(world);
            f.inventory_items = Vec::with_capacity(8);
            f.save_item_slots_array = Vec::with_capacity(8);
        }
        for i in 0..8usize {
            let slot: SubSlot = Rc::new(RefCell::new(Vec::new()));
            let mut save_slot: Vec<Vec<u8>> = Vec::new();
            // `[savedInventorySlots objectAtIndex:i]`: nil array -> nil (no iteration); short array raises
            let entry: Option<&PlistValue> = match saved_inventory_slots {
                Some(PlistValue::Array(a)) => {
                    if i >= a.items.len() {
                        range_exception_index(i as u64, a.items.len() as u64, "objectAtIndex:");
                    }
                    Some(&a.items[i])
                }
                _ => None,
            };
            if let Some(PlistValue::Array(items)) = entry {
                for si in &items.items {
                    if let PlistValue::Data(d) = si {
                        let item = InventoryItem::from_save_data(&d.bytes);
                        slot.borrow_mut().push(item);
                        save_slot.push(d.bytes.clone());
                    }
                }
            }
            let mut f = rc.bh.borrow_mut();
            f.inventory_items.push(slot);
            f.save_item_slots_array.push(save_slot);
        }
        {
            let mut f = rc.bh.borrow_mut();
            if f.inventory_items[0].borrow().is_empty() {
                let c = new_item(1, 0, 0);
                f.inventory_items[0].borrow_mut().push(c);
                f.inventory_changed_this_frame_slots[0] = true;
            }
            f.selected_tool_index = get("selectedToolIndex").map_or(0, |v| v.int_value());
            f.interaction_item_index = get("interactionItemIndex").map_or(0, |v| v.int_value());
            f.interaction_item_sub_index = get("interactionItemSubIndex").map_or(0, |v| v.int_value());
            f.clothing_increment_timer = get("clothingIncrementTimer").map_or(0, |v| v.int_value()) as f32;
            f.interaction_item = None;
            if f.interaction_item_index > 0 {
                let idx = f.interaction_item_index as usize;
                if idx >= f.inventory_items.len() {
                    range_exception_index(idx as u64, f.inventory_items.len() as u64, "objectAtIndex:");
                }
                let s = f.inventory_items[idx].clone();
                let last = s.borrow().last().cloned();
                if let Some(it0) = last {
                    f.interaction_item = Some(it0.clone());
                    let subs = it0.borrow().sub_items.clone();
                    let subs_count = subs.as_ref().map_or(0, |v| v.len());
                    if subs_count != 0 && f.interaction_item_sub_index != -1 {
                        let si = f.interaction_item_sub_index as i64 as u64;
                        if si >= subs_count as u64 {
                            range_exception_index(si, subs_count as u64, "objectAtIndex:");
                        }
                        let ss = subs.as_ref().unwrap()[si as usize].clone();
                        let l2 = ss.borrow().last().cloned();
                        if let Some(l2) = l2 {
                            f.interaction_item = Some(l2);
                        }
                    }
                }
            }
        }

        // ---- action queue
        {
            let acts = match get("actions") {
                Some(PlistValue::Array(a)) => a.items.clone(),
                _ => Vec::new(),
            };
            rc.bh.borrow_mut().action_queue = Vec::new();
            for d in &acts {
                let inv: Vec<SubSlot> = rc.bh.borrow().inventory_items.clone();
                let a = h.action_init_with_save_dict_inventory_items(d, &inv);
                rc.bh.borrow_mut().action_queue.push(a.clone());
                let p = h.action_goal_tile_pos(&a);
                let _ = tile_at(world, p); // result unused: a tile-load request
            }
            let (interacting, itype, qlen) = {
                let f = rc.bh.borrow();
                (f.state.interacting != 0, f.state.interaction_type, f.action_queue.len())
            };
            if interacting && qlen == 0 {
                rc.stop_interacting();
            }
            let (interacting, itype2) = {
                let f = rc.bh.borrow();
                (f.state.interacting != 0, f.state.interaction_type)
            };
            let _ = itype;
            if interacting && itype2 == it::INSPECT_NPC {
                rc.stop_interacting();
            }
        }

        // ---- clothing
        for (n, piece) in [(3, 0usize), (2, 1), (1, 2), (0, 3)] {
            if let Some(item) = rc.clothing_item_at_index(n) {
                let (t, db) = {
                    let i = item.borrow();
                    (i.item_type, i.data_b)
                };
                let col = tinted_color(db, t);
                let mut f = rc.bh.borrow_mut();
                match piece {
                    0 => {
                        f.hat_item_type = t.0 as u16;
                        f.hat_color_index = db;
                        f.hat_item_color = col;
                    }
                    1 => {
                        f.shirt_item_type = t.0 as u16;
                        f.shirt_color_index = db;
                        f.shirt_item_color = col;
                    }
                    2 => {
                        f.pants_item_type = t.0 as u16;
                        f.pants_color_index = db;
                        f.pants_item_color = col;
                    }
                    _ => {
                        f.shoes_item_type = t.0 as u16;
                        f.shoes_color_index = db;
                        f.shoes_item_color = col;
                    }
                }
            }
        }

        // ---- name and skin
        {
            let mut f = rc.bh.borrow_mut();
            f.name = get("name").and_then(|v| v.as_str().map(|s| s.to_string()));
            match get("skinOptions") {
                None => {
                    let male = get("male").map_or(false, |v| v.bool_value());
                    let head = get("headIndex").map_or(0, |v| v.int_value());
                    let skin = get("skinIndex").map_or(0, |v| v.int_value());
                    let hair = get("hairStyleIndex").map_or(0, |v| v.int_value());
                    f.skin_options = get_new_options_from_old(male, head, skin, hair);
                }
                Some(PlistValue::Data(d)) => {
                    f.skin_options = BlockheadSkinOptions::read_le(&padded_bytes(&d.bytes, 20));
                }
                Some(_) => {}
            }
        }

        // ---- pending path
        {
            let mut f = rc.bh.borrow_mut();
            f.load_requires_recalculation = get("loadRequiresRecalculation").map_or(false, |v| v.bool_value());
            if f.load_requires_recalculation {
                f.final_goal_square.x = get("finalGoalSquare.x").map_or(0, |v| v.int_value());
                f.final_goal_square.y = get("finalGoalSquare.y").map_or(0, |v| v.int_value());
                f.path_needs_recalculated = true;
                if let Some(a0) = f.action_queue.first().cloned() {
                    drop(f);
                    h.action_set_in_progress(&a0, false);
                }
            }
        }
        if h.is_adding_fuel_to_any_workbench(&rc) {
            rc.stop_interacting();
        }
        {
            let mut f = rc.bh.borrow_mut();
            if f.state.interacting != 0 {
                let sq = f.state.interacting_square;
                f.interacting_tile = tile_at(world, sq);
            }
        }

        // ---- interaction object
        if let Some(d) = get("interactionObjectDict") {
            let uid = d.object_for_key("uniqueID").map_or(0, |v| v.int_value());
            // int, sign-extended to the long parameter
            let obj = h.interaction_object_with_id(uid as i64);
            match obj {
                None => {
                    rc.bh.borrow_mut().interaction_object_dict_to_restore_when_object_loaded = Some(d.clone());
                }
                Some(o) => h.blockhead_would_like_to_take_ownership_with_save_dict(&o, &rc, d),
            }
        }
        rc.init_sub_derived_stuff_stuff();
        Some(me)
    }

    // ================================================================================================
    // ================================================================================================

    /// `-netInteractionObjectWasLoaded:`
    pub fn net_interaction_object_was_loaded(&self, interaction_object: &ObjRef) {
        let d = self.bh.borrow().interaction_object_dict_to_restore_when_object_loaded.clone();
        if let Some(d) = d {
            let uid = d.object_for_key("uniqueID").map_or(0, |v| v.unsigned_long_value());
            if uid == interaction_object.unique_id() {
                host().blockhead_would_like_to_take_ownership_with_save_dict(interaction_object, self, &d);
                self.bh.borrow_mut().interaction_object_dict_to_restore_when_object_loaded = None;
                self.set_update_needs_to_be_sent(true);
            }
        }
    }

    // ================================================================================================
    // : net replica
    // ================================================================================================

    /// `-setupFromNetCreationData:` `cd.dead` is ignored. A short `netData` raises
    /// `NSRangeException` from `subdataWithRange:`.
    pub fn setup_from_net_creation_data(&self, net_data: &[u8]) {
        let cd = BlockheadCreationData::read_le(&padded_bytes(net_data, 0x48));
        {
            let mut f = self.bh.borrow_mut();
            f.state.happiness = (cd.happiness as f32) / 127.0f32;
            let pieces = [
                (cd.hat_item_type, cd.hat_color, 0usize),
                (cd.shirt_item_type, cd.shirt_color, 1),
                (cd.pants_item_type, cd.pants_color, 2),
                (cd.shoes_item_type, cd.shoes_color, 3),
            ];
            for (ty, color, which) in pieces {
                match which {
                    0 => f.hat_item_type = ty,
                    1 => f.shirt_item_type = ty,
                    2 => f.pants_item_type = ty,
                    _ => f.shoes_item_type = ty,
                }
                if ty != 0 {
                    let it_ = ItemType(ty as u32);
                    let col = if color == 0 { default_color_for_item_type(it_).v } else { tinted_color(color, it_) };
                    match which {
                        0 => f.hat_item_color = col,
                        1 => f.shirt_item_color = col,
                        2 => f.pants_item_color = col,
                        _ => f.shoes_item_color = col,
                    }
                }
            }
        }
        // trailer
        let tl = (net_data.len() as i32).wrapping_sub(0x48);
        if tl < 0 {
            range_exception_beyond_size("subdataWithRange:", 0x48, tl as i64 as u64, net_data.len() as u64);
        }
        let trailer = &net_data[0x48..0x48 + tl as usize];
        // `[NSPropertyListSerialization propertyListWithData:options:format:error:]` direct: no logging helper
        let dict = crate::foundation::exception::catch_ns(|| property_list_with_data_raw(trailer, 0)).ok().flatten();
        let dget = |k: &str| -> Option<String> {
            dict.as_ref().and_then(|d| d.object_for_key(k)).and_then(|v| v.as_str().map(|s| s.to_string()))
        };
        let has_server = self.world_rc().map_or(false, |w| w.server().is_some());
        let name = resolve_net_name(has_server, dget("name"));
        let mut f = self.bh.borrow_mut();
        f.name = name;
        if f.client_id.is_none() {
            f.client_id = dget("clientID");
        }
        f.client_name = dget("clientName");
        f.skin_options = cd.skin_options;
        f.view_radius = cd.view_radius as i32;
        f.state.animation_type = cd.animation_type as u32;
        f.state.sub_animation_type = cd.sub_animation_type as u32;
        f.traverse_to_key_frame = cd.traverse_to_key_frame as i32;
        f.traverse_from_key_frame = cd.traverse_from_key_frame as i32;
        f.state.regenerating = cd.regenerating;
        f.state.on_trade_mission = cd.on_trade_mission;
        drop(f);
        self.update_skin();
    }

    /// `-initWithWorld:dynamicWorld:cache:netData:` (the replica of a connected player's character on the
    /// server).
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<ObjRef> {
        let (rc, me) = Self::alloc_linked(world);
        let wref: WorldRef = world.clone();
        if !base_init_with_world_dynamic_world_cache_net_data(&me, &wref, dw, cache, net_data) {
            return None;
        }
        rc.setup_from_net_creation_data(net_data);
        let pos = rc.pos();
        {
            let mut f = rc.bh.borrow_mut();
            set_squares(&mut f, pos);
            f.travel_fraction = 1.0f32;
            f.travel_speed = 4.0f32;
            let fs = f.from_square;
            for i in 0..4 {
                f.net_next_to_square[i] = fs;
                f.next_terrain_difficulty[i] = 10;
                f.next_animation_type[i] = f.state.animation_type as i32;
                f.next_sub_animation_type[i] = f.state.sub_animation_type as i32;
            }
            let t = tile_at(world, fs);
            f.from_tile = t;
            f.to_tile = t;
            f.render_pos = rc.base.borrow().float_pos;
        }
        rc.update_animation();
        rc.init_sub_derived_stuff_stuff();
        Some(me)
    }

    // ================================================================================================
    // : serialisation
    // ================================================================================================

    /// `-previewData`: `BlockheadPreviewDataNew` (0x28) followed by the raw UTF-8 name.
    pub fn preview_data(&self) -> Vec<u8> {
        let f = self.bh.borrow();
        let buf = BlockheadPreviewDataNew {
            skin_options: f.skin_options,
            hat_item_type: f.hat_item_type,
            pants_item_type: f.pants_item_type,
            shirt_item_type: f.shirt_item_type,
            shoes_item_type: f.shoes_item_type,
            hat_color: f.hat_color_index,
            pants_color: f.pants_color_index,
            shirt_color: f.shirt_color_index,
            shoes_color: f.shoes_color_index,
            happiness: f32_to_i32(f.state.happiness * 127.0f32) as u8,
            dead: 0,
            _padding: [0; 2],
        };
        let mut out = buf.to_vec();
        if let Some(n) = &f.name {
            out.extend_from_slice(n.as_bytes());
        }
        out
    }

    /// `-creationNetDataForClient:` Writes the string `"no name"` into the ivar when the name
    /// is nil. The XML trailer is `{name, clientID?, clientName?}`.
    pub fn creation_net_data_for_client_impl(&self, _client_id_to_send_to: Option<&str>) -> Vec<u8> {
        let me = self.me();
        let nd = DynamicObject::dynamic_object_net_data(self, &me);
        let mut f = self.bh.borrow_mut();
        let cd = BlockheadCreationData {
            dynamic_object_net_data: nd,
            skin_options: f.skin_options,
            hat_item_type: f.hat_item_type,
            pants_item_type: f.pants_item_type,
            shirt_item_type: f.shirt_item_type,
            shoes_item_type: f.shoes_item_type,
            hat_color: f.hat_color_index,
            pants_color: f.pants_color_index,
            shirt_color: f.shirt_color_index,
            shoes_color: f.shoes_color_index,
            view_radius: f.view_radius as u16,
            happiness: f32_to_i32(f.state.happiness * 127.0f32) as u8,
            dead: 0,
            animation_type: f.state.animation_type as u8,
            sub_animation_type: f.state.sub_animation_type as u8,
            traverse_from_key_frame: f.traverse_from_key_frame as u8,
            traverse_to_key_frame: f.traverse_to_key_frame as u8,
            regenerating: f.state.regenerating,
            on_trade_mission: f.state.on_trade_mission,
            padding: [0; 2],
        };
        let mut data = cd.to_vec();
        if f.name.is_none() {
            f.name = Some("no name".to_string());
        }
        let mut d = new_mutable_dict();
        d.map.insert("name".to_string(), PlistValue::string(f.name.clone().unwrap()));
        if let Some(c) = &f.client_id {
            d.map.insert("clientID".to_string(), PlistValue::string(c.clone()));
        }
        if let Some(c) = &f.client_name {
            d.map.insert("clientName".to_string(), PlistValue::string(c.clone()));
        }
        drop(f);
        if let Some(x) = data_from_property_list(&PlistValue::Dict(d)) {
            data.extend_from_slice(&x);
        }
        data
    }

    /// `-updateNetDataForClient:` Returns the 0x70 bytes of one `BlockheadUpdateData`.
    pub fn update_net_data_for_client_impl(&self, _client_id_to_send_to: Option<&str>) -> Vec<u8> {
        let me = self.me();
        let nd = DynamicObject::dynamic_object_net_data(self, &me);
        let world = self.world_rc();
        let h = host();
        let is_net = self.is_net();
        let dw_is_server = self.dw().map_or(false, |d| d.is_server());
        let w_tiles = world.as_ref().map_or(0, |w| w.world_width_macro().wrapping_mul(32));
        let is_admin = world.as_ref().map_or(false, |w| w.is_admin());

        let mut u = BlockheadUpdateData::default();
        u.dynamic_object_net_data = nd;
        // interaction item (needs `currentItem` from the inventory module, called before borrowing)
        let cur_item_type: u16 = if !is_net {
            let (mut cur, interacting) = {
                let f = self.bh.borrow();
                (f.interaction_item.clone(), f.state.interacting != 0)
            };
            if cur.is_none() && !interacting {
                cur = h.current_item(self);
            }
            cur.map_or(0, |c| c.borrow().item_type.0 as u16)
        } else {
            self.bh.borrow().remote_interaction_item_type as u16
        };
        let fishing_valid = {
            let f = self.bh.borrow();
            f.fishing_rod.as_ref().map_or(false, |r| h.fishing_rod_valid(r))
        };
        let hooked_fish = {
            let f = self.bh.borrow();
            f.fishing_rod.as_ref().map_or(0u64, |r| h.fishing_rod_hooked_fish_unique_id(r))
        };

        let mut f = self.bh.borrow_mut();
        u.from_square_x = f.from_square.x as u32;
        u.from_square_y = f.from_square.y as u32;
        u.to_square_x = f.to_square.x as u32;
        u.to_square_y = f.to_square.y as u32;
        u.paused = f.paused as u8;
        let ix = f.state.interacting_square.x;
        u.interacting_square_x = if ix < 0 { ix.wrapping_add(w_tiles) as u32 } else { ix as u32 };
        u.interacting_square_y = f.state.interacting_square.y as u32;
        u.traverse_type = f.traverse_type as u8;
        u.traverse_from_key_frame = f.traverse_from_key_frame as u8;
        u.traverse_to_key_frame = f.traverse_to_key_frame as u8;
        u.happiness = f32_to_i32(f.state.happiness * 127.0f32) as u8;
        u.health = f32_to_i32(f.state.health * 127.0f32) as u8;
        u.heat = f32_to_i32(f.state.heat) as u8;
        u.has_coffee_energy = (0.01f32 < f.state.coffee_energy_timer) as u8;
        u.regenerating = f.state.regenerating;
        u.on_trade_mission = f.state.on_trade_mission;
        u.interaction_item_type = cur_item_type;
        u.interacting = f.state.interacting;
        u.terrain_difficulty = f.terrain_difficulty as u16;
        u.interaction_type = f.state.interaction_type as u8;
        u.animation_type = f.state.animation_type as u8;
        u.sub_animation_type = f.state.sub_animation_type as u8;
        // `[dynamicWorld activeBlockhead] == self` is false on the server (activeBlockhead is nil)
        u.is_currently_active = f.is_currently_active_blockhead_according_to_net_data as u8;
        u.ride_object_id = f.ride_object.as_ref().map_or(0, |o| o.unique_id());
        u.interaction_object_id = f.interaction_object.as_ref().map_or(0, |o| o.unique_id());
        u.interaction_workbench_id = f.interaction_workbench.as_ref().map_or(0, |o| o.unique_id());

        let proceed = (!is_net && fishing_valid) || dw_is_server;
        if proceed && (f.fishing_rod_cast_x != 0 || f.fishing_rod_cast_y != 0) {
            u.fishing_rod_cast_x = f.fishing_rod_cast_x as i32;
            u.fishing_rod_cast_y = f.fishing_rod_cast_y as i32;
            u.fishing_rod_fish_unique_id =
                if !is_net { hooked_fish } else { f.remote_client_fishing_rod_fish_unique_id };
        } else {
            u.fishing_rod_cast_x = 0;
            u.fishing_rod_cast_y = 0;
            u.fishing_rod_fish_unique_id = 0;
        }
        u.z_index = if f.z_index == 1 { 1 } else { is_admin as u8 };
        u.is_offline = f.is_client_blockhead_being_controlled_by_server as u8;
        u.is_in_jet_pack_free_flight_mode = f.is_in_jet_pack_free_flight_mode as u8;
        if f.is_in_jet_pack_free_flight_mode {
            u.fishing_rod_cast_x = f32_to_i32(10000.0f32 * f.jet_pack_velocity.x);
            u.fishing_rod_cast_y = f32_to_i32(10000.0f32 * f.jet_pack_velocity.y);
            f.jetpack_send_update_timer = 0.0;
        }
        // u.dead (0x39) and u.padding (0x6f) are never written in the original: 0 (types_structs 6)
        u.to_vec()
    }

    // ================================================================================================
    // ================================================================================================

    /// `-getSaveDictIncludingWorkbenchOrInterationObject:` (sic). The write-only keys
    /// (`controlledByServerCrystalDiscrepency`, `attackingNPC`, `chasingBoat`, `lastKnown*`) are written as listed.
    /// Note: a nil `name` would make `setObject:forKey:` raise in the original; it is skipped here.
    pub fn get_save_dict_including_workbench_or_interation_object(&self, include: bool) -> PlistDict {
        let me = self.me();
        let mut d = crate::objects::base::base_get_save_dict(&me);
        let world = self.world_rc();
        let h = host();
        {
            let f = self.bh.borrow();
            let set = |d: &mut PlistDict, k: &str, v: PlistValue| {
                d.map.insert(k.to_string(), v);
            };
            set(&mut d, "state", PlistValue::data(f.state.to_save_blob()));
            set(&mut d, "selectedToolIndex", PlistValue::int(f.selected_tool_index as i64));
            set(&mut d, "interactionItemIndex", PlistValue::int(f.interaction_item_index as i64));
            set(&mut d, "interactionItemSubIndex", PlistValue::int(f.interaction_item_sub_index as i64));
            set(&mut d, "clothingIncrementTimer", PlistValue::int(f32_to_i32(f.clothing_increment_timer) as i64));
            if let Some(n) = &f.name {
                set(&mut d, "name", PlistValue::string(n.clone()));
            }
            set(&mut d, "skinOptions", PlistValue::data(f.skin_options.to_vec()));
            set(
                &mut d,
                "doubleTimeUnlocked",
                PlistValue::boolean(world.as_ref().map_or(false, |w| w.double_time_unlocked())),
            );
            if f.controlled_by_server_crystal_discrepency != 0 {
                set(
                    &mut d,
                    "controlledByServerCrystalDiscrepency",
                    PlistValue::int(f.controlled_by_server_crystal_discrepency as i64),
                );
            }
            // `[NSMutableArray array]` filled with the action dictionaries
            let acts = PlistValue::Array(crate::plist::value::PlistArray {
                items: f.action_queue.iter().map(|act| h.action_get_save_dict(act)).collect(),
                tag: crate::plist::value::ClassTag {
                    origin: crate::plist::value::Origin::Built,
                    mutable: true,
                    alt: false,
                },
            });
            set(&mut d, "actions", acts);
            // client-only block (`dynamicWorld isClient`): expectedMaximumCrystalDiscrepency (dead on server)

            if f.chasing_npc {
                set(&mut d, "attackingNPC", PlistValue::boolean(f.chasing_npc));
                set(&mut d, "lastKnownNPCPosition.x", PlistValue::int(f.last_known_npc_position.x as i64));
                set(&mut d, "lastKnownNPCPosition.y", PlistValue::int(f.last_known_npc_position.y as i64));
            } else if f.chasing_ride_object {
                set(&mut d, "chasingBoat", PlistValue::boolean(f.chasing_ride_object));
                set(&mut d, "lastKnownBoatPosition.x", PlistValue::int(f.last_known_ride_object_position.x as i64));
                set(&mut d, "lastKnownBoatPosition.y", PlistValue::int(f.last_known_ride_object_position.y as i64));
            } else if !f.last_path_was_falling {
                let path_count = f.path.as_ref().map_or(0, |p| p.len());
                if path_count != 0 || f.from_square != f.to_square {
                    set(&mut d, "loadRequiresRecalculation", PlistValue::boolean(true));
                    set(&mut d, "finalGoalSquare.x", PlistValue::int(f.final_goal_square.x as i64));
                    set(&mut d, "finalGoalSquare.y", PlistValue::int(f.final_goal_square.y as i64));
                }
            }
        }
        if include {
            let (wb, io) = {
                let f = self.bh.borrow();
                (f.interaction_workbench.clone(), f.interaction_object.clone())
            };
            if let Some(w) = wb {
                let sd = w.get_save_dict(&w);
                d.map.insert("interactionObjectDict".to_string(), PlistValue::Dict(sd));
            } else if let Some(o) = io {
                let sd = o.get_save_dict(&o);
                d.map.insert("interactionObjectDict".to_string(), PlistValue::Dict(sd));
            }
        }
        let f = self.bh.borrow();
        if f.state.on_trade_mission != 0 {
            d.map.insert("tradeMissionData".to_string(), PlistValue::data(f.current_trade_mission.to_vec()));
        }
        d
    }

    /// `-dealloc`: Rust frees owned fields on drop; the original's releases have no side effects (sounds and
    /// cubes are client-only).
    pub fn dealloc_impl(&self, _me: &ObjRef) {}
}

/// The name of a replica: on a server it goes through the curse word filter (the original also assigned the raw
/// name first, which leaked a retain and nothing else).
fn resolve_net_name(has_server: bool, raw: Option<String>) -> Option<String> {
    if has_server {
        host().remove_curse_words_from_blockhead_name(raw.as_deref())
    } else {
        raw
    }
}

#[cfg(test)]
mod tests {
    use super::super::blockhead::testutil::*;
    use super::*;
    use crate::items::food::item_type_from_food_type;
    use crate::rng::glibc_rand::{r01_from, rand, srand};
    use crate::types::structs::{CustomRules, Vector2};

    fn slot_types(b: &Blockhead, i: usize) -> Vec<u32> {
        b.bh.borrow().inventory_items[i].borrow().iter().map(|x| x.borrow().item_type.0).collect()
    }

    fn draws_after(seed: u32, n: usize) -> i32 {
        srand(seed);
        for _ in 0..n {
            rand();
        }
        rand()
    }

    /// : slot layout and rand() order for numbers 0, 1, 2, 3, -1.
    #[test]
    fn new_character_slot_layout_and_rng() {
        let _h = TestHost::install();
        let foods: Vec<i32> = (0..31).collect();
        for n in [0, 1, 2, 3, -1] {
            let (world, _dw, dwref) = setup();
            srand(42);
            let o = new_bh(&world, &dwref, n);
            let after = rand();
            let b = bh_of(&o);
            // rand draws: 0 -> 1 draw total; others -> 2
            let k = if n == 0 { 1 } else { 2 };
            assert_eq!(after, draws_after(42, k), "n={n}");
            // clothing container
            assert_eq!(slot_types(b, 0), vec![1]);
            assert!(b.bh.borrow().inventory_changed_this_frame_slots[0]);
            // tool
            srand(42);
            let r = r01_from(rand()) as f64;
            let expect_tool = match n {
                0 => 16,
                1 => [40, 49, 43][(0.33 > r) as usize + (0.66 > r) as usize],
                2 => [67, 66][(0.5 > r) as usize],
                _ => [90, 93][(0.5 > r) as usize],
            };
            assert_eq!(slot_types(b, 1), vec![expect_tool], "n={n}");
            // food x5
            let idx = match n {
                0 => 0,
                1 => 1,
                2 => 2,
                _ => 3,
            };
            let ft = item_type_from_food_type(crate::types::enums::FoodType(foods[idx] as u32)).0;
            assert_eq!(slot_types(b, 2), vec![ft; 5], "n={n}");
            let f = b.bh.borrow();
            let food0 = f.inventory_items[2].borrow()[0].clone();
            let fi = food0.borrow();
            if n == 0 {
                assert_eq!((fi.data_a, fi.data_b), (127, 127));
            }
            drop(fi);
            drop(f);
            // basket
            assert_eq!(slot_types(b, 3), vec![12]);
            // lights
            let lights = match n {
                0 => vec![],
                1 | 2 => vec![47; 10],
                _ => vec![150; 10],
            };
            assert_eq!(slot_types(b, 4), lights, "n={n}");
            assert_eq!(b.view_radius(), 22);
            let fs = b.bh.borrow();
            assert_eq!(fs.state.health, 1.0);
            assert_eq!(fs.state.fullness, 1.2f32);
            assert_eq!(fs.state.energy, 1.2f32);
            assert_eq!(fs.travel_speed, 4.0);
            assert_eq!(fs.travel_fraction, 1.0);
            assert_eq!(fs.save_item_slots_array.len(), 8);
            assert!(fs.random_idle_wait >= 5.0 && fs.random_idle_wait <= 15.0);
        }
    }

    #[test]
    fn expert_mode_and_custom_spawn() {
        let _h = TestHost::install();
        let (world, _dw, dwref) = setup();
        world.set_expert_mode(true);
        let mut rules = CustomRules::default();
        rules.custom_rules_enabled = 1;
        rules.spawn_item_types = [2, 16, 5, 31, 47, 0x127, 12, 12];
        rules.spawn_item_counts = [3, 120, 4, 2, 1, 2, 0, 5];
        world.set_custom_rules(rules);
        srand(9);
        let o = new_bh(&world, &dwref, 2);
        // expert mode skips the whole starter block, so no tool rand(): exactly the idle draw
        assert_eq!(rand(), draws_after(9, 1));
        let b = bh_of(&o);
        assert!(slot_types(b, 1).is_empty()); // marker 2 skipped
        assert_eq!(slot_types(b, 2), vec![16; 99]); // clamped to 99
        assert!(slot_types(b, 3).is_empty()); // marker 5
        assert_eq!(slot_types(b, 4), vec![31; 2]);
        assert_eq!(slot_types(b, 5), vec![47]);
        let s6 = b.bh.borrow().inventory_items[6].borrow().clone();
        assert_eq!(s6.len(), 2); // sowable 0x127 -> data 127
        assert_eq!((s6[0].borrow().data_a, s6[0].borrow().data_b), (127, 127));
        assert!(slot_types(b, 7).is_empty()); // n = 0
    }

    #[test]
    fn spawn_relocation() {
        let _h = TestHost::install();
        let (world, _dw, dwref) = setup();
        let o = Blockhead::init_with_world_dynamic_world_at_position_cache_blockhead_number_craftable_item_object_unique_id(
            &world,
            &dwref,
            IntPair { x: 5, y: 6 },
            None,
            0,
            None,
            77,
        )
        .unwrap();
        let b = bh_of(&o);
        assert_eq!(b.pos(), IntPair { x: 5, y: 8 });
        assert_eq!(b.bh.borrow().to_square, IntPair { x: 5, y: 8 });
        assert_eq!(b.unique_id(), 77);
        // floatPos is NOT updated by the relocation
        assert_eq!(b.float_pos(), Vector2 { x: 5.5, y: 6.0 });
        // no air within 4 tiles: falls back to the start portal pos
        world.set_start_portal_pos(IntPair { x: 2, y: 9 });
        let o2 = Blockhead::init_with_world_dynamic_world_at_position_cache_blockhead_number_craftable_item_object_unique_id(
            &world,
            &dwref,
            IntPair { x: 5, y: 2 },
            None,
            0,
            None,
            u64::MAX,
        )
        .unwrap();
        assert_eq!(bh_of(&o2).pos(), IntPair { x: 2, y: 9 });
        // a NULL start tile keeps fromTile = NULL (no relocation)
        let o3 = Blockhead::init_with_world_dynamic_world_at_position_cache_blockhead_number_craftable_item_object_unique_id(
            &world,
            &dwref,
            IntPair { x: 200, y: 8 },
            None,
            0,
            None,
            u64::MAX,
        )
        .unwrap();
        assert!(bh_of(&o3).bh.borrow().from_tile.is_none());
    }

    struct Craftable;
    impl BlockheadCraftableItemObjectLike for Craftable {
        fn name(&self) -> Option<String> {
            Some("Zed".into())
        }
        fn skin_options(&self) -> BlockheadSkinOptions {
            let mut s = BlockheadSkinOptions::default();
            s.is_male = 1;
            s.face_index = 3;
            s
        }
    }

    fn roundtrip_slots(b: &Blockhead) -> PlistValue {
        let f = b.bh.borrow();
        let slots: Vec<PlistValue> = f
            .inventory_items
            .iter()
            .map(|s| PlistValue::array(s.borrow().iter().map(|i| PlistValue::data(i.borrow().save_data())).collect()))
            .collect();
        PlistValue::array(slots)
    }

    /// : getSaveDict -> init(saveDict) round trip.
    #[test]
    fn load_round_trip() {
        let _h = TestHost::install();
        let (world, _dw, dwref) = setup();
        let o = Blockhead::init_with_world_dynamic_world_at_position_cache_blockhead_number_craftable_item_object_unique_id(
            &world,
            &dwref,
            IntPair { x: 5, y: 8 },
            None,
            1,
            Some(&Craftable),
            u64::MAX,
        )
        .unwrap();
        let b = bh_of(&o);
        assert_eq!(b.name().as_deref(), Some("Zed"));
        {
            let mut f = b.bh.borrow_mut();
            f.state.health = 0.5;
            f.selected_tool_index = 2;
            f.interaction_item_index = -1;
            f.interaction_item_sub_index = -1;
            f.clothing_increment_timer = 12.7;
            f.to_square = IntPair { x: 6, y: 8 };
            f.final_goal_square = IntPair { x: 9, y: 8 };
        }
        let sd = b.get_save_dict_including_workbench_or_interation_object(false);
        assert!(sd.map.contains_key(&"loadRequiresRecalculation".to_string()));
        assert_eq!(sd.map.get(&"finalGoalSquare.x".to_string()).unwrap().int_value(), 9);
        assert_eq!(sd.map.get(&"clothingIncrementTimer".to_string()).unwrap().int_value(), 12);
        assert!(sd.map.contains_key(&"actions".to_string()));
        assert!(!sd.map.contains_key(&"tradeMissionData".to_string()));
        let slots = roundtrip_slots(b);
        let o2 = Blockhead::init_with_world_dynamic_world_save_dict_saved_inventory_slots_cache_reposition_on_load_failure_client_save_dir_client_locally_saved_dict(
            &world,
            &dwref,
            &sd,
            Some(&slots),
            None,
            true,
            None,
            None,
        )
        .unwrap();
        let b2 = bh_of(&o2);
        let f2 = b2.bh.borrow();
        assert_eq!(f2.state.health, 0.5);
        assert_eq!(f2.selected_tool_index, 2);
        assert_eq!(f2.clothing_increment_timer, 12.0);
        assert_eq!(f2.name.as_deref(), Some("Zed"));
        assert_eq!(f2.skin_options.face_index, 3);
        assert!(f2.load_requires_recalculation);
        assert!(f2.path_needs_recalculated);
        assert_eq!(f2.final_goal_square, IntPair { x: 9, y: 8 });
        assert_eq!(f2.view_radius, 22);
        assert_eq!(f2.inventory_items.len(), 8);
        assert_eq!(f2.inventory_items[3].borrow().len(), 1);
        assert_eq!(f2.save_item_slots_array[3].len(), 1);
        assert_eq!(f2.interaction_item_index, -1);
        assert!(f2.interaction_item.is_none());
        assert!(!f2.is_client_blockhead_being_controlled_by_server);
        drop(f2);
        assert_eq!(b2.unique_id(), b.unique_id());
        // clientSaveDir makes it a controlled-by-server object
        let o3 = Blockhead::init_with_world_dynamic_world_save_dict_saved_inventory_slots_cache_reposition_on_load_failure_client_save_dir_client_locally_saved_dict(
            &world,
            &dwref,
            &sd,
            Some(&slots),
            None,
            true,
            Some("players/a/b/c/"),
            None,
        )
        .unwrap();
        assert!(bh_of(&o3).is_client_blockhead_being_controlled_by_server());
        assert_eq!(bh_of(&o3).client_save_dir().as_deref(), Some("players/a/b/c/"));
    }

    #[test]
    fn save_dict_movement_block_and_trade() {
        let _h = TestHost::install();
        let (world, _dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        let b = bh_of(&o);
        let keys = |d: &PlistDict| -> Vec<String> { d.map.keys_in_order() };
        // idle: no movement keys
        let d = b.get_save_dict_including_workbench_or_interation_object(false);
        assert!(!keys(&d).contains(&"loadRequiresRecalculation".to_string()));
        // chasing NPC wins over the others
        {
            let mut f = b.bh.borrow_mut();
            f.chasing_npc = true;
            f.chasing_ride_object = true;
            f.last_known_npc_position = IntPair { x: 3, y: 4 };
            f.to_square = IntPair { x: 9, y: 9 };
        }
        let d = b.get_save_dict_including_workbench_or_interation_object(false);
        let k = keys(&d);
        assert!(k.contains(&"attackingNPC".to_string()) && k.contains(&"lastKnownNPCPosition.y".to_string()));
        assert!(!k.contains(&"chasingBoat".to_string()) && !k.contains(&"loadRequiresRecalculation".to_string()));
        b.bh.borrow_mut().chasing_npc = false;
        let d = b.get_save_dict_including_workbench_or_interation_object(false);
        assert!(keys(&d).contains(&"chasingBoat".to_string()));
        b.bh.borrow_mut().chasing_ride_object = false;
        b.bh.borrow_mut().last_path_was_falling = true;
        let d = b.get_save_dict_including_workbench_or_interation_object(false);
        assert!(!keys(&d).contains(&"loadRequiresRecalculation".to_string()));
        // trade mission data
        b.bh.borrow_mut().state.on_trade_mission = 1;
        b.bh.borrow_mut().current_trade_mission.count = 4;
        let d = b.get_save_dict_including_workbench_or_interation_object(false);
        let data = d.map.get(&"tradeMissionData".to_string());
        let Some(PlistValue::Data(x)) = data else { unreachable!() };
        assert_eq!(x.bytes.len(), 0x20);
        assert_eq!(x.bytes[0x18], 4);
        b.bh.borrow_mut().controlled_by_server_crystal_discrepency = 5;
        let d = b.get_save_dict_including_workbench_or_interation_object(false);
        assert_eq!(d.map.get(&"controlledByServerCrystalDiscrepency".to_string()).unwrap().int_value(), 5);
    }

    #[test]
    fn load_bad_position_repair() {
        let _h = TestHost::install();
        let (world, _dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        let b = bh_of(&o);
        let mut sd = b.get_save_dict_including_workbench_or_interation_object(false);
        // move into solid rock
        sd.map.insert("pos_x".to_string(), PlistValue::int(5));
        sd.map.insert("pos_y".to_string(), PlistValue::int(3));
        sd.map.insert(
            "floatPos".to_string(),
            PlistValue::array(vec![PlistValue::real_f32(5.5), PlistValue::real_f32(3.0)]),
        );
        let slots = roundtrip_slots(b);
        // repositionOnLoadFailure NO: nil
        let r = Blockhead::init_with_world_dynamic_world_save_dict_saved_inventory_slots_cache_reposition_on_load_failure_client_save_dir_client_locally_saved_dict(
            &world, &dwref, &sd, Some(&slots), None, false, None, None,
        );
        assert!(r.is_none());
        // YES: search up first (k = 0: (x, y+1)), needs tile light != 0 for tileIsLitForSelf
        for y in 0..32 {
            for x in 0..32 {
                tile(&world, x, y).set_light(255);
            }
        }
        let r = Blockhead::init_with_world_dynamic_world_save_dict_saved_inventory_slots_cache_reposition_on_load_failure_client_save_dir_client_locally_saved_dict(
            &world, &dwref, &sd, Some(&slots), None, true, None, None,
        )
        .unwrap();
        // up: (5,4..7) are rock; the search order for k = 0..15 tries (x, y+k+1) first: (5,4), (5,2), (6,3), (4,3), then k = 1...
        // first air tile in that order: k = 4 -> (5, 3+5) = (5, 8)
        assert_eq!(bh_of(&r).pos(), IntPair { x: 5, y: 8 });
        assert_eq!(bh_of(&r).float_pos(), Vector2 { x: 5.5, y: 8.0 });
    }

    /// : a bad slot array raises, a missing one is empty.
    #[test]
    fn load_short_inventory_raises() {
        let _h = TestHost::install();
        let (world, _dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        let sd = bh_of(&o).get_save_dict_including_workbench_or_interation_object(false);
        let short = PlistValue::array(vec![PlistValue::array(vec![])]);
        let r = crate::foundation::exception::catch_ns(|| {
            Blockhead::init_with_world_dynamic_world_save_dict_saved_inventory_slots_cache_reposition_on_load_failure_client_save_dir_client_locally_saved_dict(
                &world, &dwref, &sd, Some(&short), None, true, None, None,
            )
        });
        let e = r.err().expect("raises");
        assert_eq!(e.name, "NSRangeException");
        // nil array: empty slots, clothing container created
        let o2 = Blockhead::init_with_world_dynamic_world_save_dict_saved_inventory_slots_cache_reposition_on_load_failure_client_save_dir_client_locally_saved_dict(
            &world, &dwref, &sd, None, None, true, None, None,
        )
        .unwrap();
        assert_eq!(slot_types(bh_of(&o2), 0), vec![1]);
    }

    /// : serialisation byte vectors.
    #[test]
    fn net_serialisation() {
        let _h = TestHost::install();
        let (world, _dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        let b = bh_of(&o);
        {
            let mut f = b.bh.borrow_mut();
            f.skin_options.face_index = 9;
            f.hat_item_type = 0x8c;
            f.pants_item_type = 0x8d;
            f.shirt_item_type = 0x116;
            f.shoes_item_type = 0x7c;
            f.hat_color_index = 1;
            f.shirt_color_index = 2;
            f.pants_color_index = 3;
            f.shoes_color_index = 4;
            f.state.happiness = 0.5;
            f.view_radius = 22;
            f.state.animation_type = 7;
            f.traverse_to_key_frame = 3;
        }
        // name nil -> "no name" stored into the ivar
        let cd = b.creation_net_data_for_client_impl(None);
        assert_eq!(b.name().as_deref(), Some("no name"));
        assert!(cd.len() > 0x48);
        let c = BlockheadCreationData::read_le(&cd);
        assert_eq!(c.skin_options.face_index, 9);
        assert_eq!(
            (c.hat_item_type, c.pants_item_type, c.shirt_item_type, c.shoes_item_type),
            (0x8c, 0x8d, 0x116, 0x7c)
        );
        assert_eq!((c.hat_color, c.pants_color, c.shirt_color, c.shoes_color), (1, 3, 2, 4));
        assert_eq!(c.view_radius, 22);
        assert_eq!(c.happiness, 63);
        assert_eq!(c.animation_type, 7);
        assert_eq!(c.traverse_to_key_frame, 3);
        assert_eq!((c.dead, c.padding), (0, [0, 0]));
        assert_eq!(c.dynamic_object_net_data.unique_id, b.unique_id());
        let trailer = property_list_with_data_raw(&cd[0x48..], 0).unwrap();
        assert_eq!(trailer.object_for_key("name").unwrap().as_str(), Some("no name"));
        assert!(trailer.object_for_key("clientID").is_none());
        b.set_client_id("cid");
        b.set_client_name(Some("cname"));
        let cd2 = b.creation_net_data_for_client_impl(None);
        let t2 = property_list_with_data_raw(&cd2[0x48..], 0).unwrap();
        assert_eq!(t2.object_for_key("clientID").unwrap().as_str(), Some("cid"));
        assert_eq!(t2.object_for_key("clientName").unwrap().as_str(), Some("cname"));
        assert!(String::from_utf8_lossy(&cd2[0x48..]).starts_with("<?xml"));

        // update data
        {
            let mut f = b.bh.borrow_mut();
            f.state.health = 1.0;
            f.state.heat = 12.9;
            f.state.coffee_energy_timer = 0.5;
            f.state.interacting_square = IntPair { x: -3, y: 4 };
            f.from_square = IntPair { x: 1, y: 2 };
            f.to_square = IntPair { x: 3, y: 4 };
            f.terrain_difficulty = 33;
            f.paused = true;
            f.fishing_rod_cast_x = -7;
            f.fishing_rod_cast_y = 8;
        }
        let ud = b.update_net_data_for_client_impl(None);
        assert_eq!(ud.len(), 0x70);
        let u = BlockheadUpdateData::read_le(&ud);
        assert_eq!(u.interacting_square_x, (-3i32 + 1024) as u32); // W = 32 * 32
        assert_eq!(u.interacting_square_y, 4);
        assert_eq!((u.from_square_x, u.from_square_y, u.to_square_x, u.to_square_y), (1, 2, 3, 4));
        assert_eq!(u.health, 127);
        assert_eq!(u.happiness, 63);
        assert_eq!(u.heat, 12);
        assert_eq!(u.has_coffee_energy, 1);
        assert_eq!(u.terrain_difficulty, 33);
        assert_eq!(u.paused, 1);
        // zIndex: world is admin on the server -> 1
        assert_eq!(u.z_index, 1);
        // dw.isServer -> the cast fields are written
        assert_eq!(u.fishing_rod_cast_x, -7);
        assert_eq!(u.fishing_rod_cast_y, 8);
        assert_eq!((u.dead, u.padding_byte()), (0, 0));
        // jetpack override
        {
            let mut f = b.bh.borrow_mut();
            f.is_in_jet_pack_free_flight_mode = true;
            f.jet_pack_velocity = Vector2 { x: 0.5, y: -0.25 };
            f.jetpack_send_update_timer = 3.0;
        }
        let u = BlockheadUpdateData::read_le(&b.update_net_data_for_client_impl(None));
        assert_eq!((u.fishing_rod_cast_x, u.fishing_rod_cast_y), (5000, -2500));
        assert_eq!(u.is_in_jet_pack_free_flight_mode, 1);
        assert_eq!(b.bh.borrow().jetpack_send_update_timer, 0.0);
        // preview data
        b.bh.borrow_mut().name = Some("Zed".into());
        let pd = b.preview_data();
        assert_eq!(pd.len(), 0x28 + 3);
        let p = BlockheadPreviewDataNew::read_le(&pd);
        assert_eq!(
            (p.hat_item_type, p.pants_item_type, p.shirt_item_type, p.shoes_item_type),
            (0x8c, 0x8d, 0x116, 0x7c)
        );
        assert_eq!((p.hat_color, p.pants_color, p.shirt_color, p.shoes_color), (1, 3, 2, 4));
        assert_eq!(&pd[0x28..], b"Zed");
        assert_eq!(p.happiness, 63);
    }

    trait PadByte {
        fn padding_byte(&self) -> u8;
    }
    impl PadByte for BlockheadUpdateData {
        fn padding_byte(&self) -> u8 {
            self.to_vec()[0x6f]
        }
    }

    ///  /: the replica constructor.
    #[test]
    fn net_constructor_and_setup() {
        let _h = TestHost::install();
        let (world, _dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        let b = bh_of(&o);
        {
            let mut f = b.bh.borrow_mut();
            f.name = Some("Rep".into());
            f.hat_item_type = 0x8c;
            f.shirt_item_type = 0x116;
            f.shirt_color_index = 0;
            f.pants_item_type = 0x8d;
            f.pants_color_index = 0x1000;
            f.state.happiness = 1.0;
            f.state.animation_type = 23;
            f.traverse_to_key_frame = 2;
            f.traverse_from_key_frame = 1;
            f.state.on_trade_mission = 1;
            f.state.regenerating = 1;
            f.view_radius = 22;
        }
        b.set_client_id("cid-1");
        b.set_client_name(Some("Player"));
        let data = b.creation_net_data_for_client_impl(None);
        let o2 = Blockhead::init_with_world_dynamic_world_cache_net_data(&world, &dwref, None, &data).unwrap();
        let r = bh_of(&o2);
        assert!(r.is_net());
        let f = r.bh.borrow();
        assert_eq!(f.name.as_deref(), Some("Rep"));
        assert_eq!(f.client_id.as_deref(), Some("cid-1"));
        assert_eq!(f.client_name.as_deref(), Some("Player"));
        assert_eq!(f.hat_item_type, 0x8c);
        // colour 0 -> default colour; colour != 0 -> 0.2 + mixed * 0.8
        assert_eq!(f.shirt_item_color, crate::items::colors::default_color_for_item_type(ItemType(0x116)).v);
        let m = crate::items::colors::mixed_color_for_index(0x1000, ItemType(0x8d)).v;
        assert_eq!(
            f.pants_item_color,
            [0.2f32 + m[0] * 0.8f32, 0.2f32 + m[1] * 0.8f32, 0.2f32 + m[2] * 0.8f32, 0.2f32 + m[3] * 0.8f32]
        );
        assert_eq!(f.state.happiness, 127.0f32 / 127.0f32);
        assert_eq!(f.traverse_to_key_frame, 2);
        assert_eq!(f.traverse_from_key_frame, 1);
        assert_eq!(f.state.on_trade_mission, 1);
        assert_eq!(f.state.regenerating, 1);
        assert_eq!(f.view_radius, 22);
        assert_eq!(f.net_next_to_square, [f.from_square; 4]);
        assert_eq!(f.next_terrain_difficulty, [10; 4]);
        assert_eq!(f.travel_fraction, 1.0);
        // updateAnimation: isNet keeps the animation type but adjusts floatPos.y for 23
        assert_eq!(f.state.animation_type, 23);
        drop(f);
        // an unparsable trailer yields nil name/clientID/clientName without panicking
        let mut bad = data[..0x48].to_vec();
        bad.extend_from_slice(b"not a plist");
        let o3 = Blockhead::init_with_world_dynamic_world_cache_net_data(&world, &dwref, None, &bad).unwrap();
        assert!(bh_of(&o3).name().is_none() && bh_of(&o3).client_id().is_none());
        // netData shorter than 0x48 raises NSRangeException from subdataWithRange:
        let r = crate::foundation::exception::catch_ns(|| {
            Blockhead::init_with_world_dynamic_world_cache_net_data(&world, &dwref, None, &data[..0x20])
        });
        assert!(r.is_err());
    }

    #[test]
    fn net_interaction_object_was_loaded_matches_id() {
        let h = TestHost::install();
        let _ = h;
        let (world, _dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        let b = bh_of(&o);
        let obj = stub(1, true);
        obj.base().borrow_mut().unique_id = 55;
        let mut d = crate::objects::base::new_mutable_dict();
        d.map.insert("uniqueID".to_string(), PlistValue::int(54));
        b.bh.borrow_mut().interaction_object_dict_to_restore_when_object_loaded = Some(PlistValue::Dict(d.clone()));
        b.net_interaction_object_was_loaded(&obj);
        assert!(b.bh.borrow().interaction_object_dict_to_restore_when_object_loaded.is_some());
        d.map.insert("uniqueID".to_string(), PlistValue::int(55));
        b.bh.borrow_mut().interaction_object_dict_to_restore_when_object_loaded = Some(PlistValue::Dict(d));
        b.set_update_needs_to_be_sent(false);
        b.net_interaction_object_was_loaded(&obj);
        assert!(b.bh.borrow().interaction_object_dict_to_restore_when_object_loaded.is_none());
        assert!(o.update_needs_to_be_sent());
    }

    type LoadResult = Option<ObjRef>;

    fn load(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        sd: &PlistDict,
        slots: Option<&PlistValue>,
        dir: Option<&str>,
    ) -> LoadResult {
        Blockhead::init_with_world_dynamic_world_save_dict_saved_inventory_slots_cache_reposition_on_load_failure_client_save_dir_client_locally_saved_dict(
            world, dw, sd, slots, None, true, dir, None,
        )
    }

    #[test]
    fn starter_tools_cover_every_roll() {
        let _h = TestHost::install();
        for (n, expect) in [(1, vec![43u32, 49, 40]), (2, vec![66, 67]), (3, vec![93, 90]), (-1, vec![93, 90])] {
            let mut seen = std::collections::BTreeSet::new();
            for seed in 1..80u32 {
                let (world, _dw, dwref) = setup();
                srand(seed);
                let o = new_bh(&world, &dwref, n);
                seen.insert(slot_types(bh_of(&o), 1)[0]);
            }
            assert_eq!(seen.into_iter().collect::<Vec<_>>(), {
                let mut e = expect.clone();
                e.sort();
                e
            });
        }
    }

    #[test]
    fn dodo_eggs_as_the_first_food_and_custom_rules_that_skip_starters() {
        let _h = TestHost::install();
        let (world, _dw, dwref) = setup();
        *world.distance_ordered_food_types.borrow_mut() = vec![13, 1, 2, 3];
        let o = new_bh(&world, &dwref, 0);
        let b = bh_of(&o);
        let egg = b.bh.borrow().inventory_items[2].borrow()[0].clone();
        assert_eq!((egg.borrow().item_type, egg.borrow().data_a, egg.borrow().data_b), (ItemType(91), 0, 0));
        // custom rules decide per category whether the default starter items are given
        let rules = CustomRules {
            custom_rules_enabled: 1,
            spawn_item_types: [0, 2, 0, 2, 0, 0, 0, 0],
            ..CustomRules::default()
        };
        world.set_custom_rules(rules);
        let o = new_bh(&world, &dwref, 1);
        let b = bh_of(&o);
        assert!(slot_types(b, 1).is_empty() && slot_types(b, 3).is_empty());
        assert_eq!(slot_types(b, 2).len(), 5);
        assert_eq!(slot_types(b, 4).len(), 10);
    }

    #[test]
    fn spawn_without_air_or_a_start_tile_fails() {
        let _h = TestHost::install();
        let (world, _dw, dwref) = setup();
        world.set_start_portal_pos(IntPair { x: 300, y: 300 });
        let r =
            Blockhead::init_with_world_dynamic_world_at_position_cache_blockhead_number_craftable_item_object_unique_id(
                &world,
                &dwref,
                IntPair { x: 5, y: 2 },
                None,
                0,
                None,
                u64::MAX,
            );
        assert!(r.is_none());
    }

    /// A character with clothes, an item held from a container and a running action, saved and loaded back.
    #[test]
    fn load_restores_clothes_actions_items_and_interactions() {
        let h = TestHost::install();
        let (world, _dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        let b = bh_of(&o);
        // clothes in the clothing container, a basket with an item in its second slot
        {
            let f = b.bh.borrow();
            let cont = f.inventory_items[0].borrow();
            let c0 = cont[0].borrow();
            let subs = c0.sub_items.as_ref().unwrap();
            subs[3].borrow_mut().push(InventoryItem::new(ItemType(0x8c), 0, 0x1000, None, None));
            subs[2].borrow_mut().push(InventoryItem::new(ItemType(0x116), 0, 0, None, None));
            subs[1].borrow_mut().push(InventoryItem::new(ItemType(0x8d), 0, 0, None, None));
            subs[0].borrow_mut().push(InventoryItem::new(ItemType(0x7c), 0, 0x1000, None, None));
            drop(c0);
            drop(cont);
            let basket = f.inventory_items[3].borrow()[0].clone();
            basket.borrow().sub_items.as_ref().unwrap()[1].borrow_mut().push(InventoryItem::new(
                ItemType(8),
                0,
                0,
                None,
                None,
            ));
        }
        {
            let mut f = b.bh.borrow_mut();
            f.interaction_item_index = 3;
            f.interaction_item_sub_index = 1;
            f.state.interacting = 1;
            f.state.interaction_type = it::INSPECT_NPC;
            f.action_queue.push(Rc::new(()));
        }
        let mut sd = b.get_save_dict_including_workbench_or_interation_object(false);
        assert!(matches!(sd.map.get(&"actions".to_string()), Some(PlistValue::Array(a)) if a.items.len() == 1));
        let slots = roundtrip_slots(b);
        // an old-style dictionary without skin options
        sd.map.remove(&"skinOptions".to_string());
        sd.map.insert("male".to_string(), PlistValue::boolean(true));
        sd.map.insert("headIndex".to_string(), PlistValue::int(2));
        sd.map.insert("skinIndex".to_string(), PlistValue::int(3));
        sd.map.insert("hairStyleIndex".to_string(), PlistValue::int(1));
        // the saved action is rebuilt through the host
        let mut ad = crate::objects::base::new_mutable_dict();
        ad.map.insert("goalTilePos.x".to_string(), PlistValue::int(5));
        sd.map.insert("actions".to_string(), PlistValue::array(vec![PlistValue::Dict(ad)]));
        let r = load(&world, &dwref, &sd, Some(&slots), None).unwrap();
        let r = bh_of(&r);
        {
            let f = r.bh.borrow();
            assert_eq!(
                (f.hat_item_type, f.shirt_item_type, f.pants_item_type, f.shoes_item_type),
                (0x8c, 0x116, 0x8d, 0x7c)
            );
            assert_eq!((f.hat_color_index, f.shoes_color_index), (0x1000, 0x1000));
            assert_eq!(f.skin_options, get_new_options_from_old(true, 2, 3, 1));
            assert_eq!(f.action_queue.len(), 1);
            // inspecting an NPC does not survive a reload (stopping drops the held item too)
            assert!(f.interaction_item.is_none());
            assert_eq!(f.state.interacting, 0);
        }
        // an interaction that has no action behind it is dropped, a fuel carrier stops interacting
        let mut bare = sd.clone();
        bare.map.remove(&"actions".to_string());
        bare.map.insert("state".to_string(), {
            let mut st = b.bh.borrow().state;
            st.interacting = 1;
            st.interaction_type = 5;
            PlistValue::data(st.to_save_blob())
        });
        let r2 = load(&world, &dwref, &bare, Some(&slots), None).unwrap();
        assert_eq!(bh_of(&r2).bh.borrow().state.interacting, 0);
        let mut busy = bare.clone();
        busy.map.insert(
            "actions".to_string(),
            PlistValue::array(vec![PlistValue::Dict(crate::objects::base::new_mutable_dict())]),
        );
        h.adding_fuel.set(true);
        let r3 = load(&world, &dwref, &busy, Some(&slots), None).unwrap();
        assert_eq!(bh_of(&r3).bh.borrow().state.interacting, 0);
        h.adding_fuel.set(false);
        // a walking character is flagged for path recalculation and its first action restarts
        let mut walking = busy.clone();
        walking.map.insert("loadRequiresRecalculation".to_string(), PlistValue::boolean(true));
        walking.map.insert("finalGoalSquare.x".to_string(), PlistValue::int(8));
        walking.map.insert("state".to_string(), PlistValue::data(b.bh.borrow().state.to_save_blob()));
        let r4 = load(&world, &dwref, &walking, Some(&slots), None).unwrap();
        assert!(bh_of(&r4).bh.borrow().path_needs_recalculated);
        // interacting characters get their tile back
        let mut st = b.bh.borrow().state;
        st.interacting = 1;
        st.interaction_type = 3;
        st.interacting_square = IntPair { x: 6, y: 8 };
        let mut inter = busy.clone();
        inter.map.insert("state".to_string(), PlistValue::data(st.to_save_blob()));
        let r5 = load(&world, &dwref, &inter, Some(&slots), None).unwrap();
        assert!(bh_of(&r5).bh.borrow().interacting_tile.is_some());
    }

    #[test]
    fn load_interaction_objects_trade_missions_and_positions() {
        let h = TestHost::install();
        let (world, _dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        let b = bh_of(&o);
        // the held item sits in the second slot of the basket in inventory slot 3
        let basket = b.bh.borrow().inventory_items[3].borrow()[0].clone();
        basket.borrow().sub_items.as_ref().unwrap()[1].borrow_mut().push(InventoryItem::new(
            ItemType(8),
            0,
            0,
            None,
            None,
        ));
        b.bh.borrow_mut().interaction_item_index = 3;
        b.bh.borrow_mut().interaction_item_sub_index = 1;
        let slots = roundtrip_slots(b);
        let mut sd = b.get_save_dict_including_workbench_or_interation_object(false);
        let held = bh_of(&load(&world, &dwref, &sd, Some(&slots), None).unwrap()).bh.borrow().interaction_item.clone();
        assert_eq!(held.unwrap().borrow().item_type, ItemType(8));
        // a trade mission block shorter than the struct is copied as far as it goes
        sd.map.insert("tradeMissionData".to_string(), PlistValue::data(vec![7u8; 4]));
        // a skin options entry of an unexpected kind is ignored
        sd.map.insert("skinOptions".to_string(), PlistValue::string("junk"));
        // the interaction object is not loaded yet: its dictionary waits; once it is, ownership is taken
        let mut od = crate::objects::base::new_mutable_dict();
        od.map.insert("uniqueID".to_string(), PlistValue::int(42));
        sd.map.insert("interactionObjectDict".to_string(), PlistValue::Dict(od));
        let r = bh_of(&load(&world, &dwref, &sd, Some(&slots), None).unwrap())
            .bh
            .borrow()
            .interaction_object_dict_to_restore_when_object_loaded
            .is_some();
        assert!(r);
        *h.io_by_id.borrow_mut() = Some(stub(46, true));
        h.take();
        load(&world, &dwref, &sd, Some(&slots), None).unwrap();
        assert!(h.take().contains(&"takeownership".to_string()));
        // a position whose float position disagrees is repaired, and a bad item index raises
        sd.map.insert(
            "floatPos".to_string(),
            PlistValue::array(vec![PlistValue::real_f32(9.5), PlistValue::real_f32(9.0)]),
        );
        let fixed = load(&world, &dwref, &sd, Some(&slots), None).unwrap();
        assert_eq!(bh_of(&fixed).float_pos(), Vector2 { x: 5.5, y: 8.0 });
        let mut bad = sd.clone();
        bad.map.insert("interactionItemIndex".to_string(), PlistValue::int(12));
        let e = crate::foundation::exception::catch_ns(|| load(&world, &dwref, &bad, Some(&slots), None).is_some())
            .unwrap_err();
        assert_eq!(e.name, "NSRangeException");
        let mut bad_sub = sd.clone();
        bad_sub.map.insert("interactionItemIndex".to_string(), PlistValue::int(3));
        bad_sub.map.insert("interactionItemSubIndex".to_string(), PlistValue::int(9));
        let e = crate::foundation::exception::catch_ns(|| load(&world, &dwref, &bad_sub, Some(&slots), None).is_some())
            .unwrap_err();
        assert_eq!(e.name, "NSRangeException");
        // an empty basket slot keeps the container itself as the interaction item
        let mut empty_sub = sd.clone();
        empty_sub.map.insert("interactionItemIndex".to_string(), PlistValue::int(3));
        empty_sub.map.insert("interactionItemSubIndex".to_string(), PlistValue::int(2));
        let kept = load(&world, &dwref, &empty_sub, Some(&slots), None).unwrap();
        assert_eq!(bh_of(&kept).bh.borrow().interaction_item.as_ref().unwrap().borrow().item_type, ItemType(12));
        // the start portal tile is unusable too: the load fails
        world.set_start_portal_pos(IntPair { x: 300, y: 300 });
        let mut lost = sd.clone();
        lost.map.insert("pos_x".to_string(), PlistValue::int(5));
        lost.map.insert("pos_y".to_string(), PlistValue::int(900));
        assert!(load(&world, &dwref, &lost, Some(&slots), None).is_none());
    }

    #[test]
    fn replica_names_dead_state_and_dictionaries() {
        let _h = TestHost::install();
        assert_eq!(resolve_net_name(false, Some("A".into())).as_deref(), Some("A"));
        assert_eq!(resolve_net_name(true, Some("B".into())).as_deref(), Some("B"));
        assert_eq!(resolve_net_name(true, None), None);
        let (world, _dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        let b = bh_of(&o);
        b.dealloc_impl(&o);
        // a replica of every clothing piece with colours
        {
            let mut f = b.bh.borrow_mut();
            f.hat_item_type = 0x8c;
            f.hat_color_index = 0x1000;
            f.shirt_item_type = 0x116;
            f.shirt_color_index = 0x1000;
            f.pants_item_type = 0x8d;
            f.pants_color_index = 0x1000;
            f.shoes_item_type = 0x7c;
            f.shoes_color_index = 0x1000;
        }
        let data = b.creation_net_data_for_client_impl(None);
        let rep = Blockhead::init_with_world_dynamic_world_cache_net_data(&world, &dwref, None, &data).unwrap();
        let rb = bh_of(&rep).bh.borrow();
        let want = |t: u32| tinted_color(0x1000, ItemType(t));
        assert_eq!(rb.shoes_item_color, want(0x7c));
        assert_eq!(rb.hat_item_color, want(0x8c));
        drop(rb);
        // an unloaded position refuses the replica
        let mut far = data.clone();
        far[8..12].copy_from_slice(&900u32.to_le_bytes());
        far[12..16].copy_from_slice(&900u32.to_le_bytes());
        assert!(Blockhead::init_with_world_dynamic_world_cache_net_data(&world, &dwref, None, &far).is_none());
        // update data: replicas read their item and fishing state back from what they were told
        let r = bh_of(&rep);
        r.bh.borrow_mut().remote_interaction_item_type = 0x66;
        r.bh.borrow_mut().remote_client_fishing_rod_fish_unique_id = 9;
        r.bh.borrow_mut().fishing_rod_cast_x = 4;
        let u = BlockheadUpdateData::read_le(&r.update_net_data_for_client_impl(None));
        assert_eq!(u.interaction_item_type, 0x66);
        // the owner's interaction objects ride along in the save dictionary
        let wb = stub(45, true);
        b.bh.borrow_mut().interaction_workbench = Some(wb.clone());
        assert!(b
            .get_save_dict_including_workbench_or_interation_object(true)
            .map
            .contains_key(&"interactionObjectDict".to_string()));
        b.bh.borrow_mut().interaction_workbench = None;
        b.bh.borrow_mut().interaction_object = Some(stub(46, true));
        assert!(b
            .get_save_dict_including_workbench_or_interation_object(true)
            .map
            .contains_key(&"interactionObjectDict".to_string()));
    }
}

#[cfg(test)]
mod cov_tests;
