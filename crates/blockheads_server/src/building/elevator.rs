//! `ElevatorShaft: DynamicObject` (type 56) and `ElevatorMotor: DynamicObject <PowerUser>` (type 55).
//!
//!  ElevatorShaft";
//! Class `ElevatorMotor`" (10.1 to 10.3) and 0.2 `ElevatorMotorCreationNetData`.
//! Client-only members (`draw:`, `opening` animation, `addDraw*`) are not ported.

use std::cell::RefCell;
use std::rc::Rc;

use super::column::{
    append_plist_tail, owner_from_client_or_save, read_plist_tail, sd_get, sd_string, tail_string, Links,
};
use crate::dynworld::state::DynamicWorld;
use crate::objects::base::{
    base_dynamic_object_net_data, base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_cache_net_data, base_init_with_world_dynamic_world_save_dict_cache,
    base_remote_update, padded_bytes, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::objects::protocols::PowerUser;
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::netdata::{ElevatorMotorCreationNetData, ElevatorShaftCreationNetData};
use crate::types::structs::{IntPair, WireStruct};
use crate::world::helpers::tile_is_solid;
use crate::world::state::World;
use crate::world::tiles::tile_at_world_position_loaded;

/// `DYNAMIC_OBJECT_TYPE_ELEVATOR_SHAFT`.
pub const DYNAMIC_OBJECT_TYPE_ELEVATOR_SHAFT: i32 = 56;
/// `DYNAMIC_OBJECT_TYPE_ELEVATOR_MOTOR`.
pub const DYNAMIC_OBJECT_TYPE_ELEVATOR_MOTOR: i32 = 55;
/// `CONTENTS_ELEVATOR_SHAFT` (tile `contents`).
pub const CONTENTS_ELEVATOR_SHAFT: u8 = 103;

/// `ElevatorShaft` class descriptor.
pub static ELEVATOR_SHAFT_CLASS: ClassInfo =
    ClassInfo { name: "ElevatorShaft", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &[] };
/// `ElevatorMotor` class descriptor (adopts `PowerUser`).
pub static ELEVATOR_MOTOR_CLASS: ClassInfo =
    ClassInfo { name: "ElevatorMotor", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &["PowerUser"] };

// =====================================================================================================
// ElevatorShaft
// =====================================================================================================

/// ElevatorShaft ivars (`opening`/`openTimer` are client animation only; `opening` kept as a field).
pub struct ShaftState {
    pub item_type: i32,
    pub last_known_motor_pos: IntPair,
    pub paint_color: u16,
    pub solid_tile: bool,
    pub opening: bool,
}

/// `ElevatorShaft: DynamicObject` (type 56).
pub struct ElevatorShaft {
    base: RefCell<DynamicObjectBase>,
    /// The ElevatorShaft ivars.
    pub st: RefCell<ShaftState>,
    links: Links,
}

impl ElevatorShaft {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<ElevatorShaft> {
        let links = Links::new(world, dw);
        handle::alloc_obj(|sw| ElevatorShaft {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            st: RefCell::new(ShaftState {
                item_type: 0,
                last_known_motor_pos: IntPair { x: 0, y: 0 },
                paint_color: 0,
                solid_tile: false,
                opening: false,
            }),
            links,
        })
    }

    /// Placement constructor. Order: base, itemType, owner, paint, `lastKnownMotorPos = pos`, `initSubDerivedItems`.
    pub fn init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        item_type: i32,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        if !base_init_with_world_dynamic_world_at_position_cache(&me_obj, &wref, &dref, pos, cache) {
            return None;
        }
        me.st.borrow_mut().item_type = item_type;
        if let Some(o) = owner_from_client_or_save(client_id, save_dict) {
            me.base.borrow_mut().owner_id = Some(o);
        }
        let own_pos = me.base.borrow().pos;
        {
            let mut st = me.st.borrow_mut();
            st.paint_color = sd_get(save_dict, "paintColor").map_or(0, |v| v.unsigned_int_value()) as u16;
            st.last_known_motor_pos = own_pos;
        }
        me.init_sub_derived_items();
        Some(me_obj)
    }

    /// Save constructor.
    pub fn init_with_world_dynamic_world_save_dict_cache(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        if !base_init_with_world_dynamic_world_save_dict_cache(&me_obj, &wref, &dref, save_dict, cache) {
            return None;
        }
        {
            let g = |k: &str| save_dict.map.get(&k.to_string());
            let mut st = me.st.borrow_mut();
            st.item_type = g("itemType").map_or(0, |v| v.int_value());
            st.last_known_motor_pos.x = g("lastKnownMotorPos.x").map_or(0, |v| v.int_value());
            st.last_known_motor_pos.y = g("lastKnownMotorPos.y").map_or(0, |v| v.int_value());
            st.paint_color = g("paintColor").map_or(0, |v| v.unsigned_int_value()) as u16;
        }
        me.base.borrow_mut().owner_id = sd_string(save_dict, "ownerID");
        me.init_sub_derived_items();
        Some(me_obj)
    }

    /// Net constructor (0x28 clamped read; `solidTile` recomputed).
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        if !base_init_with_world_dynamic_world_cache_net_data(&me_obj, &wref, &dref, cache, net_data) {
            return None;
        }
        let s = ElevatorShaftCreationNetData::read_le(&padded_bytes(net_data, 0x28));
        {
            let mut st = me.st.borrow_mut();
            st.item_type = s.item_type as i32;
            st.last_known_motor_pos =
                IntPair { x: s.last_known_motor_pos_x as i32, y: s.last_known_motor_pos_y as i32 };
            st.paint_color = s.paint_color;
        }
        if net_data.len() > 0x28 {
            let owner = tail_string(&read_plist_tail(net_data, 0x28), "ownerID");
            me.base.borrow_mut().owner_id = owner;
        }
        me.init_sub_derived_items();
        Some(me_obj)
    }

    /// `-[ElevatorShaft initSubDerivedItems]`: the discarded tile lookup is kept (block-load side effect).
    pub fn init_sub_derived_items(&self) {
        let world = self.links.world();
        let lk = self.st.borrow().last_known_motor_pos;
        let _ = tile_at_world_position_loaded(lk.x, lk.y, &world);
        let pos = self.base.borrow().pos;
        if let Some(m) = self.links.dw().elevator_motor_for_shaft_at_pos(pos) {
            self.st.borrow_mut().last_known_motor_pos = m.pos();
        }
        let solid = tile_is_solid(tile_at_world_position_loaded(pos.x, pos.y, &world));
        self.st.borrow_mut().solid_tile = solid;
    }

    /// `-[ElevatorShaft itemType]`
    pub fn item_type(&self) -> i32 {
        self.st.borrow().item_type
    }
    /// `-[ElevatorShaft lastKnownMotorPos]`
    pub fn last_known_motor_pos(&self) -> IntPair {
        self.st.borrow().last_known_motor_pos
    }
    /// `-[ElevatorShaft open]`: client animation flag only.
    pub fn open(&self) {
        self.st.borrow_mut().opening = true;
    }
    /// `-[ElevatorShaft isPaintable]`
    pub fn is_paintable(&self) -> bool {
        true
    }
    /// `-[ElevatorShaft solidTile]` (serialised flag).
    pub fn solid_tile(&self) -> bool {
        self.st.borrow().solid_tile
    }

    /// `-[ElevatorShaft paint:]`.
    pub fn paint(&self, _me: &ObjRef, color_index: u16) {
        self.st.borrow_mut().paint_color = color_index;
        if !self.base.borrow().is_net {
            self.base.borrow_mut().update_needs_to_be_sent = true;
            let pos = self.base.borrow().pos;
            self.links.dw().dynamic_world_changed_at_pos_object_type(pos, DYNAMIC_OBJECT_TYPE_ELEVATOR_SHAFT);
        }
    }
}

impl DynamicObject for ElevatorShaft {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &ELEVATOR_SHAFT_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_ELEVATOR_SHAFT
    }
    fn occupies_normal_contents(&self) -> bool {
        true
    }
    fn freeblock_creation_item_type(&self) -> i32 {
        self.item_type()
    }
    fn free_block_creation_save_dict(&self, me: &ObjRef) -> Option<PlistDict> {
        Some(self.get_save_dict(me))
    }

    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = base_get_save_dict(me);
        let (it, lk, pc) = {
            let st = self.st.borrow();
            (st.item_type, st.last_known_motor_pos, st.paint_color)
        };
        d.map.insert("itemType".to_string(), PlistValue::int(it as i64));
        d.map.insert("lastKnownMotorPos.x".to_string(), PlistValue::int(lk.x as i64));
        d.map.insert("lastKnownMotorPos.y".to_string(), PlistValue::int(lk.y as i64));
        d.map.insert("paintColor".to_string(), PlistValue::int(pc as i64));
        if let Some(o) = self.base.borrow().owner_id.clone() {
            d.map.insert("ownerID".to_string(), PlistValue::string(o));
        }
        d
    }

    fn creation_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        let st = self.st.borrow();
        let s = ElevatorShaftCreationNetData {
            dynamic_object_net_data: base_dynamic_object_net_data(me),
            last_known_motor_pos_x: st.last_known_motor_pos.x as u32,
            last_known_motor_pos_y: st.last_known_motor_pos.y as u32,
            item_type: st.item_type as u16,
            paint_color: st.paint_color,
            solid_tile: st.solid_tile as u8,
            padding: [0; 3],
        };
        let mut out = s.to_vec();
        let owner = self.base.borrow().owner_id.clone();
        append_plist_tail(&mut out, &[("ownerID", owner.as_deref())]);
        Some(out)
    }
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        self.creation_net_data_for_client(me, client_id)
    }

    /// `-[ElevatorShaft remoteUpdate:]`: recompute `solidTile`, copy `paintColor` (itemType / motor pos untouched).
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        base_remote_update(me, net_data);
        let s = ElevatorShaftCreationNetData::read_le(&padded_bytes(net_data, 0x28));
        let pos = self.base.borrow().pos;
        let solid = tile_is_solid(tile_at_world_position_loaded(pos.x, pos.y, &self.links.world()));
        let mut st = self.st.borrow_mut();
        st.solid_tile = solid;
        st.paint_color = s.paint_color;
    }

    /// `-[ElevatorShaft worldChanged:]`: only `p.x == pos.x` is compared (confirmed original quirk) and no
    /// `dynamicWorldChangedAtPos`.
    fn world_changed(&self, _me: &ObjRef, positions: &mut Vec<IntPair>) {
        if self.base.borrow().is_net {
            return;
        }
        let world = self.links.world();
        let dw = self.links.dw();
        let pos = self.base.borrow().pos;
        for p in positions.iter() {
            if p.x == pos.x {
                if let Some(m) = dw.elevator_motor_for_shaft_at_pos(pos) {
                    self.st.borrow_mut().last_known_motor_pos = m.pos();
                }
                let new_solid = tile_is_solid(tile_at_world_position_loaded(pos.x, pos.y, &world));
                let changed = {
                    let mut st = self.st.borrow_mut();
                    if new_solid != st.solid_tile {
                        st.solid_tile = new_solid;
                        true
                    } else {
                        false
                    }
                };
                if changed && !self.base.borrow().is_net {
                    self.base.borrow_mut().update_needs_to_be_sent = true;
                }
            }
        }
    }
}

// =====================================================================================================
// ElevatorMotor
// =====================================================================================================

/// ElevatorMotor ivars.
pub struct MotorState {
    pub item_type: i32,
    pub available_electricity: u16,
    pub client_power_usage: u16,
    pub min_y: i32,
    pub max_y: i32,
    pub time_until_next_power_check: f32,
}

/// `ElevatorMotor: DynamicObject <PowerUser>` (type 55).
pub struct ElevatorMotor {
    base: RefCell<DynamicObjectBase>,
    /// The ElevatorMotor ivars.
    pub st: RefCell<MotorState>,
    links: Links,
}

impl ElevatorMotor {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<ElevatorMotor> {
        let links = Links::new(world, dw);
        handle::alloc_obj(|sw| ElevatorMotor {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            st: RefCell::new(MotorState {
                item_type: 0,
                available_electricity: 0,
                client_power_usage: 0,
                min_y: 0,
                max_y: 0,
                time_until_next_power_check: 0.0,
            }),
            links,
        })
    }

    /// `tileAtWorldPositionLoaded(x, y)` is a shaft tile (`contents == 103`).
    fn is_shaft_tile(&self, world: &Rc<World>, x: i32, y: i32) -> bool {
        matches!(tile_at_world_position_loaded(x, y, world), Some(t) if t.contents() == CONTENTS_ELEVATOR_SHAFT)
    }

    /// Placement constructor. `itemType` is stored (the original does store it).
    pub fn init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        item_type: i32,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        if !base_init_with_world_dynamic_world_at_position_cache(&me_obj, &wref, &dref, pos, cache) {
            return None;
        }
        let pos = me.base.borrow().pos;
        let (mut min_y, mut max_y) = (pos.y, pos.y);
        let mut y = pos.y;
        loop {
            y += 1;
            if !(y < 0x400) || !me.is_shaft_tile(world, pos.x, y) {
                break;
            }
            max_y = y;
        }
        y = pos.y;
        loop {
            y -= 1;
            if !(y > -1) || !me.is_shaft_tile(world, pos.x, y) {
                break;
            }
            min_y = y;
        }
        {
            let mut st = me.st.borrow_mut();
            st.item_type = item_type;
            st.min_y = min_y;
            st.max_y = max_y;
        }
        if let Some(o) = owner_from_client_or_save(client_id, save_dict) {
            me.base.borrow_mut().owner_id = Some(o);
        }
        Some(me_obj)
    }

    /// Save constructor.
    pub fn init_with_world_dynamic_world_save_dict_cache(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        if !base_init_with_world_dynamic_world_save_dict_cache(&me_obj, &wref, &dref, save_dict, cache) {
            return None;
        }
        {
            let g = |k: &str| save_dict.map.get(&k.to_string());
            let mut st = me.st.borrow_mut();
            st.item_type = g("itemType").map_or(0, |v| v.int_value());
            st.available_electricity = g("availableElectricity").map_or(0, |v| v.unsigned_int_value()) as u16;
            st.min_y = g("minY").map_or(0, |v| v.unsigned_int_value()) as i32;
            st.max_y = g("maxY").map_or(0, |v| v.unsigned_int_value()) as i32;
        }
        me.base.borrow_mut().owner_id = sd_string(save_dict, "ownerID");
        Some(me_obj)
    }

    /// Net constructor (`clientPowerUsage` is not read).
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        if !base_init_with_world_dynamic_world_cache_net_data(&me_obj, &wref, &dref, cache, net_data) {
            return None;
        }
        let s = ElevatorMotorCreationNetData::read_le(&padded_bytes(net_data, 0x28));
        {
            let mut st = me.st.borrow_mut();
            st.item_type = s.item_type as i32;
            st.available_electricity = s.available_electricity;
            st.min_y = s.min_y as i32;
            st.max_y = s.max_y as i32;
        }
        if net_data.len() > 0x28 {
            let owner = tail_string(&read_plist_tail(net_data, 0x28), "ownerID");
            me.base.borrow_mut().owner_id = owner;
        }
        Some(me_obj)
    }

    /// `-[ElevatorMotor itemType]`
    pub fn item_type(&self) -> i32 {
        self.st.borrow().item_type
    }
    /// `-[ElevatorMotor minY]`
    pub fn min_y(&self) -> i32 {
        self.st.borrow().min_y
    }
    /// `-[ElevatorMotor maxY]`
    pub fn max_y(&self) -> i32 {
        self.st.borrow().max_y
    }
    /// `-[ElevatorMotor setMinY:]`
    pub fn set_min_y(&self, v: i32) {
        self.st.borrow_mut().min_y = v;
    }
    /// `-[ElevatorMotor setMaxY:]`
    pub fn set_max_y(&self, v: i32) {
        self.st.borrow_mut().max_y = v;
    }
    /// `-[ElevatorMotor availableElectricity]`
    pub fn available_electricity(&self) -> u16 {
        self.st.borrow().available_electricity
    }
    /// `-[ElevatorMotor hasRequiredPower]`: `availableElectricity > 9`.
    pub fn has_required_power(&self) -> bool {
        self.st.borrow().available_electricity > 9
    }
    /// `-[ElevatorMotor usePower]`.
    pub fn use_power(&self, _me: &ObjRef) {
        let is_net = self.base.borrow().is_net;
        {
            let mut st = self.st.borrow_mut();
            if !is_net {
                if st.available_electricity != 0 {
                    st.time_until_next_power_check = 0.0;
                }
                st.available_electricity =
                    if st.available_electricity <= 10 { 0 } else { st.available_electricity - 10 };
            } else {
                st.client_power_usage = st.client_power_usage.wrapping_add(10);
            }
        }
        self.base.borrow_mut().update_needs_to_be_sent = true;
    }
}

impl PowerUser for ElevatorMotor {
    fn is_storage_device(&self) -> bool {
        false
    }
}

impl DynamicObject for ElevatorMotor {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &ELEVATOR_MOTOR_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_ELEVATOR_MOTOR
    }
    fn occupies_normal_contents(&self) -> bool {
        true
    }
    fn freeblock_creation_item_type(&self) -> i32 {
        self.item_type()
    }
    fn free_block_creation_save_dict(&self, me: &ObjRef) -> Option<PlistDict> {
        Some(self.get_save_dict(me))
    }
    fn as_power_user(&self) -> Option<&dyn PowerUser> {
        Some(self)
    }

    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = base_get_save_dict(me);
        let (it, ae, mn, mx) = {
            let st = self.st.borrow();
            (st.item_type, st.available_electricity, st.min_y, st.max_y)
        };
        d.map.insert("itemType".to_string(), PlistValue::int(it as i64));
        d.map.insert("availableElectricity".to_string(), PlistValue::int(ae as i64));
        d.map.insert("minY".to_string(), PlistValue::int(mn as u32 as i64));
        d.map.insert("maxY".to_string(), PlistValue::int(mx as u32 as i64));
        if let Some(o) = self.base.borrow().owner_id.clone() {
            d.map.insert("ownerID".to_string(), PlistValue::string(o));
        }
        d
    }

    /// Side effect: `clientPowerUsage = 0` after serialising.
    fn creation_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        let s = {
            let mut st = self.st.borrow_mut();
            let s = ElevatorMotorCreationNetData {
                dynamic_object_net_data: base_dynamic_object_net_data(me),
                item_type: st.item_type as u16,
                available_electricity: st.available_electricity,
                min_y: st.min_y as u16,
                max_y: st.max_y as u16,
                client_power_usage: st.client_power_usage,
                padding: [0; 6],
            };
            st.client_power_usage = 0;
            s
        };
        let mut out = s.to_vec();
        let owner = self.base.borrow().owner_id.clone();
        append_plist_tail(&mut out, &[("ownerID", owner.as_deref())]);
        Some(out)
    }
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        self.creation_net_data_for_client(me, client_id)
    }

    /// `-[ElevatorMotor remoteUpdate:]`.
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        base_remote_update(me, net_data);
        let pos = self.base.borrow().pos;
        self.links.dw().dynamic_world_changed_at_pos_object_type(pos, DYNAMIC_OBJECT_TYPE_ELEVATOR_MOTOR);
        let u = ElevatorMotorCreationNetData::read_le(&padded_bytes(net_data, 0x28));
        let is_net = self.base.borrow().is_net;
        let mut st = self.st.borrow_mut();
        if !is_net {
            if st.available_electricity != 0 && u.client_power_usage != 0 {
                st.time_until_next_power_check = 0.0;
            }
            if u.client_power_usage < st.available_electricity {
                st.available_electricity -= u.client_power_usage;
            } else {
                st.available_electricity = 0;
            }
        } else {
            st.available_electricity = u.available_electricity;
            st.min_y = u.min_y as i32;
            st.max_y = u.max_y as i32;
        }
    }

    /// `-[ElevatorMotor update:accurateDT:isSimulation:]`: tops up from the wire network every 5 s.
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, _accurate_dt: f32, _is_simulation: bool) {
        if self.base.borrow().is_net {
            return;
        }
        let run = {
            let mut st = self.st.borrow_mut();
            if st.available_electricity < 15 {
                st.time_until_next_power_check -= dt;
                if st.time_until_next_power_check <= 0.0f32 {
                    st.time_until_next_power_check = 5.0f32;
                    Some(st.available_electricity)
                } else {
                    None
                }
            } else {
                None
            }
        };
        if let Some(ae) = run {
            let dw = self.links.dw();
            let got = dw.find_and_subtract_all_power_up_to_for_user((15i16.wrapping_sub(ae as i16)) as u16, me);
            if got > 0 {
                {
                    let mut st = self.st.borrow_mut();
                    st.available_electricity = st.available_electricity.wrapping_add(got as i16 as u16);
                }
                let pos = self.base.borrow().pos;
                dw.dynamic_world_changed_at_pos_object_type(pos, DYNAMIC_OBJECT_TYPE_ELEVATOR_MOTOR);
                self.base.borrow_mut().update_needs_to_be_sent = true;
            }
        }
    }

    /// `-[ElevatorMotor worldChanged:]`: keeps `[minY, maxY]` in sync (exact x, no wrap).
    fn world_changed(&self, _me: &ObjRef, positions: &mut Vec<IntPair>) {
        if self.base.borrow().is_net {
            return;
        }
        let world = self.links.world();
        let pos = self.base.borrow().pos;
        for p in positions.iter() {
            if p.x != pos.x {
                continue;
            }
            if p.y > pos.y {
                let max_y = self.max_y();
                if p.y > max_y {
                    if p.y == max_y + 1 {
                        let mut y = max_y;
                        loop {
                            y += 1;
                            if !(y < 1024) || !self.is_shaft_tile(&world, pos.x, y) {
                                break;
                            }
                            self.set_max_y(y);
                            self.base.borrow_mut().update_needs_to_be_sent = true;
                        }
                    }
                } else if let Some(t) = tile_at_world_position_loaded(p.x, p.y, &world) {
                    if t.contents() != CONTENTS_ELEVATOR_SHAFT {
                        self.set_max_y(p.y - 1);
                        self.base.borrow_mut().update_needs_to_be_sent = true;
                    }
                }
            } else if p.y < pos.y {
                let min_y = self.min_y();
                if p.y < min_y {
                    if p.y == min_y - 1 {
                        let mut y = min_y;
                        loop {
                            y -= 1;
                            if !(y >= 0) || !self.is_shaft_tile(&world, pos.x, y) {
                                break;
                            }
                            self.set_min_y(y);
                            self.base.borrow_mut().update_needs_to_be_sent = true;
                        }
                    }
                } else if let Some(t) = tile_at_world_position_loaded(p.x, p.y, &world) {
                    if t.contents() != CONTENTS_ELEVATOR_SHAFT {
                        self.set_min_y(p.y + 1);
                        self.base.borrow_mut().update_needs_to_be_sent = true;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::column::testkit::*;
    use super::*;

    fn motor(w: &Rc<World>, d: &Rc<DynamicWorld>, x: i32, y: i32) -> ObjRef {
        ElevatorMotor::init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
            w,
            d,
            pos(x, y),
            None,
            1088,
            None,
            Some("own"),
        )
        .unwrap()
    }
    fn m(o: &ObjRef) -> &ElevatorMotor {
        handle::downcast::<ElevatorMotor>(o).unwrap()
    }
    fn shaft_tiles(w: &World, x: i32, y0: i32, y1: i32) {
        for y in y0..=y1 {
            tile(w, x, y).set_contents(CONTENTS_ELEVATOR_SHAFT);
        }
    }

    /// : range scan of the placement constructor.
    #[test]
    fn range_scan() {
        let (w, d) = env();
        let a = motor(&w, &d, 40, 20);
        assert_eq!((m(&a).min_y(), m(&a).max_y()), (20, 20));
        shaft_tiles(&w, 41, 21, 23);
        shaft_tiles(&w, 41, 17, 19);
        let b = motor(&w, &d, 41, 20);
        assert_eq!((m(&b).min_y(), m(&b).max_y()), (17, 23));
        // reaching y = 0
        shaft_tiles(&w, 42, 0, 5);
        let c = motor(&w, &d, 42, 3);
        assert_eq!((m(&c).min_y(), m(&c).max_y()), (0, 5));
        // reaching y = 1023 (block y=31 is not loaded in the test world: the lookup stops there)
        assert_eq!(c.owner_id().as_deref(), Some("own"));
    }

    /// `hasRequiredPower` / `usePower` boundaries 9, 10, 11, 15.
    #[test]
    fn power_boundaries() {
        let (w, d) = env();
        let a = motor(&w, &d, 40, 20);
        for (start, req, after) in [(9u16, false, 0u16), (10, true, 0), (11, true, 1), (15, true, 5), (0, false, 0)] {
            m(&a).st.borrow_mut().available_electricity = start;
            m(&a).st.borrow_mut().time_until_next_power_check = 3.0;
            assert_eq!(m(&a).has_required_power(), req, "{start}");
            a.set_update_needs_to_be_sent(false);
            m(&a).use_power(&a);
            assert_eq!(m(&a).available_electricity(), after, "{start}");
            assert!(a.update_needs_to_be_sent());
            let t = m(&a).st.borrow().time_until_next_power_check;
            assert_eq!(t, if start != 0 { 0.0 } else { 3.0 });
        }
        // isNet replicas accumulate client usage and report it, resetting on serialisation
        let net = a.creation_net_data_for_client(&a, None).unwrap();
        let n = ElevatorMotor::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &net).unwrap();
        m(&n).use_power(&n);
        m(&n).use_power(&n);
        assert_eq!(m(&n).st.borrow().client_power_usage, 20);
        let report = n.creation_net_data_for_client(&n, None).unwrap();
        assert_eq!(m(&n).st.borrow().client_power_usage, 0);
        assert_eq!(&report[0x20..0x22], &20u16.to_le_bytes());
        // the server applies the report: 15 - 10 -> 5; more than available -> 0
        m(&a).st.borrow_mut().available_electricity = 15;
        a.remote_update(&a, &report[..0x28].iter().copied().chain([0u8; 0].into_iter()).collect::<Vec<u8>>());
        assert_eq!(m(&a).available_electricity(), 0);
        m(&a).st.borrow_mut().available_electricity = 25;
        a.remote_update(&a, &report);
        assert_eq!(m(&a).available_electricity(), 5);
        assert_eq!(m(&a).st.borrow().time_until_next_power_check, 0.0);
    }

    #[test]
    fn net_and_save_round_trip() {
        let (w, d) = env();
        shaft_tiles(&w, 41, 21, 23);
        let a = motor(&w, &d, 41, 20);
        m(&a).st.borrow_mut().available_electricity = 12;
        let sd = a.get_save_dict(&a);
        assert_eq!(sd.map.len(), 9);
        let b = ElevatorMotor::init_with_world_dynamic_world_save_dict_cache(&w, &d, &sd, None).unwrap();
        assert_eq!(
            (m(&b).item_type(), m(&b).available_electricity(), m(&b).min_y(), m(&b).max_y()),
            (1088, 12, 20, 23)
        );
        let net = a.creation_net_data_for_client(&a, None).unwrap();
        assert!(net[0x22..0x28].iter().all(|x| *x == 0));
        let c = ElevatorMotor::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &net).unwrap();
        assert!(c.is_net());
        assert_eq!((m(&c).available_electricity(), m(&c).min_y(), m(&c).max_y()), (12, 20, 23));
        assert_eq!(c.owner_id().as_deref(), Some("own"));
        assert!(ELEVATOR_MOTOR_CLASS.conforms_to("PowerUser"));
        assert!(!c.as_power_user().unwrap().is_storage_device());
    }

    /// Shaft growth / cut keeps `[minY, maxY]` in sync and flags updates.
    #[test]
    fn world_changed_tracks_shaft() {
        let (w, d) = env();
        shaft_tiles(&w, 41, 21, 22);
        let a = motor(&w, &d, 41, 20);
        assert_eq!(m(&a).max_y(), 22);
        a.set_update_needs_to_be_sent(false);
        // grow by one
        tile(&w, 41, 23).set_contents(CONTENTS_ELEVATOR_SHAFT);
        a.world_changed(&a, &mut vec![pos(41, 23)]);
        assert_eq!(m(&a).max_y(), 23);
        assert!(a.update_needs_to_be_sent());
        // cut in the middle
        a.set_update_needs_to_be_sent(false);
        tile(&w, 41, 22).set_contents(0);
        a.world_changed(&a, &mut vec![pos(41, 22)]);
        assert_eq!(m(&a).max_y(), 21);
        // other columns and the motor row are ignored
        a.set_update_needs_to_be_sent(false);
        a.world_changed(&a, &mut vec![pos(42, 22), pos(41, 20)]);
        assert!(!a.update_needs_to_be_sent());
        // downward
        shaft_tiles(&w, 41, 18, 19);
        a.world_changed(&a, &mut vec![pos(41, 19)]);
        assert_eq!(m(&a).min_y(), 18);
        tile(&w, 41, 19).set_contents(0);
        a.world_changed(&a, &mut vec![pos(41, 19)]);
        assert_eq!(m(&a).min_y(), 20);
    }

    #[test]
    fn update_tops_up_every_five_seconds() {
        let (w, d) = env();
        let a = motor(&w, &d, 40, 20);
        // no power source: the countdown restarts at 5 and nothing changes
        a.update_accurate_dt_is_simulation(&a, 0.1, 0.1, false);
        assert_eq!(m(&a).st.borrow().time_until_next_power_check, 5.0);
        a.update_accurate_dt_is_simulation(&a, 1.0, 1.0, false);
        assert_eq!(m(&a).st.borrow().time_until_next_power_check, 4.0);
        assert_eq!(m(&a).available_electricity(), 0);
        // full motors do not count down
        m(&a).st.borrow_mut().available_electricity = 15;
        a.update_accurate_dt_is_simulation(&a, 1.0, 1.0, false);
        assert_eq!(m(&a).st.borrow().time_until_next_power_check, 4.0);
    }

    #[test]
    fn shaft_lifecycle() {
        let (w, d) = env();
        let motor_o = motor(&w, &d, 40, 20);
        let _ = motor_o;
        solid(&w, 40, 21, true);
        let s = ElevatorShaft::init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
            &w,
            &d,
            pos(40, 21),
            None,
            1087,
            None,
            Some("own"),
        )
        .unwrap();
        let sh = handle::downcast::<ElevatorShaft>(&s).unwrap();
        assert!(sh.solid_tile());
        assert_eq!(sh.last_known_motor_pos(), pos(40, 21));
        assert!(sh.is_paintable());
        sh.paint(&s, 4);
        assert!(s.update_needs_to_be_sent());
        let sd = s.get_save_dict(&s);
        assert_eq!(sd.map.len(), 9);
        let net = s.creation_net_data_for_client(&s, None).unwrap();
        assert_eq!(&net[0x18..0x1c], &40u32.to_le_bytes());
        assert_eq!(&net[0x1c..0x20], &21u32.to_le_bytes());
        assert_eq!(&net[0x20..0x22], &1087u16.to_le_bytes());
        assert_eq!(&net[0x22..0x24], &4u16.to_le_bytes());
        assert_eq!(net[0x24], 1);
        assert!(net[0x25..0x28].iter().all(|x| *x == 0));
        let s2 = ElevatorShaft::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &net).unwrap();
        assert!(s2.is_net());
        assert_eq!(s2.owner_id().as_deref(), Some("own"));
        let s3 = ElevatorShaft::init_with_world_dynamic_world_save_dict_cache(&w, &d, &sd, None).unwrap();
        assert_eq!(handle::downcast::<ElevatorShaft>(&s3).unwrap().item_type(), 1087);
        // solidTile flip flags a net update; only x is compared; no dynamicWorldChangedAtPos needed
        s.set_update_needs_to_be_sent(false);
        solid(&w, 40, 21, false);
        s.world_changed(&s, &mut vec![pos(41, 21)]);
        assert!(!s.update_needs_to_be_sent());
        s.world_changed(&s, &mut vec![pos(40, 5)]);
        assert!(s.update_needs_to_be_sent());
        assert!(!sh.solid_tile());
    }
}
