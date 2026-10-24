//! `TrainCar`: shared state and free functions of every rail vehicle (HandCar 41, SteamTrain 42, FreightCar 43,
//! PassengerCar 44).
//!
//! TrainCar is not instantiated: the concrete classes hold a [`TrainCarState`]
//! and implement [`TrainCarLike`]; a subclass override that does `[super X]` calls the `train_car_x` free function
//! here. Client-only code (shaders, textures, `draw:`, camera/body matrices) is not ported.
//!
//! This file also holds the physics helpers shared by the four classes (wheel classification, rail surface
//! height, rotated-body tap test, GLK point evaluation) and the shared rolling-car `update` used verbatim by
//! FreightCar and PassengerCar.

use std::cell::RefCell;
use std::rc::Rc;

use crate::blockhead::blockhead::{as_blockhead, host, Blockhead};
use crate::dynworld::state::DynamicWorld;
use crate::fmath::cconv::f32_to_i32;
use crate::fmath::libm::{atan2f, cosf, fmodf, sinf};
use crate::foundation::gzip::{gzip_deflate, gzip_inflate};
use crate::objects::base::{
    base_dynamic_object_net_data, base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_cache_net_data, base_init_with_world_dynamic_world_save_dict_cache,
    base_remote_update, base_remove_from_macro_block, base_update_position, new_mutable_dict, padded_bytes,
    DynamicObject, DynamicWorldRef, WorldRef,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{downcast, ptr_eq, CacheRef, ObjRef};
/// `TrainCar: DynamicObject <RidableDynamicObject, TapChoiceDynamicObject>` (A4); parent of the four car classes.
pub static TRAIN_CAR_CLASS: ClassInfo = ClassInfo {
    name: "TrainCar",
    superclass: Some(&DYNAMIC_OBJECT_CLASS),
    protocols: &["RidableDynamicObject", "TapChoiceDynamicObject"],
};

use crate::objects::protocols::{RidableDynamicObject, TapChoiceDynamicObject};
use crate::plist::helpers::{data_from_property_list, property_list_from_data};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::item_type::ItemType;
use crate::types::structs::{IntPair, Vector2, WireStruct};
use crate::world::helpers::{tile_is_solid, tile_is_water};
use crate::world::state::{TileRef, World};
use crate::world::tiles::tile_at_world_position_loaded;

pub use super::rail::{
    rail_current_configuration, CONTENTS_COLUMN, CONTENTS_RAIL, DOT_RAIL, RAIL_DIAGONAL_HALF_FLAT,
    RAIL_DIAGONAL_HALF_UP_LEFT_BOT, RAIL_DIAGONAL_HALF_UP_LEFT_TOP, RAIL_DIAGONAL_HALF_UP_RIGHT_BOT,
    RAIL_DIAGONAL_HALF_UP_RIGHT_TOP, RAIL_DIAGONAL_UP_LEFT, RAIL_DIAGONAL_UP_RIGHT, RAIL_FLAT, RAIL_UNDEFINED,
};
pub use crate::types::netdata::TrainCarCreationNetData;

// ------------------------------------------------------------------------------------------------ constants

/// `DYNAMIC_OBJECT_TYPE_TRAIN_CAR_HANDCAR`.
pub const DOT_TRAIN_CAR_HANDCAR: i32 = 41;
/// `DYNAMIC_OBJECT_TYPE_TRAIN_CAR_STEAM_ENGINE`.
pub const DOT_TRAIN_CAR_STEAM_ENGINE: i32 = 42;
/// `DYNAMIC_OBJECT_TYPE_TRAIN_CAR_FREIGHT`.
pub const DOT_TRAIN_CAR_FREIGHT: i32 = 43;
/// `DYNAMIC_OBJECT_TYPE_TRAIN_CAR_PASSENGER`.
pub const DOT_TRAIN_CAR_PASSENGER: i32 = 44;
/// Iteration order used by the DynamicWorld lookups.
pub const TRAIN_CAR_DYNAMIC_OBJECT_TYPES: [i32; 4] = [41, 42, 43, 44];
/// `ITEM_RAIL`.
pub const ITEM_RAIL: i32 = 199;
/// `ITEM_TRAIN_STATION`.
pub const ITEM_TRAIN_STATION: i32 = 200;
/// `ITEM_HANDCAR`.
pub const ITEM_HANDCAR: i32 = 204;
/// `ITEM_STEAM_TRAIN`.
pub const ITEM_STEAM_TRAIN: i32 = 205;
/// `ITEM_FREIGHT_CAR`.
pub const ITEM_FREIGHT_CAR: i32 = 206;
/// `ITEM_PASSENGER_CAR`.
pub const ITEM_PASSENGER_CAR: i32 = 208;
/// `ITEM_CHEST` (0x413).
pub const ITEM_CHEST: i32 = 0x413;
/// `ITEM_WOOD`.
pub const ITEM_WOOD: i32 = 1049;
/// `ITEM_CHARCOAL`.
pub const ITEM_CHARCOAL: i32 = 14;
/// `ITEM_COAL`.
pub const ITEM_COAL: i32 = 51;
/// `CONTENTS_INTERACTION_OBJECT` in `Tile.contents` ('0').
pub const CONTENTS_INTERACTION_OBJECT: u8 = 0x30;

// ------------------------------------------------------------------------------------------------ state

/// TrainCar ivars 0x78..0x118 (client-only ivars omitted).
pub struct TrainCarState {
    /// 0x78 `riders[2]` (retained)
    pub riders: [Option<ObjRef>; 2],
    /// 0x88 `savedBlockheadIndex[2]`: zeroed on alloc (NOT -1 until the saveDict init writes -1)
    pub saved_blockhead_index: [i32; 2],
    /// 0x94
    pub left_wheel_pos: Vector2,
    /// 0x9c
    pub right_wheel_pos: Vector2,
    /// 0xa4
    pub left_wheel_vel: Vector2,
    /// 0xac
    pub right_wheel_vel: Vector2,
    /// 0xb4 (only `.x` is networked)
    pub visual_velocity: Vector2,
    /// 0xbc
    pub remote_update_timer: f32,
    /// 0xc0
    pub remote_left_car_id: u64,
    /// 0xc8
    pub remote_right_car_id: u64,
    /// 0xd0
    pub remote_engine_car_id: u64,
    /// 0xd8 (retained)
    pub right_car: Option<ObjRef>,
    /// 0xe0 (retained)
    pub left_car: Option<ObjRef>,
    /// 0xe8 (retained)
    pub engine_car: Option<ObjRef>,
    /// 0xf0
    pub engine_is_right: bool,
    /// 0xf4
    pub left_wheel_tile_pos: IntPair,
    /// 0xfc
    pub right_wheel_tile_pos: IntPair,
    /// 0x108
    pub left_wheel_rail: Option<ObjRef>,
    /// 0x110
    pub right_wheel_rail: Option<ObjRef>,
}

impl TrainCarState {
    /// `[X alloc]` + `.cxx_construct`: the five `Vector2` ivars are (1,1), everything else zero.
    pub fn alloc_zeroed() -> Self {
        TrainCarState {
            riders: [None, None],
            saved_blockhead_index: [0, 0],
            left_wheel_pos: Vector2::cxx_new(),
            right_wheel_pos: Vector2::cxx_new(),
            left_wheel_vel: Vector2::cxx_new(),
            right_wheel_vel: Vector2::cxx_new(),
            visual_velocity: Vector2::cxx_new(),
            remote_update_timer: 0.0,
            remote_left_car_id: 0,
            remote_right_car_id: 0,
            remote_engine_car_id: 0,
            right_car: None,
            left_car: None,
            engine_car: None,
            engine_is_right: false,
            left_wheel_tile_pos: IntPair { x: 0, y: 0 },
            right_wheel_tile_pos: IntPair { x: 0, y: 0 },
            left_wheel_rail: None,
            right_wheel_rail: None,
        }
    }
}

// ------------------------------------------------------------------------------------------------ the trait

/// Virtual selectors of TrainCar that other code or sibling classes call.
pub trait TrainCarLike: DynamicObject {
    /// The TrainCar ivars.
    fn tc(&self) -> &RefCell<TrainCarState>;
    /// `[self world]` as the concrete `World` (the object keeps a weak link).
    fn tc_world(&self) -> Rc<World>;
    /// `[self dynamicWorld]` as the concrete `DynamicWorld`.
    fn tc_dw(&self) -> Rc<DynamicWorld>;

    /// `loadDerivedStuff` (virtual; subclasses call [`train_car_load_derived_stuff`] as their super).
    fn load_derived_stuff(&self, me: &ObjRef) {
        train_car_load_derived_stuff(me)
    }
    /// `maxNumberOfRiders`: Hand 1, Steam 1, Passenger 2, Freight 0.
    fn max_number_of_riders(&self) -> i32 {
        0
    }
    /// `isEngine`: Steam true.
    fn is_engine(&self) -> bool {
        false
    }
    /// `requiresFuel`: Steam true.
    fn requires_fuel(&self) -> bool {
        false
    }
    /// `connectsToOtherCars`: Hand false.
    fn connects_to_other_cars(&self) -> bool {
        true
    }
    /// `itemType`: 204 / 205 / 206 / 208.
    fn item_type(&self) -> i32 {
        0
    }
    /// `actionTitle`: `@"RIDE"` in the base.
    fn action_title(&self) -> String {
        "RIDE".to_string()
    }
    /// `riderPosForBlockhead:` (`Vector()` = (1,1,1,1) in the base). Server-visible (R1).
    fn rider_pos_for_blockhead(&self, _b: &ObjRef) -> [f32; 4] {
        [1.0; 4]
    }
    /// `setTargetVelocity:`: no-op in the base.
    fn set_target_velocity(&self, _v: Vector2) {}
    /// `tapIsWithinBodyRadius:`; the base is [`train_car_tap_is_within_body_radius`] (R2).
    fn tap_is_within_body_radius(&self, tap: Vector2) -> bool {
        train_car_tap_is_within_body_radius(&self.me(), tap)
    }
    /// `railOrStationNameChanged`: no-op in the base (Steam overrides).
    fn rail_or_station_name_changed(&self) {}
    /// `addRider:` (Passenger adds an achievement call that is a no-op on the server).
    fn add_rider(&self, me: &ObjRef, rider: &ObjRef) {
        train_car_add_rider(me, rider)
    }
    /// `removeRider:` (Hand and Steam extend it).
    fn remove_rider(&self, me: &ObjRef, rider: &ObjRef) {
        train_car_remove_rider(me, rider)
    }
    /// `blockheadCanRide:usingItem:` (Freight: NO).
    fn blockhead_can_ride_using_item(&self, me: &ObjRef, _b: &ObjRef, _item: i32) -> bool {
        train_car_blockhead_can_ride_using_item(me)
    }
    /// `cleanup`.
    fn cleanup(&self, me: &ObjRef) {
        train_car_cleanup(me)
    }
    /// `leftWheelPos`
    fn left_wheel_pos(&self) -> Vector2 {
        self.tc().borrow().left_wheel_pos
    }
    /// `rightWheelPos`
    fn right_wheel_pos(&self) -> Vector2 {
        self.tc().borrow().right_wheel_pos
    }
    /// `leftCar`
    fn left_car(&self) -> Option<ObjRef> {
        self.tc().borrow().left_car.clone()
    }
    /// `rightCar`
    fn right_car(&self) -> Option<ObjRef> {
        self.tc().borrow().right_car.clone()
    }
    /// `engineCar`: `self` when `isEngine`, else the ivar.
    fn engine_car(&self, me: &ObjRef) -> Option<ObjRef> {
        if self.is_engine() {
            Some(me.clone())
        } else {
            self.tc().borrow().engine_car.clone()
        }
    }
    /// `setLeftCar:`
    fn set_left_car(&self, me: &ObjRef, c: Option<&ObjRef>) {
        train_car_set_left_car(me, c)
    }
    /// `setRightCar:`
    fn set_right_car(&self, me: &ObjRef, c: Option<&ObjRef>) {
        train_car_set_right_car(me, c)
    }
    /// `setEngineCar:`
    fn set_engine_car(&self, me: &ObjRef, c: Option<&ObjRef>) {
        train_car_set_engine_car(me, c)
    }
}

/// `match o.object_type() { 41..=44 => downcast }`; `None` for any other object.
pub fn train_car_of(o: &ObjRef) -> Option<&dyn TrainCarLike> {
    use super::freight_car::FreightCar;
    use super::hand_car::HandCar;
    use super::passenger_car::PassengerCar;
    use super::steam_train::SteamTrain;
    match o.object_type() {
        DOT_TRAIN_CAR_HANDCAR => downcast::<HandCar>(o).map(|c| c as &dyn TrainCarLike),
        DOT_TRAIN_CAR_STEAM_ENGINE => downcast::<SteamTrain>(o).map(|c| c as &dyn TrainCarLike),
        DOT_TRAIN_CAR_FREIGHT => downcast::<FreightCar>(o).map(|c| c as &dyn TrainCarLike),
        DOT_TRAIN_CAR_PASSENGER => downcast::<PassengerCar>(o).map(|c| c as &dyn TrainCarLike),
        _ => None,
    }
}

fn tcl(me: &ObjRef) -> &dyn TrainCarLike {
    train_car_of(me).expect("not a TrainCar")
}

/// Pointer equality of two possibly-nil handles (ObjC `==`).
pub fn same_opt(a: &Option<ObjRef>, b: &Option<ObjRef>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => ptr_eq(a, b),
        _ => false,
    }
}

// ------------------------------------------------------------------------------------------------ nil-safe Blockhead calls

/// `[r needsRemoved]` (nil -> NO).
pub fn rider_needs_removed(r: &Option<ObjRef>) -> bool {
    r.as_ref().map_or(false, |r| r.needs_removed())
}
/// `[r isNet]` (nil -> NO).
pub fn rider_is_net(r: &Option<ObjRef>) -> bool {
    r.as_ref().map_or(false, |r| r.is_net())
}
/// `[r paused]` (nil -> NO).
pub fn rider_paused(r: &Option<ObjRef>) -> bool {
    r.as_ref().and_then(|r| as_blockhead(r)).map_or(false, |b| b.paused())
}
/// `[r isClientBlockheadBeingControlledByServer]` (nil -> NO).
pub fn rider_controlled_by_server(r: &Option<ObjRef>) -> bool {
    r.as_ref().and_then(|r| r.as_blockhead()).map_or(false, |b| b.is_client_blockhead_being_controlled_by_server())
}
/// `[r stopRiding]` (nil -> no-op). Goes through the blockhead host seam (`BlockheadHost::stop_riding`).
pub fn rider_stop_riding(r: &ObjRef) {
    if let Some(b) = as_blockhead(r) {
        host().stop_riding(b);
    }
}
/// `[r setRidingObject:o]` (nil -> no-op): `rideObject = o` (the UI fuel call is a no-op on the server).
pub fn rider_set_riding_object(r: &ObjRef, o: &ObjRef) {
    if let Some(b) = as_blockhead(r) {
        b.bh.borrow_mut().ride_object = Some(o.clone());
    }
}
/// `[r sufferDamage:d isSimulation:sim recoil:recoil]`.
pub fn rider_suffer_damage(r: &ObjRef, d: f32, sim: bool, recoil: bool) {
    if let Some(b) = as_blockhead(r) {
        let b: &Blockhead = b;
        b.suffer_damage_is_simulation_recoil(d, sim, recoil);
    }
}

// ------------------------------------------------------------------------------------------------ numeric helpers

/// Horizontal wrap of a distance on the cylindrical world (`m` = `worldWidthMacro`):
/// `d < (float)((m<<5)/2) ? ((float)((m*-32)/2) <= d ? d: d + full): d - full`.
pub fn wrap_dx(dx: f32, m: i32) -> f32 {
    let full = m.wrapping_shl(5) as f32;
    if dx < (m.wrapping_shl(5) / 2) as f32 {
        if ((m.wrapping_mul(-32)) / 2) as f32 <= dx {
            dx
        } else {
            dx + full
        }
    } else {
        dx - full
    }
}

/// The GLK `Translate * RotateZ` times a local point, evaluated in the original order (translation added last):
/// `x = ((c*ox + (-s)*oy) + 0*0) + tx`, `y = ((s*ox + c*oy) + 0*0) + ty`.
pub fn glk_xy(c: f32, s: f32, ox: f32, oy: f32, tx: f32, ty: f32) -> (f32, f32) {
    let z = std::hint::black_box(0.0f32);
    let x = ((c * ox + (-s) * oy) + z * z) + tx;
    let y = ((s * ox + c * oy) + z * z) + ty;
    (x, y)
}

/// `theta = atan2f(rw.y - lw.y, rw.x - lw.x)`.
pub fn wheel_axis_angle(lw: Vector2, rw: Vector2) -> f32 {
    atan2f(rw.y - lw.y, rw.x - lw.x)
}

/// `riderPosForBlockhead:` common evaluation (R1): `Vector(x, y, -0.75f)`.
pub fn rider_pos_common(float_pos: Vector2, lw: Vector2, rw: Vector2, ox: f32) -> [f32; 4] {
    let theta = wheel_axis_angle(lw, rw);
    let (c, s) = (cosf(theta), sinf(theta));
    // rp = floatPos + (0.0, 0.5); tx = rp.x; ty = rp.y + 0.5f
    let tx = float_pos.x + 0.0f32;
    let ty = (float_pos.y + 0.5f32) + 0.5f32;
    let (x, y) = glk_xy(c, s, ox, 0.0, tx, ty);
    [x, y, -0.75f32, 1.0f32]
}

/// Rotated body hit test shared by FreightCar / PassengerCar (`upper` = 2.0) and SteamTrain (`upper` = 2.5).
pub fn rotated_body_tap_test(
    float_pos: Vector2,
    lw: Vector2,
    rw: Vector2,
    tap: Vector2,
    macro_w: i32,
    upper: f32,
) -> bool {
    let theta = wheel_axis_angle(lw, rw);
    let neg = f32::from_bits(theta.to_bits() ^ 0x8000_0000);
    let (c, s) = (cosf(neg), sinf(neg));
    // rp = floatPos + (0.0, 0.5)
    let rp = Vector2 { x: float_pos.x + 0.0f32, y: float_pos.y + 0.5f32 };
    let off = Vector2 { x: tap.x - rp.x, y: tap.y - rp.y };
    let z = std::hint::black_box(0.0f32);
    let mx = ((c * off.x + (-s) * off.y) + z * z) + rp.x;
    let my = ((s * off.x + c * off.y) + z * z) + rp.y;
    let d = wrap_dx(float_pos.x - mx, macro_w);
    (d as f64) < 1.5 && -1.5 < (d as f64) && float_pos.y < my && my < float_pos.y + upper
}

// ------------------------------------------------------------------------------------------------ wheel helpers

/// Result of looking at the tile a wheel is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelTile {
    /// The tile itself holds a rail.
    Rail,
    /// The tile above holds a rail: the wheel tile position moves up by one.
    RailAbove,
    /// No rail and the tile is solid: derail / crash.
    Solid,
    /// Empty space: free fall (`water` = `tileIsWater(tile)`).
    Air { water: bool },
}

/// Classifies the tile of a wheel (`tile` non-NULL, `pos` its position). The tile above is fetched with the
/// loading lookup.
pub fn classify_wheel_tile(world: &World, tile: TileRef, pos: IntPair) -> WheelTile {
    if tile.foreground_contents() == CONTENTS_RAIL {
        return WheelTile::Rail;
    }
    let above = tile_at_world_position_loaded(pos.x, pos.y.wrapping_add(1), world);
    if above.map_or(true, |a| a.foreground_contents() != CONTENTS_RAIL) {
        if tile_is_solid(Some(tile)) {
            WheelTile::Solid
        } else {
            WheelTile::Air { water: tile_is_water(Some(tile)) }
        }
    } else {
        WheelTile::RailAbove
    }
}

/// Rail surface of one wheel: height, optional x-acceleration contribution and the slope velocity `cvy`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RailSurface {
    pub height: f32,
    /// `Some(x)` when the configuration writes the wheel's acceleration x
    pub acc_x: Option<f32>,
    pub cvy: f32,
}

/// `cfg` = `[rail currentConfiguration]`, `f` = x fraction in the tile, `ty` = `(float)tilePos.y`,
/// `vx` = the wheel's own velocity x before the snap.
pub fn rail_surface(cfg: i32, f: f32, ty: f32, vx: f32) -> RailSurface {
    match cfg {
        8 => RailSurface { height: 0.5f32 + ty, acc_x: None, cvy: 0.0 },
        2 => RailSurface { height: (1.0f32 - f) + ty, acc_x: Some(1.0), cvy: (-vx) * 0.5f32 },
        3 => RailSurface { height: (0.5f32 - 0.5f32 * f) + ty, acc_x: Some(0.5), cvy: (-vx) * 0.25f32 },
        4 => RailSurface { height: (1.0f32 - 0.5f32 * f) + ty, acc_x: Some(0.5), cvy: (-vx) * 0.25f32 },
        5 => RailSurface { height: f + ty, acc_x: Some(-1.0), cvy: 0.5f32 * vx },
        6 => RailSurface { height: 0.5f32 * f + ty, acc_x: Some(-0.5), cvy: 0.25f32 * vx },
        7 => RailSurface { height: (0.5f32 + 0.5f32 * f) + ty, acc_x: Some(-0.5), cvy: 0.25f32 * vx },
        _ => RailSurface { height: ty, acc_x: None, cvy: 0.0 },
    }
}

/// Vertical resolution of one wheel (left or right). `rail_cfg == None` means no `Rail` object.
/// Without a rail the test only runs when the wheel is not already airborne; with a rail it always runs.
/// The comparison is `height >= pos.y` -> snap, else (including NaN) airborne with `acc = (0, -2)`.
// Note: // document the NaN-takes-the-air-branch compare (`ucomiss; jb`) for Freight, Passenger
// and Steam; (HandCar) writes `if railHeight < w.y` without saying how NaN compares. The same `>=` form is
// used for HandCar (
pub fn resolve_wheel_vertical(
    rail_cfg: Option<i32>,
    tile_pos: IntPair,
    x_fraction: f32,
    avg_vel: Vector2,
    pos: &mut Vector2,
    vel: &mut Vector2,
    acc: &mut Vector2,
    air: &mut bool,
) {
    let ty = tile_pos.y as f32;
    match rail_cfg {
        None => {
            if !*air {
                if ty >= pos.y {
                    pos.y = ty;
                    vel.y = 0.0;
                    vel.x = avg_vel.x;
                } else {
                    *air = true;
                    *acc = Vector2 { x: 0.0, y: -2.0 };
                }
            }
        }
        Some(cfg) => {
            let rs = rail_surface(cfg, x_fraction, ty, vel.x);
            if let Some(ax) = rs.acc_x {
                acc.x = ax;
            }
            if rs.height >= pos.y {
                pos.y = rs.height;
                vel.y = 2.0f32 * rs.cvy;
                vel.x = avg_vel.x;
            } else {
                *air = true;
                *acc = Vector2 { x: 0.0, y: -2.0 };
            }
        }
    }
}

/// Rail (re)acquisition of both wheels: tile-changed flags, remembered tile positions, release of removed rails
/// and `getRailAtPos:` when the tile changed or no rail is cached. Returns `(leftChanged, rightChanged)`.
pub fn refresh_wheel_rails(tcl: &dyn TrainCarLike, new_l: IntPair, new_r: IntPair) -> (bool, bool) {
    let dw = tcl.tc_dw();
    let (lchg, rchg) = {
        let mut t = tcl.tc().borrow_mut();
        let lchg = new_l != t.left_wheel_tile_pos;
        let rchg = new_r != t.right_wheel_tile_pos;
        t.left_wheel_tile_pos = new_l;
        t.right_wheel_tile_pos = new_r;
        if t.left_wheel_rail.as_ref().map_or(false, |r| r.needs_removed()) {
            t.left_wheel_rail = None;
        }
        if t.right_wheel_rail.as_ref().map_or(false, |r| r.needs_removed()) {
            t.right_wheel_rail = None;
        }
        (lchg, rchg)
    };
    let need_l = lchg || tcl.tc().borrow().left_wheel_rail.is_none();
    if need_l {
        let r = dw.get_rail_at_pos(new_l);
        tcl.tc().borrow_mut().left_wheel_rail = r;
    }
    let need_r = rchg || tcl.tc().borrow().right_wheel_rail.is_none();
    if need_r {
        let r = dw.get_rail_at_pos(new_r);
        tcl.tc().borrow_mut().right_wheel_rail = r;
    }
    (lchg, rchg)
}

/// The "CRASH / DERAIL" tail shared by every car: `setNeedsRemoved:YES; updateNeedsToBeSent = YES;`
/// `createFreeBlockAtPosition:pos ofType:[self itemType] dataA:0 dataB:0 subItems:nil
/// dynamicObjectSaveDict:[self freeBlockCreationSaveDict] hovers:NO playSound:YES priorityBlockhead:priority`.
/// Argument order of evaluation: `itemType`, then the save dict.
pub fn car_drop_as_free_block(me: &ObjRef, priority: Option<&ObjRef>) {
    let t = tcl(me);
    me.set_needs_removed(me, true);
    me.base().borrow_mut().update_needs_to_be_sent = true;
    let item = t.item_type();
    let sd = me.free_block_creation_save_dict(me);
    let pos = me.pos();
    t.tc_dw().create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
        pos,
        ItemType(item as u32),
        0,
        0,
        None,
        sd.map(PlistValue::Dict),
        false,
        1,
        priority,
    );
}

/// `[dynamicWorld dynamicWorldChangedAtPos:pos objectType:[self objectType]]`.
pub fn car_changed_notify(me: &ObjRef) {
    let t = tcl(me);
    let pos = me.pos();
    let ot = me.object_type();
    t.tc_dw().dynamic_world_changed_at_pos_object_type(pos, ot);
}

// ------------------------------------------------------------------------------------------------ TrainCar methods

/// `-[TrainCar loadDerivedStuff]` (server-relevant part: wheel initialisation and the rail-configuration
/// table; the shader/texture setup is client-only). This MUTATES `floatPos` (adds the rail `dy`), also for cars
/// loaded from a save (R9).
pub fn train_car_load_derived_stuff(me: &ObjRef) {
    let t = tcl(me);
    let fp0 = me.float_pos();
    {
        let mut s = t.tc().borrow_mut();
        s.left_wheel_pos = Vector2 { x: fp0.x + (-0.5f32), y: fp0.y + 0.0f32 };
        s.right_wheel_pos = Vector2 { x: fp0.x + 0.5f32, y: fp0.y + 0.0f32 };
        s.right_wheel_vel = Vector2::ZERO;
        s.left_wheel_vel = Vector2::ZERO;
        s.visual_velocity = Vector2::ZERO;
    }
    let pos = me.pos();
    let world = t.tc_world();
    let tile = tile_at_world_position_loaded(pos.x, pos.y, &world);
    if let Some(tile) = tile {
        if tile.foreground_contents() == CONTENTS_RAIL {
            if let Some(rail) = t.tc_dw().get_rail_at_pos(pos) {
                let cfg = rail_current_configuration(&rail);
                // (dy, L, R); cfg 1 FLAT and 0 leave the step-2 values
                let row: Option<(f32, (f32, f32), (f32, f32))> = match cfg {
                    8 => Some((0.5, (-0.5, 0.0), (0.5, 0.0))),
                    2 => Some((0.5, (-0.5, 0.5), (0.5, -0.5))),
                    3 => Some((0.25, (-0.5, 0.25), (0.5, -0.25))),
                    4 => Some((0.75, (-0.5, 0.25), (0.5, -0.25))),
                    5 => Some((0.5, (-0.5, -0.5), (0.5, 0.5))),
                    6 => Some((0.25, (-0.5, -0.25), (0.5, 0.25))),
                    7 => Some((0.75, (-0.5, -0.25), (0.5, 0.25))),
                    _ => None,
                };
                if let Some((dy, l, r)) = row {
                    let fp = {
                        let mut b = me.base().borrow_mut();
                        b.float_pos = Vector2 { x: b.float_pos.x + 0.0f32, y: b.float_pos.y + dy };
                        b.float_pos
                    };
                    let mut s = t.tc().borrow_mut();
                    s.left_wheel_pos = Vector2 { x: fp.x + l.0, y: fp.y + l.1 };
                    s.right_wheel_pos = Vector2 { x: fp.x + r.0, y: fp.y + r.1 };
                }
            }
        }
    }
}

/// `-[TrainCar initWithWorld:dynamicWorld:atPosition:cache:saveDict:placedByClient:]`.
/// `false` = `return nil`.
pub fn train_car_init_with_world_dynamic_world_at_position_cache_save_dict_placed_by_client(
    me: &ObjRef,
    world: &Rc<World>,
    dw: &Rc<DynamicWorld>,
    pos: IntPair,
    cache: CacheRef,
    save_dict: Option<&PlistDict>,
    client_id: Option<&str>,
) -> bool {
    let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
    // the base initialiser always succeeds when the position is given
    let inited = base_init_with_world_dynamic_world_at_position_cache(me, &wref, &dref, pos, cache);
    debug_assert!(inited);
    {
        let mut b = me.base().borrow_mut();
        b.float_pos.y = b.pos.y as f32;
        b.float_pos.x = 0.5f32 + b.pos.x as f32;
    }
    match client_id {
        None => {
            if let Some(o) = save_dict.and_then(|d| d.map.get(&"ownerID".to_string())) {
                me.set_owner_id(o.as_str().map(|s| s.to_string()));
            }
        }
        Some(c) => me.set_owner_id(Some(c.to_string())),
    }
    tcl(me).load_derived_stuff(me);
    true
}

/// `-[TrainCar initWithWorld:dynamicWorld:saveDict:cache:]`.
pub fn train_car_init_with_world_dynamic_world_save_dict_cache(
    me: &ObjRef,
    world: &Rc<World>,
    dw: &Rc<DynamicWorld>,
    save_dict: &PlistDict,
    cache: CacheRef,
) -> bool {
    let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
    let inited = base_init_with_world_dynamic_world_save_dict_cache(me, &wref, &dref, save_dict, cache);
    debug_assert!(inited);
    let t = tcl(me);
    let get = |k: &str| save_dict.map.get(&k.to_string());
    let max = t.max_number_of_riders();
    // `maxNumberOfRiders` is 0, 1 or 2 for every car class, so the index always fits the two-element array
    for i in 0..max {
        let mut s = t.tc().borrow_mut();
        let idx = i as usize;
        s.saved_blockhead_index[idx] = -1;
        if let Some(n) = get(&format!("currentBlockheadIndex_{}", i)) {
            s.saved_blockhead_index[idx] = n.unsigned_long_value() as i32;
        }
    }
    {
        let mut s = t.tc().borrow_mut();
        if let Some(v) = get("rightCarID") {
            s.remote_right_car_id = v.unsigned_long_value();
        }
        if let Some(v) = get("leftCarID") {
            s.remote_left_car_id = v.unsigned_long_value();
        }
        if let Some(v) = get("engineCarID") {
            s.remote_engine_car_id = v.unsigned_long_value();
        }
        s.engine_is_right = get("engineIsRight").map_or(false, |v| v.bool_value());
    }
    me.set_owner_id(get("ownerID").and_then(|v| v.as_str()).map(|s| s.to_string()));
    t.load_derived_stuff(me);
    true
}

/// `-[TrainCar initWithWorld:dynamicWorld:cache:netData:]` (`isNet` replica; reachable on the server
/// only through `DynamicWorld.remoteCreate`,).
pub fn train_car_init_with_world_dynamic_world_cache_net_data(
    me: &ObjRef,
    world: &Rc<World>,
    dw: &Rc<DynamicWorld>,
    cache: CacheRef,
    net_data: &[u8],
) -> bool {
    let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
    if !base_init_with_world_dynamic_world_cache_net_data(me, &wref, &dref, cache, net_data) {
        return false;
    }
    tcl(me).load_derived_stuff(me);
    {
        let mut b = me.base().borrow_mut();
        b.float_pos.y = b.pos.y as f32;
        b.float_pos.x = 0.5f32 + b.pos.x as f32;
    }
    me.remote_update(me, net_data);
    true
}

/// `-[TrainCar blockheadsLoaded]`.
pub fn train_car_blockheads_loaded(me: &ObjRef) {
    let t = tcl(me);
    let max = t.max_number_of_riders().min(2);
    for i in 0..max {
        let idx = i as usize;
        let blockheads = t.tc_dw().blockheads();
        let saved = t.tc().borrow().saved_blockhead_index[idx];
        if saved >= 0 && (saved as u32 as u64) < blockheads.len() as u64 {
            t.tc().borrow_mut().riders[idx] = Some(blockheads[saved as usize].clone());
        }
        t.tc().borrow_mut().saved_blockhead_index[idx] = -1;
        let r = t.tc().borrow().riders[idx].clone();
        if !rider_controlled_by_server(&r) {
            if let Some(r) = &r {
                rider_set_riding_object(r, me);
            }
        } else if let Some(r) = &r {
            rider_stop_riding(r);
            t.tc().borrow_mut().riders[idx] = None;
        }
    }
}

/// `-[TrainCar getSaveDict]` (rider index = position in `[dynamicWorld blockheads]`; car ids are
/// 32-bit `numberWithInt:`).
pub fn train_car_get_save_dict(me: &ObjRef) -> PlistDict {
    let t = tcl(me);
    let mut d = base_get_save_dict(me);
    let max = t.max_number_of_riders().min(2);
    for i in 0..max {
        let r = t.tc().borrow().riders[i as usize].clone();
        if let Some(r) = r {
            let mut j: i64 = 0;
            let mut found = false;
            for bh in t.tc_dw().blockheads() {
                if ptr_eq(&bh, &r) && !bh.needs_removed() {
                    found = true;
                    break;
                }
                j += 1;
            }
            if found {
                d.map.insert(format!("currentBlockheadIndex_{}", i), PlistValue::int(j));
            }
        }
    }
    let (rc, lc, ec, eir) = {
        let s = t.tc().borrow();
        (s.right_car.clone(), s.left_car.clone(), s.engine_car.clone(), s.engine_is_right)
    };
    if let Some(c) = rc {
        d.map.insert("rightCarID".to_string(), PlistValue::int(c.unique_id() as i32 as i64));
    }
    if let Some(c) = lc {
        d.map.insert("leftCarID".to_string(), PlistValue::int(c.unique_id() as i32 as i64));
    }
    if let Some(c) = ec {
        d.map.insert("engineCarID".to_string(), PlistValue::int(c.unique_id() as i32 as i64));
    }
    if let Some(o) = me.owner_id() {
        d.map.insert("ownerID".to_string(), PlistValue::string(o));
    }
    d.map.insert("engineIsRight".to_string(), PlistValue::boolean(eir));
    d
}

/// `-[TrainCar freeBlockCreationSaveDict]` = the virtual `getSaveDict`.
pub fn train_car_free_block_creation_save_dict(me: &ObjRef) -> PlistDict {
    me.get_save_dict(me)
}

/// `-[TrainCar trainCarCreationNetData]`. Reads `riders[1]` regardless of `maxNumberOfRiders`;
/// padding is written as zero.
pub fn train_car_train_car_creation_net_data(me: &ObjRef) -> TrainCarCreationNetData {
    let t = tcl(me);
    let s = t.tc().borrow();
    let mut o = TrainCarCreationNetData::default();
    o.dynamic_object_net_data = base_dynamic_object_net_data(me);
    o.rider0_id = s.riders[0].as_ref().map_or(0, |r| r.unique_id());
    o.rider1_id = s.riders[1].as_ref().map_or(0, |r| r.unique_id());
    o.right_car_id = s.right_car.as_ref().map_or(0, |r| r.unique_id());
    o.left_car_id = s.left_car.as_ref().map_or(0, |r| r.unique_id());
    o.engine_car_id = s.engine_car.as_ref().map_or(0, |r| r.unique_id());
    o.left_wheel_pos_x = s.left_wheel_pos.x;
    o.left_wheel_pos_y = s.left_wheel_pos.y;
    o.left_wheel_vel_x = s.left_wheel_vel.x;
    o.left_wheel_vel_y = s.left_wheel_vel.y;
    o.right_wheel_pos_x = s.right_wheel_pos.x;
    o.right_wheel_pos_y = s.right_wheel_pos.y;
    o.right_wheel_vel_x = s.right_wheel_vel.x;
    o.right_wheel_vel_y = s.right_wheel_vel.y;
    o.visual_velocity_x = s.visual_velocity.x;
    o.engine_is_right = s.engine_is_right as u8;
    o
}

/// `-[TrainCar creationNetDataForClient:]`: the bare 0x68-byte struct.
pub fn train_car_creation_net_data_for_client(me: &ObjRef) -> Vec<u8> {
    train_car_train_car_creation_net_data(me).to_vec()
}

/// Shared by HandCar / PassengerCar `creationNetDataForClient:`: `[0x68 struct][gzip(XML plist {ownerID?})]`; the
/// dictionary is serialised even when empty.
pub fn train_car_owner_tail_creation_net_data(me: &ObjRef) -> Vec<u8> {
    let mut out = train_car_creation_net_data_for_client(me);
    let mut dict = new_mutable_dict();
    if let Some(o) = me.owner_id() {
        dict.map.insert("ownerID".to_string(), PlistValue::string(o));
    }
    // [nil gzipDeflate] is nil and appendData:nil appends nothing
    if let Some(p) = data_from_property_list(&PlistValue::Dict(dict)) {
        out.extend_from_slice(&gzip_deflate(&p));
    }
    out
}

/// Tail reader shared by the netData constructors of Hand/Passenger: `ownerID = plist["ownerID"]` when the data is
/// longer than `head` bytes.
pub fn read_owner_id_tail(me: &ObjRef, net_data: &[u8], head: usize) {
    if net_data.len() > head {
        let inflated = gzip_inflate(&net_data[head..]);
        let dict = inflated.as_deref().and_then(property_list_from_data);
        me.set_owner_id(
            dict.as_ref().and_then(|d| d.object_for_key("ownerID")).and_then(|v| v.as_str()).map(|s| s.to_string()),
        );
    }
}

/// `-[TrainCar remoteUpdate:]` (server branch; the replica-only link resolution runs when `isNet`).
pub fn train_car_remote_update(me: &ObjRef, net_data: &[u8]) {
    base_remote_update(me, net_data);
    if me.needs_removed() {
        return;
    }
    let t = tcl(me);
    let u = TrainCarCreationNetData::read_le(&padded_bytes(net_data, 0x68));
    let dw = t.tc_dw();
    let max = t.max_number_of_riders().min(2);
    // dynamicWorld isClient is NO on the server: SERVER branch
    for i in 0..max {
        let idx = i as usize;
        let rid = if i == 0 { u.rider0_id } else { u.rider1_id };
        let r = t.tc().borrow().riders[idx].clone();
        if let Some(rr) = &r {
            if !rr.is_net() {
                if rid != 0 && rid != rr.unique_id() {
                    rider_stop_riding(rr);
                    t.tc().borrow_mut().riders[idx] = None;
                }
                continue;
            }
        }
        me.base().borrow_mut().update_needs_to_be_sent = true;
        if rid == 0 {
            let cur = t.tc().borrow().riders[idx].clone();
            if let Some(c) = cur {
                rider_stop_riding(&c);
                t.tc().borrow_mut().riders[idx] = None;
            }
        } else {
            let cur = t.tc().borrow().riders[idx].clone();
            if cur.as_ref().map_or(false, |c| c.unique_id() == rid) {
                continue;
            }
            for bh in dw.all_blockheads_including_net() {
                if bh.is_net() && bh.unique_id() == rid {
                    t.tc().borrow_mut().riders[idx] = Some(bh);
                    me.base().borrow_mut().update_needs_to_be_sent = true;
                    car_changed_notify(me);
                    break;
                }
            }
        }
    }
    // wheel sync
    let (mut has_local, mut has_net) = (false, false);
    for i in 0..max {
        if let Some(r) = t.tc().borrow().riders[i as usize].as_ref() {
            if r.is_net() {
                has_net = true;
            } else {
                has_local = true;
            }
        }
    }
    if !has_local && (has_net || me.is_net()) {
        let mut s = t.tc().borrow_mut();
        s.left_wheel_pos = Vector2 { x: u.left_wheel_pos_x, y: u.left_wheel_pos_y };
        s.left_wheel_vel = Vector2 { x: u.left_wheel_vel_x, y: u.left_wheel_vel_y };
        s.right_wheel_pos = Vector2 { x: u.right_wheel_pos_x, y: u.right_wheel_pos_y };
        s.right_wheel_vel = Vector2 { x: u.right_wheel_vel_x, y: u.right_wheel_vel_y };
        s.visual_velocity.x = u.visual_velocity_x;
    }
    if !me.is_net() {
        return;
    }
    // replica-only: car links, order right, left, engine
    t.tc().borrow_mut().engine_is_right = u.engine_is_right != 0;
    for which in 0..3 {
        let id = match which {
            0 => u.right_car_id,
            1 => u.left_car_id,
            _ => u.engine_car_id,
        };
        if id == 0 {
            let mut s = t.tc().borrow_mut();
            match which {
                0 => {
                    s.right_car = None;
                    s.remote_right_car_id = 0;
                }
                1 => {
                    s.left_car = None;
                    s.remote_left_car_id = 0;
                }
                _ => {
                    s.engine_car = None;
                    s.remote_engine_car_id = 0;
                }
            }
        } else {
            let cur = {
                let s = t.tc().borrow();
                match which {
                    0 => s.right_car.clone(),
                    1 => s.left_car.clone(),
                    _ => s.engine_car.clone(),
                }
            };
            if cur.as_ref().map_or(false, |c| c.unique_id() == id) {
                continue;
            }
            let car = dw.train_car_with_id(id);
            let remote = if car.is_none() { id } else { 0 };
            let mut s = t.tc().borrow_mut();
            match which {
                0 => {
                    s.right_car = car;
                    s.remote_right_car_id = remote;
                }
                1 => {
                    s.left_car = car;
                    s.remote_left_car_id = remote;
                }
                _ => {
                    s.engine_car = car;
                    s.remote_engine_car_id = remote;
                }
            }
        }
    }
}

/// `-[TrainCar blockheadCanRide:usingItem:]`.
pub fn train_car_blockhead_can_ride_using_item(me: &ObjRef) -> bool {
    if !me.needs_removed() {
        let t = tcl(me);
        for i in 0..t.max_number_of_riders().min(2) {
            if t.tc().borrow().riders[i as usize].is_none() {
                return true;
            }
        }
    }
    false
}

/// `-[TrainCar updatePosition:]`: a car moving in x with a local rider marks the circumnavigation
/// tracker, then the DynamicObject version.
pub fn train_car_update_position(me: &ObjRef, new_position: IntPair) {
    let t = tcl(me);
    if new_position.x != me.pos().x {
        for i in 0..t.max_number_of_riders().min(2) {
            let r = t.tc().borrow().riders[i as usize].clone();
            if r.as_ref().map_or(false, |r| !r.is_net()) {
                t.tc_world().mark_circum_navigate_x(new_position.x);
            }
        }
    }
    base_update_position(me, new_position);
}

/// `-[TrainCar update:accurateDT:isSimulation:]`: only resolves pending car links (the discarded
/// `tileAtWorldPositionLoaded` calls force-load the neighbour chunk).
pub fn train_car_update_accurate_dt_is_simulation(me: &ObjRef, _dt: f32, _accurate_dt: f32, _is_simulation: bool) {
    let t = tcl(me);
    let dw = t.tc_dw();
    let world = t.tc_world();
    let pos = me.pos();
    // left
    let rl = t.tc().borrow().remote_left_car_id;
    if rl != 0 {
        let has = t.tc().borrow().left_car.is_some();
        if !has {
            let c = dw.train_car_with_id(rl);
            let found = c.is_some();
            t.tc().borrow_mut().left_car = c;
            if !found {
                let _ = tile_at_world_position_loaded(pos.x.wrapping_sub(2), pos.y, &world);
            } else {
                t.tc().borrow_mut().remote_left_car_id = 0;
            }
        } else {
            t.tc().borrow_mut().remote_left_car_id = 0;
        }
    }
    // right
    let rr = t.tc().borrow().remote_right_car_id;
    if rr != 0 {
        let has = t.tc().borrow().right_car.is_some();
        if !has {
            let c = dw.train_car_with_id(rr);
            let found = c.is_some();
            t.tc().borrow_mut().right_car = c;
            if !found {
                let _ = tile_at_world_position_loaded(pos.x.wrapping_add(2), pos.y, &world);
            } else {
                t.tc().borrow_mut().remote_right_car_id = 0;
            }
        } else {
            t.tc().borrow_mut().remote_right_car_id = 0;
        }
    }
    // engine
    let re = t.tc().borrow().remote_engine_car_id;
    if re != 0 {
        let has = t.tc().borrow().engine_car.is_some();
        if !has {
            if !t.is_engine() {
                let c = dw.train_car_with_id(re);
                let found = c.is_some();
                t.tc().borrow_mut().engine_car = c;
                if !found {
                    return;
                }
                t.tc().borrow_mut().remote_engine_car_id = 0;
                return;
            }
            // isEngine: fall through
        }
        t.tc().borrow_mut().remote_engine_car_id = 0;
    }
}

/// `-[TrainCar addRider:]`.
pub fn train_car_add_rider(me: &ObjRef, rider: &ObjRef) {
    let t = tcl(me);
    let max = t.max_number_of_riders().min(2);
    let some_rider = Some(rider.clone());
    if rider_controlled_by_server(&some_rider) {
        rider_stop_riding(rider);
        for i in 0..max {
            let is_it = t.tc().borrow().riders[i as usize].as_ref().map_or(false, |r| ptr_eq(r, rider));
            if is_it {
                t.tc().borrow_mut().riders[i as usize] = None;
                // dynamicWorld isClient is NO
                me.base().borrow_mut().update_needs_to_be_sent = true;
                car_changed_notify(me);
            }
        }
        return;
    }
    let mut first_space: i32 = -1;
    for i in 0..max {
        let (is_it, is_nil) = {
            let s = t.tc().borrow();
            let r = &s.riders[i as usize];
            (r.as_ref().map_or(false, |r| ptr_eq(r, rider)), r.is_none())
        };
        if is_it {
            return;
        }
        if is_nil && first_space == -1 {
            first_space = i;
        }
    }
    if first_space == -1 {
        return;
    }
    t.tc().borrow_mut().riders[first_space as usize] = Some(rider.clone());
    // not (isClient &&...): always true on the server
    me.base().borrow_mut().update_needs_to_be_sent = true;
    car_changed_notify(me);
}

/// `-[TrainCar removeRider:]`.
pub fn train_car_remove_rider(me: &ObjRef, rider: &ObjRef) {
    let t = tcl(me);
    let mut hit = None;
    for i in 0..t.max_number_of_riders().min(2) {
        if t.tc().borrow().riders[i as usize].as_ref().map_or(false, |r| ptr_eq(r, rider)) {
            hit = Some(i as usize);
            break;
        }
    }
    let Some(i) = hit else { return };
    t.tc().borrow_mut().riders[i] = None;
    me.base().borrow_mut().update_needs_to_be_sent = true;
    car_changed_notify(me);
}

/// `-[TrainCar worldChanged:]`: converts the car into a free block when its rail tile vanishes
/// (never while ridden; only the first matching position is handled).
pub fn train_car_world_changed(me: &ObjRef, positions: &mut Vec<IntPair>) {
    let t = tcl(me);
    for i in 0..t.max_number_of_riders().min(2) {
        if t.tc().borrow().riders[i as usize].is_some() {
            return;
        }
    }
    if !me.needs_removed() && !me.is_net() {
        let pos = me.pos();
        for p in positions.iter() {
            if p.x == pos.x && p.y == pos.y {
                let world = t.tc_world();
                let tile = tile_at_world_position_loaded(pos.x, pos.y, &world);
                let Some(tile) = tile else { return };
                if !tile_is_solid(Some(tile)) && tile.foreground_contents() == CONTENTS_RAIL {
                    return;
                }
                let prio = t.tc().borrow().riders[0].clone();
                car_drop_as_free_block(me, prio.as_ref());
                return;
            }
        }
    }
}

/// `-[TrainCar tapIsWithinBodyRadius:]` (HandCar uses it unchanged).
pub fn train_car_tap_is_within_body_radius(me: &ObjRef, tap: Vector2) -> bool {
    let t = tcl(me);
    let m = t.tc_world().world_width_macro();
    let fp = me.float_pos();
    let dx = tap.x - fp.x;
    let d = wrap_dx(dx, m);
    (d as f64) < 0.75 && -0.75 < (d as f64) && tap.y > fp.y && tap.y < fp.y + 2.0f32
}

/// `-[TrainCar setNeedsRemoved:]`.
pub fn train_car_set_needs_removed(me: &ObjRef, v: bool) {
    if v && !me.needs_removed() {
        let t = tcl(me);
        for i in 0..t.max_number_of_riders().min(2) {
            let r = t.tc().borrow().riders[i as usize].clone();
            if let Some(r) = r {
                rider_stop_riding(&r);
            }
        }
    }
    me.base().borrow_mut().needs_removed = v;
}

/// `-[TrainCar setRightCar:]`.
pub fn train_car_set_right_car(me: &ObjRef, c: Option<&ObjRef>) {
    let t = tcl(me);
    let new = c.cloned();
    let differs = !same_opt(&new, &t.tc().borrow().right_car);
    if !me.is_net() && differs {
        me.base().borrow_mut().update_needs_to_be_sent = true;
    }
    t.tc().borrow_mut().right_car = new;
}

/// `-[TrainCar setLeftCar:]`.
pub fn train_car_set_left_car(me: &ObjRef, c: Option<&ObjRef>) {
    let t = tcl(me);
    let new = c.cloned();
    let differs = !same_opt(&new, &t.tc().borrow().left_car);
    if !me.is_net() && differs {
        me.base().borrow_mut().update_needs_to_be_sent = true;
    }
    t.tc().borrow_mut().left_car = new;
}

/// `[car engineCar]` through an `ObjRef` (nil for a non-car).
pub fn car_engine_car(c: &ObjRef) -> Option<ObjRef> {
    train_car_of(c).and_then(|t| t.engine_car(c))
}

/// `-[TrainCar setEngineCar:]`: propagates along the consist and recomputes `engineIsRight`.
/// Engines ignore the call.
pub fn train_car_set_engine_car(me: &ObjRef, engine: Option<&ObjRef>) {
    let t = tcl(me);
    if t.is_engine() {
        return;
    }
    let new = engine.cloned();
    let differs = !same_opt(&new, &t.tc().borrow().engine_car);
    if !me.is_net() && differs {
        me.base().borrow_mut().update_needs_to_be_sent = true;
    }
    t.tc().borrow_mut().engine_car = new.clone();
    let (lc, rc) = {
        let s = t.tc().borrow();
        (s.left_car.clone(), s.right_car.clone())
    };
    if let Some(l) = &lc {
        if !same_opt(&car_engine_car(l), &new) {
            if let Some(lt) = train_car_of(l) {
                lt.set_engine_car(l, new.as_ref());
            }
        }
    }
    if let Some(r) = &rc {
        if !same_opt(&car_engine_car(r), &new) {
            if let Some(rt) = train_car_of(r) {
                rt.set_engine_car(r, new.as_ref());
            }
        }
    }
    if new.is_some() {
        t.tc().borrow_mut().engine_is_right = true;
        let mut x = me.clone();
        loop {
            let next = train_car_of(&x).and_then(|c| c.left_car());
            match next {
                None => return,
                Some(n) => {
                    if train_car_of(&n).map_or(false, |c| c.is_engine()) {
                        break;
                    }
                    x = n;
                }
            }
        }
        t.tc().borrow_mut().engine_is_right = false;
    }
}

/// `-[TrainCar blockheadUnloaded:]`: no `stopRiding`, no update flag.
pub fn train_car_blockhead_unloaded(me: &ObjRef, b: &ObjRef) {
    let t = tcl(me);
    for i in 0..t.max_number_of_riders().min(2) {
        let hit = t.tc().borrow().riders[i as usize].as_ref().map_or(false, |r| ptr_eq(r, b));
        if hit {
            t.tc().borrow_mut().riders[i as usize] = None;
        }
    }
}

/// `-[TrainCar removeFromMacroBlock]`: `[super removeFromMacroBlock]; [self cleanup]`.
pub fn train_car_remove_from_macro_block(me: &ObjRef) {
    base_remove_from_macro_block(me);
    tcl(me).cleanup(me);
}

/// `-[TrainCar cleanup]`: drops the car links and the cached wheel rails (breaks the retain cycles).
pub fn train_car_cleanup(me: &ObjRef) {
    let t = tcl(me);
    let mut s = t.tc().borrow_mut();
    s.right_car = None;
    s.left_car = None;
    s.engine_car = None;
    s.left_wheel_rail = None;
    s.right_wheel_rail = None;
}

/// `-[TrainCar renderPos]`: `floatPos + (0, 1)`.
pub fn train_car_render_pos(me: &ObjRef) -> Vector2 {
    let fp = me.float_pos();
    Vector2 { x: fp.x + 0.0f32, y: fp.y + 1.0f32 }
}

// ------------------------------------------------------------------------------------------------ rolling cars (Freight, Passenger)

/// Steps 2-13 of `PassengerCar update:` / steps 1-9 of `FreightCar update:` after `[super update:]` and the dt
/// clamp (`dtc`): rider cleanup, neighbour link maintenance, engine-less damping, wheel/rail resolution, coupling
/// pulls, integration, derail checks. `FreightCar` has no riders (`maxNumberOfRiders` 0), so its rider cleanup is
/// empty.
pub fn rolling_car_update_after_super(me: &ObjRef, dtc: f32) {
    let t = tcl(me);
    let dw = t.tc_dw();
    let world = t.tc_world();
    let macro_w = world.world_width_macro();

    // 2. rider cleanup
    for i in 0..t.max_number_of_riders().min(2) {
        let r = t.tc().borrow().riders[i as usize].clone();
        if rider_needs_removed(&r) {
            if !rider_is_net(&r) {
                me.base().borrow_mut().update_needs_to_be_sent = true;
            }
            t.tc().borrow_mut().riders[i as usize] = None;
        }
    }

    // 3. neighbour link maintenance
    if !me.is_net() {
        // left
        let lc = t.tc().borrow().left_car.clone();
        if lc.as_ref().map_or(false, |c| c.needs_removed()) {
            let lc = lc.unwrap();
            let mut engine_found = train_car_of(&lc).map_or(false, |c| c.is_engine());
            if !engine_found {
                let mut c = lc.clone();
                loop {
                    let n = train_car_of(&c).and_then(|x| x.left_car());
                    match n {
                        None => break,
                        Some(n) => {
                            if train_car_of(&n).map_or(false, |x| x.is_engine()) {
                                engine_found = true;
                                break;
                            }
                            c = n;
                        }
                    }
                }
            }
            if engine_found {
                let mut c = me.clone();
                loop {
                    let n = train_car_of(&c).and_then(|x| x.right_car());
                    match n {
                        None => break,
                        Some(n) => {
                            if let Some(nt) = train_car_of(&n) {
                                nt.set_engine_car(&n, None);
                            }
                            c = n;
                        }
                    }
                }
            }
            t.tc().borrow_mut().left_car = None;
            if !me.is_net() {
                me.base().borrow_mut().update_needs_to_be_sent = true;
            }
        }
        // right
        let rc = t.tc().borrow().right_car.clone();
        if rc.as_ref().map_or(false, |c| c.needs_removed()) {
            let rc = rc.unwrap();
            let mut engine_found = train_car_of(&rc).map_or(false, |c| c.is_engine());
            if !engine_found {
                let mut c = rc.clone();
                loop {
                    let n = train_car_of(&c).and_then(|x| x.right_car());
                    match n {
                        None => break,
                        Some(n) => {
                            if train_car_of(&n).map_or(false, |x| x.is_engine()) {
                                engine_found = true;
                                break;
                            }
                            c = n;
                        }
                    }
                }
            }
            if engine_found {
                let mut c = me.clone();
                loop {
                    let n = train_car_of(&c).and_then(|x| x.left_car());
                    match n {
                        None => break,
                        Some(n) => {
                            if let Some(nt) = train_car_of(&n) {
                                nt.set_engine_car(&n, None);
                            }
                            c = n;
                        }
                    }
                }
            }
            t.tc().borrow_mut().right_car = None;
            me.base().borrow_mut().update_needs_to_be_sent = true;
        }
        // engine
        let ec = t.tc().borrow().engine_car.clone();
        if ec.as_ref().map_or(false, |c| c.needs_removed()) {
            t.tc().borrow_mut().engine_car = None;
            me.base().borrow_mut().update_needs_to_be_sent = true;
        }
    } else {
        // replica: resolve pending ids, order right, left, engine
        // `train_car_update_accurate_dt_is_simulation` (the caller's `[super update:]`) already resolved and cleared
        // every id the dynamic world knows, so an id left here names an absent car: the lookup can only come back
        // empty and the original's clear-on-found step has nothing to clear. The same update leaves a non-zero id
        // only while the matching link is still empty (a held link clears its id), so the lookup below never
        // replaces a car that already carries that id.
        let rid = t.tc().borrow().remote_right_car_id;
        if rid != 0 {
            t.tc().borrow_mut().right_car = dw.train_car_with_id(rid);
        }
        let lid = t.tc().borrow().remote_left_car_id;
        if lid != 0 {
            t.tc().borrow_mut().left_car = dw.train_car_with_id(lid);
        }
        let eid = t.tc().borrow().remote_engine_car_id;
        if eid != 0 {
            t.tc().borrow_mut().engine_car = dw.train_car_with_id(eid);
        }
    }

    // 4. engine-less damping
    if t.tc().borrow().engine_car.is_none() {
        let fp = me.float_pos();
        let f = fmodf(0.5f32 + fp.x, 1.0f32);
        let mut s = t.tc().borrow_mut();
        s.left_wheel_vel.x = ((s.left_wheel_vel.x as f64) * (1.0f64 - dtc as f64)) as f32;
        s.right_wheel_vel.x = ((s.right_wheel_vel.x as f64) * (1.0f64 - dtc as f64)) as f32;
        if f <= 0.5f32 {
            s.left_wheel_vel.x -= f * dtc * 0.2f32;
            s.right_wheel_vel.x -= f * dtc * 0.2f32;
        } else {
            s.left_wheel_vel.x += (1.0f32 - f) * dtc * 0.2f32;
            s.right_wheel_vel.x += (1.0f32 - f) * dtc * 0.2f32;
        }
    }

    // 5. wheel tile sampling
    let (lw0, rw0, lv0, rv0) = {
        let s = t.tc().borrow();
        (s.left_wheel_pos, s.right_wheel_pos, s.left_wheel_vel, s.right_wheel_vel)
    };
    let mut new_l = IntPair { x: f32_to_i32(lw0.x), y: f32_to_i32(lw0.y) };
    let mut new_r = IntPair { x: f32_to_i32(rw0.x), y: f32_to_i32(rw0.y) };
    let lt = tile_at_world_position_loaded(new_l.x, new_l.y, &world);
    let rt = tile_at_world_position_loaded(new_r.x, new_r.y, &world);
    let avg_vel = lv0.add(rv0).mul_f(0.5f32);
    let (Some(lt), Some(rt)) = (lt, rt) else { return };
    let mut left_air = false;
    let mut right_air = false;
    let mut left_water = false;
    let mut right_water = false;
    let mut left_acc = Vector2::ZERO;
    let mut right_acc = Vector2::ZERO;
    for side in 0..2 {
        let (tile, tp) = if side == 0 { (lt, &mut new_l) } else { (rt, &mut new_r) };
        match classify_wheel_tile(&world, tile, *tp) {
            WheelTile::Rail => {}
            WheelTile::RailAbove => tp.y = tp.y.wrapping_add(1),
            WheelTile::Solid => {
                if me.needs_removed() {
                    return;
                }
                if me.is_net() {
                    return;
                }
                car_drop_as_free_block(me, None);
                return;
            }
            WheelTile::Air { water } => {
                if side == 0 {
                    left_acc.y = -2.0;
                    left_air = true;
                    left_water = water;
                } else {
                    right_acc.y = -2.0;
                    right_air = true;
                    right_water = water;
                }
            }
        }
    }

    // 6. rail (re)acquisition
    refresh_wheel_rails(t, new_l, new_r);

    // 7. wheel/rail height constraint
    let (mut lw, mut rw, mut lv, mut rv) = (lw0, rw0, lv0, rv0);
    let (lcfg, rcfg) = {
        let s = t.tc().borrow();
        (
            s.left_wheel_rail.as_ref().map(rail_current_configuration),
            s.right_wheel_rail.as_ref().map(rail_current_configuration),
        )
    };
    let xl = lw.x - new_l.x as f32;
    let xr = rw.x - new_r.x as f32;
    resolve_wheel_vertical(lcfg, new_l, xl, avg_vel, &mut lw, &mut lv, &mut left_acc, &mut left_air);
    resolve_wheel_vertical(rcfg, new_r, xr, avg_vel, &mut rw, &mut rv, &mut right_acc, &mut right_air);

    // 8. velocity integration
    let mask = Vector2 { x: 0.0, y: 0.5 };
    let comb_l = left_acc.mul_vec(mask);
    let comb_r = right_acc.mul_vec(mask);
    let drag = (1.0f64 - (dtc as f64) * 0.2f64) as f32;
    lv = lv.mul_f(drag).add(comb_l.mul_f(dtc).mul_f(2.0f32));
    rv = rv.mul_f(drag).add(comb_r.mul_f(dtc).mul_f(2.0f32));
    let left_mul = if left_water { 0.1f32 } else { 1.0f32 };
    let right_mul = if right_water { 0.1f32 } else { 1.0f32 };

    // 9. coupling to the engine-side neighbour
    let (right_car, left_car, engine_is_right, has_engine) = {
        let s = t.tc().borrow();
        (s.right_car.clone(), s.left_car.clone(), s.engine_is_right, s.engine_car.is_some())
    };
    if let (Some(rc), true, true) = (&right_car, engine_is_right, has_engine) {
        let p = train_car_of(rc).map_or(Vector2::ZERO, |c| c.left_wheel_pos());
        let mut d = p.sub(rw).normal();
        if (d.x as f64) < 0.5 {
            d.x = 0.5;
            d = d.normal();
        }
        let diff = p.sub(d).sub(rw);
        rw = rw.add(diff.mul_f(0.99f32));
        lw = lw.add(diff.mul_f(0.99f32));
    }
    if let (Some(lc), false, true) = (&left_car, engine_is_right, has_engine) {
        let p = train_car_of(lc).map_or(Vector2::ZERO, |c| c.right_wheel_pos());
        let mut d = p.sub(lw).normal();
        if (d.x as f64) > -0.5 {
            d.x = -0.5;
            d = d.normal();
        }
        let diff = p.sub(d).sub(lw);
        lw = lw.add(diff.mul_f(0.99f32));
        rw = rw.add(diff.mul_f(0.99f32));
    }

    // 10. position integration + world wrap
    lw = lw.add(lv.mul_f(dtc).mul_f(8.0f32).mul_f(left_mul));
    rw = rw.add(rv.mul_f(dtc).mul_f(8.0f32).mul_f(right_mul));
    let wf = macro_w.wrapping_shl(5) as f32;
    if lw.x >= 0.0 {
        if wf <= lw.x {
            lw.x -= wf;
            rw.x -= wf;
        }
    } else {
        lw.x += wf;
        rw.x += wf;
    }

    // 11. floatPos and visual velocity
    let old = me.float_pos();
    let fp = lw.mul_f(0.5f32).add(rw.mul_f(0.5f32));
    me.base().borrow_mut().float_pos = fp;
    if dtc > 0.0 {
        let dx = fp.x - old.x;
        let vx = wrap_dx(dx, macro_w);
        let mut s = t.tc().borrow_mut();
        s.visual_velocity.x = vx / dtc;
        s.visual_velocity.y = (fp.y - old.y) / dtc;
    }

    // 12. re-normalise the wheel axis, tile change
    let wd = rw.sub(lw).normal();
    lw = fp.add(wd.mul_f(-1.0f32));
    rw = fp.add(wd.mul_f(1.0f32));
    {
        let mut s = t.tc().borrow_mut();
        s.left_wheel_pos = lw;
        s.right_wheel_pos = rw;
        s.left_wheel_vel = lv;
        s.right_wheel_vel = rv;
    }
    let new_pos = IntPair { x: f32_to_i32(fp.x), y: f32_to_i32(fp.y) };
    if new_pos != me.pos() {
        me.update_position(me, new_pos);
    }

    // 13. flipped-car derail
    if !me.is_net() && !(left_air && right_air) && lw.x > rw.x && !me.needs_removed() {
        car_drop_as_free_block(me, None);
    }
}

/// Placement-time neighbour search shared by every class (offsets -1 then +1 are tried by the callers): finds the
/// rail one or two tiles away (a NULL tile after a slope probe counts as "not a rail": `UB-PARITY`,)
/// and returns the car found by `checkForTrainCarUnderTap:` at `(rr.x + 0.5, rr.y + 1.5)` (the callers apply their
/// own acceptance test).
pub fn neighbour_car_for_placement(me: &ObjRef, offset: i32) -> Option<ObjRef> {
    let t = tcl(me);
    let world = t.tc_world();
    let dw = t.tc_dw();
    let pos = me.pos();
    let mut p = IntPair { x: pos.x.wrapping_add(offset), y: pos.y };
    let tile = tile_at_world_position_loaded(p.x, p.y, &world)?;
    let mut ok = true;
    if tile.foreground_contents() != CONTENTS_RAIL {
        p = if !tile_is_solid(Some(tile)) {
            IntPair { x: pos.x.wrapping_add(offset), y: pos.y.wrapping_sub(1) }
        } else {
            IntPair { x: pos.x.wrapping_add(offset), y: pos.y.wrapping_add(1) }
        };
        // UB-PARITY: no nil check in the original
        ok =
            tile_at_world_position_loaded(p.x, p.y, &world).map_or(false, |t| t.foreground_contents() == CONTENTS_RAIL);
    }
    if !ok {
        return None;
    }
    let mut rr = IntPair { x: p.x.wrapping_add(offset), y: p.y };
    let t2 = tile_at_world_position_loaded(rr.x, rr.y, &world)?;
    let mut rail = true;
    if t2.foreground_contents() != CONTENTS_RAIL {
        rr = if !tile_is_solid(Some(t2)) {
            IntPair { x: p.x.wrapping_add(offset), y: p.y.wrapping_sub(1) }
        } else {
            IntPair { x: p.x.wrapping_add(offset), y: p.y.wrapping_add(1) }
        };
        // UB-PARITY: no nil check in the original
        if tile_at_world_position_loaded(rr.x, rr.y, &world).map_or(true, |t| t.foreground_contents() != CONTENTS_RAIL)
        {
            rail = false;
        }
    }
    if !rail {
        return None;
    }
    let centre = Vector2 { x: rr.x as f32 + 0.5f32, y: rr.y as f32 + 1.5f32 };
    dw.check_for_train_car_under_tap(centre)
}

/// Placement-time coupling of FreightCar / PassengerCar to the neighbouring cars (offset -1 then +1): any
/// `connectsToOtherCars` car is linked, its engine is adopted, then a car that is an engine becomes the engine.
pub fn couple_rolling_car_at_placement(me: &ObjRef) {
    let t = tcl(me);
    for offset in [-1i32, 1i32] {
        let Some(car) = neighbour_car_for_placement(me, offset) else { continue };
        let Some(ct) = train_car_of(&car) else { continue };
        if !ct.connects_to_other_cars() {
            continue;
        }
        if offset == -1 {
            t.tc().borrow_mut().left_car = Some(car.clone());
            ct.set_right_car(&car, Some(me));
        } else {
            t.tc().borrow_mut().right_car = Some(car.clone());
            ct.set_left_car(&car, Some(me));
        }
        if t.tc().borrow().engine_car.is_none() {
            let e = ct.engine_car(&car);
            t.set_engine_car(me, e.as_ref());
        }
        // (an engine neighbour is already its own `engineCar`, so the adoption above has set it)
    }
}

// ------------------------------------------------------------------------------------------------ protocol impls

/// Implements `RidableDynamicObject` and `TapChoiceDynamicObject` for a concrete car type by delegating to its
/// [`TrainCarLike`] implementation.
macro_rules! impl_train_car_protocols {
    ($ty:ty) => {
        impl $crate::objects::protocols::RidableDynamicObject for $ty {
            fn rider_pos_for_blockhead(&self, b: &$crate::objects::handle::ObjRef) -> [f32; 4] {
                $crate::trains::train_car::TrainCarLike::rider_pos_for_blockhead(self, b)
            }
            /// CLIENT-ONLY (`cameraPosForBlockhead:` = `renderPos`).
            fn camera_pos_for_blockhead(
                &self,
                _b: &$crate::objects::handle::ObjRef,
            ) -> $crate::types::structs::Vector2 {
                $crate::objects::base::DynamicObject::render_pos(self)
            }
            /// CLIENT-ONLY.
            fn rider_body_y_rotation_for_blockhead(&self, _b: &$crate::objects::handle::ObjRef) -> f32 {
                0.0
            }
            fn set_target_velocity(&self, v: $crate::types::structs::Vector2) {
                $crate::trains::train_car::TrainCarLike::set_target_velocity(self, v)
            }
            fn ride_direction(&self) -> i32 {
                0
            }
            fn add_rider(&self, me: &$crate::objects::handle::ObjRef, b: &$crate::objects::handle::ObjRef) {
                $crate::trains::train_car::TrainCarLike::add_rider(self, me, b)
            }
            fn remove_rider(&self, me: &$crate::objects::handle::ObjRef, b: &$crate::objects::handle::ObjRef) {
                $crate::trains::train_car::TrainCarLike::remove_rider(self, me, b)
            }
            fn swipe_up_gesture(&self, _me: &$crate::objects::handle::ObjRef) {}
            fn blockhead_can_ride_using_item(&self, b: &$crate::objects::handle::ObjRef, item: i32) -> bool {
                let me = $crate::objects::base::DynamicObject::me(self);
                $crate::trains::train_car::TrainCarLike::blockhead_can_ride_using_item(self, &me, b, item)
            }
            fn jumps_on_swipe(&self) -> bool {
                false
            }
            fn requires_fuel(&self) -> bool {
                $crate::trains::train_car::TrainCarLike::requires_fuel(self)
            }
            ///  (FreightCar returns its chest through `acts_as_interaction_object_override`).
            fn acts_as_interaction_object(&self) -> Option<$crate::objects::handle::ObjRef> {
                self.acts_as_interaction_object_impl()
            }
            fn rider_d_pad_should_allow_up_down(&self) -> bool {
                false
            }
            fn rider_d_pad_should_give_discrete_values(&self) -> bool {
                false
            }
        }
        impl $crate::objects::protocols::TapChoiceDynamicObject for $ty {
            fn action_title(&self) -> String {
                $crate::trains::train_car::TrainCarLike::action_title(self)
            }
            fn is_double_height(&self) -> bool {
                false
            }
        }
    };
}
pub(crate) use impl_train_car_protocols;

/// The `DynamicObject` methods that are identical for every car class (the classes add their own overrides in
/// the same `impl` block).
macro_rules! train_car_dynamic_object_common {
    () => {
        fn base(&self) -> &std::cell::RefCell<$crate::objects::base::DynamicObjectBase> {
            &self.base
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn world_changed(
            &self,
            me: &$crate::objects::handle::ObjRef,
            positions: &mut Vec<$crate::types::structs::IntPair>,
        ) {
            $crate::trains::train_car::train_car_world_changed(me, positions)
        }
        fn blockhead_unloaded(&self, me: &$crate::objects::handle::ObjRef, b: &$crate::objects::handle::ObjRef) {
            $crate::trains::train_car::train_car_blockhead_unloaded(me, b)
        }
        fn remove_from_macro_block(&self, me: &$crate::objects::handle::ObjRef) {
            $crate::trains::train_car::train_car_remove_from_macro_block(me)
        }
        fn as_ridable(&self) -> Option<&dyn $crate::objects::protocols::RidableDynamicObject> {
            Some(self)
        }
        fn as_tap_choice(&self) -> Option<&dyn $crate::objects::protocols::TapChoiceDynamicObject> {
            Some(self)
        }
    };
}
pub(crate) use train_car_dynamic_object_common;

// keep the trait imports used by the macro expansions alive
#[allow(dead_code)]
fn _use_traits(_: &dyn RidableDynamicObject, _: &dyn TapChoiceDynamicObject) {}

// ------------------------------------------------------------------------------------------------ tests

#[cfg(test)]
pub(crate) mod world_tests {
    //! World-backed tests: a flat rail line on solid ground, cars placed through the
    //! real `DynamicWorld` placement path with the constructors installed as `DwOpsHooks`.
    use super::*;
    use crate::dynworld::lookups::{set_ops_hooks, DwOpsHooks};
    use crate::dynworld::state::testutil::dw as test_dw;
    use crate::objects::handle::ObjRef;
    use crate::trains::freight_car::FreightCar;
    use crate::trains::hand_car::HandCar;
    use crate::trains::passenger_car::PassengerCar;
    use crate::trains::rail::Rail;
    use crate::trains::steam_train::SteamTrain;
    use crate::world::state::{MacroTileRef, WorldOpsHooks};

    /// Ground (type 1) below y = 10, air (type 2) above.
    struct Loader;
    impl WorldOpsHooks for Loader {
        fn load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
            &self,
            world: &World,
            mt: &MacroTileRef,
            mx: i32,
            my: i32,
            _surrounding: bool,
            _create: bool,
        ) -> bool {
            let pb = world.take_physical_block();
            pb.x_pos.set(mx);
            pb.y_pos.set(my);
            pb.valid.set(true);
            mt.dirty.set(0);
            for ly in 0..32usize {
                for lx in 0..32usize {
                    let wy = my * 32 + ly as i32;
                    TileRef::new(pb, ly * 32 + lx).update(|t| {
                        t.type_index = if wy < 10 { 1 } else { 2 };
                    });
                }
            }
            *mt.physical_block.borrow_mut() = Some(pb);
            *mt.dynamic_objects.borrow_mut() = Some(Vec::new());
            true
        }
        fn load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(
            &self,
            _w: &World,
            _mt: &MacroTileRef,
            _i: bool,
        ) {
        }
        fn tile_is_protected_at_pos_against_blockhead(&self, _w: &World, _p: IntPair, _b: &ObjRef) -> bool {
            false
        }
    }

    struct Ops;
    impl crate::dynworld::lookups::DwOpsHooksStub for Ops {
        fn new_rail(&self, dw: &Rc<DynamicWorld>, pos: IntPair, ty: i32, owned: bool) -> Option<ObjRef> {
            Rail::new(&dw.world.upgrade().unwrap(), dw, pos, None, ty, owned)
        }
        fn new_train_car(
            &self,
            dw: &Rc<DynamicWorld>,
            item_type: i32,
            pos: IntPair,
            sd: Option<&PlistDict>,
            cid: Option<&str>,
        ) -> Option<ObjRef> {
            let w = dw.world.upgrade().unwrap();
            match item_type {
                204 => HandCar::new(&w, dw, pos, None, sd, cid),
                205 => SteamTrain::new(&w, dw, pos, None, sd, cid),
                206 => FreightCar::new(&w, dw, pos, None, sd, cid),
                208 => PassengerCar::new(&w, dw, pos, None, sd, cid),
                _ => None,
            }
        }
        fn train_car_rail_or_station_name_changed(&self, car: &ObjRef) {
            if let Some(t) = train_car_of(car) {
                t.rail_or_station_name_changed();
            }
        }
        fn tap_is_within_body_radius(&self, o: &ObjRef, tap: Vector2) -> bool {
            train_car_of(o).map_or(false, |t| t.tap_is_within_body_radius(tap))
        }
    }

    pub(crate) fn setup(name: &str) -> (Rc<World>, Rc<DynamicWorld>) {
        setup_wide(name, 2)
    }

    /// `Loader::tile_is_protected_at_pos_against_blockhead` for any tile.
    pub(super) fn protected_nowhere(world: &World, b: &ObjRef) -> bool {
        Loader.tile_is_protected_at_pos_against_blockhead(world, IntPair { x: 0, y: 0 }, b)
    }

    /// Like [`setup`] with a world `macro_width` macro tiles wide.
    pub(crate) fn setup_wide(name: &str, macro_width: i32) -> (Rc<World>, Rc<DynamicWorld>) {
        let dw = test_dw(name, macro_width);
        let world = dw.world.upgrade().unwrap();
        world.set_ops_hooks(Some(Rc::new(Loader)));
        set_ops_hooks(Rc::new(Ops));
        (world, dw)
    }

    /// Rails on `x0..=x1` at y = 10 (the rail tile holds foreground contents 98 and a Rail object).
    pub(crate) fn lay_rails(world: &Rc<World>, dw: &Rc<DynamicWorld>, x0: i32, x1: i32) {
        for x in x0..=x1 {
            tile_at_world_position_loaded(x, 10, world).unwrap().set_foreground_contents(CONTENTS_RAIL);
        }
        for x in x0..=x1 {
            dw.add_rail_at_pos_of_type_owned_by_station(IntPair { x, y: 10 }, ITEM_RAIL, false);
        }
    }

    pub(crate) fn find_car(dw: &Rc<DynamicWorld>, ty: i32) -> ObjRef {
        for id in 0..1024u64 {
            if let Some(c) = dw.train_car_with_id(id) {
                if c.object_type() == ty {
                    return c;
                }
            }
        }
        panic!("car of type {ty} not found");
    }

    /// : flat line -> every rail FLAT; the net image is 0x20 bytes.
    #[test]
    fn rails_on_a_flat_line() {
        let (world, dw) = setup("trains_rails");
        lay_rails(&world, &dw, 5, 14);
        for x in 5..=14 {
            let r = dw.get_rail_at_pos(IntPair { x, y: 10 }).expect("rail");
            assert_eq!(rail_current_configuration(&r), RAIL_FLAT, "x={x}");
            assert_eq!(r.creation_net_data_for_client(&r, None).unwrap().len(), 0x20);
        }
        // a step up: a solid tile right of the last rail (rail on its left) gives HALF_UP_RIGHT_TOP after a world change
        tile_at_world_position_loaded(15, 10, &world).unwrap().set_type_index(1);
        let r = dw.get_rail_at_pos(IntPair { x: 14, y: 10 }).unwrap();
        r.world_changed(&r, &mut vec![IntPair { x: 15, y: 10 }]);
        assert_eq!(rail_current_configuration(&r), RAIL_DIAGONAL_HALF_UP_RIGHT_TOP);
        // ... and DIAGONAL_UP_RIGHT when there is no rail on its left
        tile_at_world_position_loaded(13, 10, &world).unwrap().set_foreground_contents(0);
        r.world_changed(&r, &mut vec![IntPair { x: 13, y: 10 }]);
        assert_eq!(rail_current_configuration(&r), RAIL_DIAGONAL_UP_RIGHT);
    }

    /// : placement, wheel layout, a motionless car stays put.
    #[test]
    fn hand_car_placement_and_rest() {
        let (world, dw) = setup("trains_hand");
        lay_rails(&world, &dw, 5, 14);
        dw.place_train_car_at_pos_of_type_save_dict_placed_by_client(IntPair { x: 8, y: 10 }, 204, None, None);
        let car = find_car(&dw, 41);
        assert_eq!(car.float_pos(), Vector2 { x: 8.5, y: 10.0 });
        let t = train_car_of(&car).unwrap();
        assert_eq!(t.left_wheel_pos(), Vector2 { x: 8.0, y: 10.0 });
        assert_eq!(t.right_wheel_pos(), Vector2 { x: 9.0, y: 10.0 });
        for _ in 0..120 {
            car.update_accurate_dt_is_simulation(&car, 0.016, 0.016, false);
        }
        assert_eq!(car.float_pos(), Vector2 { x: 8.5, y: 10.0 });
        assert!(!car.needs_removed());
        // HandCar has no coupling and a riderless hit test of the base class
        assert!(!t.connects_to_other_cars());
        assert!(t.tap_is_within_body_radius(Vector2 { x: 8.5, y: 11.0 }));
        assert!(!t.tap_is_within_body_radius(Vector2 { x: 9.4, y: 11.0 }));
        // net image: 0x68 struct + gzip tail
        let nd = car.creation_net_data_for_client(&car, None).unwrap();
        assert!(nd.len() > 0x68);
        let s = TrainCarCreationNetData::read_le(&nd);
        assert_eq!(s.dynamic_object_net_data.unique_id, car.unique_id());
        assert_eq!((s.left_wheel_pos_x, s.right_wheel_pos_x), (8.0, 9.0));
        let tail = crate::foundation::gzip::gzip_inflate(&nd[0x68..]).unwrap();
        assert!(crate::plist::helpers::property_list_from_data(&tail).is_some());
    }

    /// : Steam + Passenger + Freight coupling, save dict keys, reload.
    #[test]
    fn consist_coupling_and_save_round_trip() {
        let (world, dw) = setup("trains_consist");
        lay_rails(&world, &dw, 3, 20);
        // cars sit in `dynamicObjectsToAdd` until the next DynamicWorld tick; `checkForTrainCarUnderTap:` only sees
        // `dynamicObjects`, so move them by hand like the tick does
        let promote = |ty: i32| {
            let c = find_car(&dw, ty);
            dw.do_set(ty as usize, c.unique_id(), c.clone());
            c
        };
        dw.place_train_car_at_pos_of_type_save_dict_placed_by_client(IntPair { x: 8, y: 10 }, 205, None, None);
        let steam = promote(42);
        dw.place_train_car_at_pos_of_type_save_dict_placed_by_client(IntPair { x: 9, y: 10 }, 208, None, None);
        let pass = promote(44);
        // the placement slid the passenger car to the far side of the engine
        assert_eq!(pass.pos(), IntPair { x: 10, y: 10 });
        let (st, pt) = (train_car_of(&steam).unwrap(), train_car_of(&pass).unwrap());
        assert!(ptr_eq(&pt.left_car().unwrap(), &steam));
        assert!(ptr_eq(&st.right_car().unwrap(), &pass));
        assert!(ptr_eq(&pt.engine_car(&pass).unwrap(), &steam));
        assert!(!pt.tc().borrow().engine_is_right);
        // a freight car on the far side adopts the engine through the passenger car
        dw.place_train_car_at_pos_of_type_save_dict_placed_by_client(IntPair { x: 11, y: 10 }, 206, None, None);
        let freight = promote(43);
        let ft = train_car_of(&freight).unwrap();
        assert!(ft.tc().borrow().engine_car.as_ref().map_or(false, |e| ptr_eq(e, &steam)));
        // save dict keys and number kinds
        let d = pass.get_save_dict(&pass);
        assert_eq!(d.map.get(&"leftCarID".to_string()).unwrap().int_value() as u64, steam.unique_id());
        assert_eq!(d.map.get(&"engineCarID".to_string()).unwrap().int_value() as u64, steam.unique_id());
        assert!(matches!(d.map.get(&"engineIsRight".to_string()), Some(PlistValue::Bool(false))));
        let sd = steam.get_save_dict(&steam);
        assert!(matches!(sd.map.get(&"fuelFraction".to_string()), Some(PlistValue::Real { is_float: true, .. })));
        assert!(matches!(sd.map.get(&"hasFuel".to_string()), Some(PlistValue::Bool(true))));
        assert_eq!(sd.map.get(&"rightCarID".to_string()).unwrap().int_value() as u64, pass.unique_id());
        // ticks keep the consist stable and the net image of the steam train is 0x88 + tail
        for _ in 0..60 {
            for c in [&steam, &pass, &freight] {
                c.update_accurate_dt_is_simulation(c, 0.016, 0.016, false);
            }
        }
        assert!(!steam.needs_removed() && !pass.needs_removed() && !freight.needs_removed());
        let nd = steam.creation_net_data_for_client(&steam, None).unwrap();
        assert!(nd.len() > 0x88);
        let s = crate::types::netdata::SteamTrainCreationNetData::read_le(&nd);
        assert_eq!(s.fuel_fraction, 255);
        assert_eq!(s.has_fuel, 1);
        assert_eq!(s.train_car_creation_net_data.right_car_id, pass.unique_id());
        // reload the passenger car from its save dict: ids come back as pending links, resolved by the first update
        let w2 = world.clone();
        let again = PassengerCar::from_save_dict(&w2, &dw, &d, None).unwrap();
        let at = train_car_of(&again).unwrap();
        assert_eq!(at.tc().borrow().remote_left_car_id, steam.unique_id());
        assert_eq!(at.tc().borrow().remote_engine_car_id, steam.unique_id());
        assert!(at.tc().borrow().left_car.is_none());
        assert_eq!(again.unique_id(), pass.unique_id());
    }

    /// : removing the rail under a parked car turns it into a free block.
    #[test]
    fn car_drops_when_rail_vanishes() {
        let (world, dw) = setup("trains_drop");
        lay_rails(&world, &dw, 5, 14);
        dw.place_train_car_at_pos_of_type_save_dict_placed_by_client(IntPair { x: 8, y: 10 }, 204, None, None);
        let car = find_car(&dw, 41);
        // an unrelated change does nothing
        car.world_changed(&car, &mut vec![IntPair { x: 3, y: 3 }]);
        assert!(!car.needs_removed());
        // rail tile still there: kept
        car.world_changed(&car, &mut vec![IntPair { x: 8, y: 10 }]);
        assert!(!car.needs_removed());
        tile_at_world_position_loaded(8, 10, &world).unwrap().set_foreground_contents(0);
        car.world_changed(&car, &mut vec![IntPair { x: 8, y: 10 }]);
        assert!(car.needs_removed());
        assert!(car.update_needs_to_be_sent());
    }

    /// : the rail-configuration `dy` / wheel offset table of `loadDerivedStuff`.
    #[test]
    fn load_derived_stuff_rail_table() {
        let (world, dw) = setup("trains_table");
        lay_rails(&world, &dw, 5, 14);
        let rail = dw.get_rail_at_pos(IntPair { x: 8, y: 10 }).unwrap();
        let rows: [(i32, f32, (f32, f32), (f32, f32)); 7] = [
            (8, 0.5, (-0.5, 0.0), (0.5, 0.0)),
            (2, 0.5, (-0.5, 0.5), (0.5, -0.5)),
            (3, 0.25, (-0.5, 0.25), (0.5, -0.25)),
            (4, 0.75, (-0.5, 0.25), (0.5, -0.25)),
            (5, 0.5, (-0.5, -0.5), (0.5, 0.5)),
            (6, 0.25, (-0.5, -0.25), (0.5, 0.25)),
            (7, 0.75, (-0.5, -0.25), (0.5, 0.25)),
        ];
        for (cfg, dy, l, r) in rows {
            crate::objects::handle::downcast::<Rail>(&rail).unwrap().st.borrow_mut().current_configuration = cfg;
            for (ty, w) in [(41, 0), (43, 1), (44, 1), (42, 1)] {
                let car: ObjRef = match ty {
                    41 => HandCar::new(&world, &dw, IntPair { x: 8, y: 10 }, None, None, None),
                    42 => SteamTrain::new(&world, &dw, IntPair { x: 8, y: 10 }, None, None, None),
                    43 => FreightCar::new(&world, &dw, IntPair { x: 8, y: 10 }, None, None, None),
                    _ => PassengerCar::new(&world, &dw, IntPair { x: 8, y: 10 }, None, None, None),
                }
                .unwrap();
                let fp = car.float_pos();
                assert_eq!(fp, Vector2 { x: 8.5, y: 10.0 + dy }, "cfg {cfg} type {ty}");
                let t = train_car_of(&car).unwrap();
                if w == 0 {
                    assert_eq!(t.left_wheel_pos(), Vector2 { x: fp.x + l.0, y: fp.y + l.1 }, "cfg {cfg}");
                    assert_eq!(t.right_wheel_pos(), Vector2 { x: fp.x + r.0, y: fp.y + r.1 }, "cfg {cfg}");
                } else {
                    // Steam / Freight / Passenger overwrite the wheels with floatPos +- (1, 0) (R9)
                    assert_eq!(t.left_wheel_pos(), Vector2 { x: fp.x - 1.0, y: fp.y }, "cfg {cfg} type {ty}");
                    assert_eq!(t.right_wheel_pos(), Vector2 { x: fp.x + 1.0, y: fp.y }, "cfg {cfg} type {ty}");
                }
            }
        }
    }

    /// : `TrainCarCreationNetData` offsets and zero padding.
    #[test]
    fn creation_net_data_layout() {
        let mut d = TrainCarCreationNetData::default();
        d.rider0_id = 1;
        d.rider1_id = 2;
        d.left_car_id = 3;
        d.right_car_id = 4;
        d.engine_car_id = 5;
        d.left_wheel_pos_x = 1.5;
        d.right_wheel_vel_y = -2.5;
        d.visual_velocity_x = 7.0;
        d.engine_is_right = 1;
        let b = d.to_vec();
        assert_eq!(b.len(), 0x68);
        for (off, v) in [(0x18usize, 1u64), (0x20, 2), (0x28, 3), (0x30, 4), (0x38, 5)] {
            assert_eq!(&b[off..off + 8], &v.to_le_bytes());
        }
        assert_eq!(&b[0x40..0x44], &1.5f32.to_le_bytes());
        assert_eq!(&b[0x5c..0x60], &(-2.5f32).to_le_bytes());
        assert_eq!(&b[0x60..0x64], &7.0f32.to_le_bytes());
        assert_eq!(b[0x64], 1);
        assert!(b[0x65..].iter().all(|&x| x == 0));
        assert!(b[0x11..0x18].iter().all(|&x| x == 0));
    }

    /// Passenger / Freight physics on a sloped rail: the wheels follow the rail surface.
    #[test]
    fn passenger_car_follows_the_rail() {
        let (world, dw) = setup("trains_slope");
        lay_rails(&world, &dw, 5, 14);
        dw.place_train_car_at_pos_of_type_save_dict_placed_by_client(IntPair { x: 9, y: 10 }, 208, None, None);
        let car = find_car(&dw, 44);
        for _ in 0..300 {
            car.update_accurate_dt_is_simulation(&car, 0.016, 0.016, false);
        }
        let t = train_car_of(&car).unwrap();
        // flat rail, no engine: the car stays on the rail line at height 10 and never derails
        assert!(!car.needs_removed());
        assert_eq!(t.left_wheel_pos().y, 10.0);
        assert_eq!(t.right_wheel_pos().y, 10.0);
        // remove the ground under the whole line: the wheels fall (acc -2) and the car eventually derails or keeps falling
        for x in 4..=15 {
            tile_at_world_position_loaded(x, 10, &world).unwrap().set_foreground_contents(0);
        }
        for _ in 0..30 {
            car.update_accurate_dt_is_simulation(&car, 0.016, 0.016, false);
        }
        assert!(t.left_wheel_pos().y < 10.0);
    }
}

#[cfg(test)]
mod car_tests {
    //! The shared TrainCar machinery through real cars: riders and their persistence, remote updates, links,
    //! placement probing and the protocol faces.
    use super::world_tests::{find_car, lay_rails, setup};
    use super::*;
    use crate::blockhead::blockhead::testutil::{bh_of, new_bh, TestHost};
    use crate::objects::base::DynamicObjectBase;
    use crate::objects::base::DynamicWorldRef;
    use crate::objects::class_info::DYNAMIC_OBJECT_CLASS;
    use crate::trains::freight_car::FreightCar;
    use crate::trains::hand_car::HandCar;
    use crate::trains::passenger_car::PassengerCar;
    use crate::trains::steam_train::SteamTrain;
    use std::rc::Weak;

    /// A car class that overrides nothing, to run the trait's default methods.
    struct BareCar {
        base: RefCell<DynamicObjectBase>,
        tc: RefCell<TrainCarState>,
        world: Weak<World>,
        dw: Weak<DynamicWorld>,
        engine: std::cell::Cell<bool>,
    }
    impl DynamicObject for BareCar {
        fn base(&self) -> &RefCell<DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static ClassInfo {
            &DYNAMIC_OBJECT_CLASS
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn object_type(&self) -> i32 {
            DOT_TRAIN_CAR_HANDCAR
        }
    }
    impl TrainCarLike for BareCar {
        // the one override: lets a single class run both branches of the defaults that read it
        fn is_engine(&self) -> bool {
            self.engine.get()
        }
        fn tc(&self) -> &RefCell<TrainCarState> {
            &self.tc
        }
        fn tc_world(&self) -> Rc<World> {
            self.world.upgrade().unwrap()
        }
        fn tc_dw(&self) -> Rc<DynamicWorld> {
            self.dw.upgrade().unwrap()
        }
    }

    fn place(dw: &Rc<DynamicWorld>, x: i32, item: i32, ty: usize) -> ObjRef {
        dw.place_train_car_at_pos_of_type_save_dict_placed_by_client(IntPair { x, y: 10 }, item, None, None);
        let c = find_car(dw, ty as i32);
        dw.do_set(ty, c.unique_id(), c.clone());
        c
    }

    fn bh(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> ObjRef {
        *world.distance_ordered_food_types.borrow_mut() = (0..31).collect();
        new_bh(world, &(dw.clone() as DynamicWorldRef), 0)
    }

    fn tc_of(o: &ObjRef) -> &RefCell<TrainCarState> {
        train_car_of(o).unwrap().tc()
    }

    /// The defaults of the trait (what a class gets that overrides nothing).
    #[test]
    fn trait_defaults() {
        let _h = TestHost::install();
        let (world, dw) = setup("tc_defaults");
        lay_rails(&world, &dw, 3, 40);
        let hand = place(&dw, 20, 204, 41);
        let me = hand.clone();
        let bare = crate::objects::handle::alloc_obj(|w| BareCar {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            tc: RefCell::new(TrainCarState::alloc_zeroed()),
            world: Rc::downgrade(&world),
            dw: Rc::downgrade(&dw),
            engine: std::cell::Cell::new(false),
        });
        assert!(!bare.requires_fuel());
        // the stand-in's own accessors
        let bare_obj: ObjRef = bare.clone();
        assert_eq!(bare_obj.class_info().name, "DynamicObject");
        assert!(bare_obj.as_any().is::<BareCar>());
        assert_eq!(bare_obj.object_type(), DOT_TRAIN_CAR_HANDCAR);
        assert!(bare.tc().borrow().riders[0].is_none());
        // engineCar: itself when it is the engine, else the ivar
        assert!(bare.engine_car(&bare_obj).is_none());
        bare.engine.set(true);
        assert!(bare.engine_car(&bare_obj).is_some());
        bare.engine.set(false);
        assert_eq!(bare_obj.pos(), bare_obj.base().borrow().pos);
        let _ = (bare.tc_world(), bare.tc_dw());
        assert_eq!(bare.item_type(), 0);
        assert_eq!(bare.action_title(), "RIDE");
        assert_eq!(bare.rider_pos_for_blockhead(&me), [1.0; 4]);
        bare.set_target_velocity(Vector2 { x: 1.0, y: 1.0 });
        bare.rail_or_station_name_changed();
        // load_derived_stuff / addRider: / removeRider: act on the car they are given
        bare.load_derived_stuff(&me);
        let b = bh(&world, &dw);
        bare.add_rider(&me, &b);
        assert!(tc_of(&me).borrow().riders[0].is_some());
        bare.remove_rider(&me, &b);
        assert!(tc_of(&me).borrow().riders[0].is_none());
        // the creation image names a second rider and every link that is set
        {
            let mut st = tc_of(&me).borrow_mut();
            st.riders[1] = Some(b.clone());
            st.left_car = Some(bare_obj.clone());
            st.engine_car = Some(bare_obj.clone());
        }
        let cd = train_car_train_car_creation_net_data(&me);
        assert_eq!(cd.rider1_id, b.unique_id());
        assert_eq!(cd.left_car_id, bare_obj.unique_id());
        assert_eq!(cd.engine_car_id, bare_obj.unique_id());
        {
            let mut st = tc_of(&me).borrow_mut();
            st.riders[1] = None;
            st.left_car = None;
            st.engine_car = None;
        }
    }

    /// The Ridable / TapChoice faces of every car class.
    #[test]
    fn protocol_faces() {
        let _h = TestHost::install();
        let (world, dw) = setup("tc_faces");
        lay_rails(&world, &dw, 3, 60);
        let cars = [place(&dw, 10, 204, 41), place(&dw, 20, 205, 42), place(&dw, 30, 206, 43), place(&dw, 40, 208, 44)];
        let b = bh(&world, &dw);
        for c in &cars {
            let r = c.as_ridable().unwrap();
            let _ = r.rider_pos_for_blockhead(&b);
            let _ = r.camera_pos_for_blockhead(&b);
            assert_eq!(r.rider_body_y_rotation_for_blockhead(&b), 0.0);
            r.set_target_velocity(Vector2 { x: 0.0, y: 0.0 });
            assert_eq!(r.ride_direction(), 0);
            r.add_rider(c, &b);
            r.remove_rider(c, &b);
            r.swipe_up_gesture(c);
            let _ = r.blockhead_can_ride_using_item(&b, 0);
            assert!(!r.jumps_on_swipe());
            let _ = r.requires_fuel();
            let _ = r.acts_as_interaction_object();
            assert!(!r.rider_d_pad_should_allow_up_down());
            assert!(!r.rider_d_pad_should_give_discrete_values());
            let t = c.as_tap_choice().unwrap();
            assert!(!t.action_title().is_empty());
            assert!(!t.is_double_height());
            let _ = train_car_render_pos(c);
            let t = train_car_of(c).unwrap();
            assert!(t.engine_car(c).is_some() == t.is_engine());
        }
        assert!(cars[1].as_ridable().unwrap().requires_fuel());
        assert!(!cars[0].as_ridable().unwrap().requires_fuel());
    }

    /// Owner ids: from the placing client, from the save dict, or none.
    #[test]
    fn owner_of_a_placed_car() {
        let (world, dw) = setup("tc_owner");
        lay_rails(&world, &dw, 3, 40);
        let mut sd = new_mutable_dict();
        sd.map.insert("ownerID".to_string(), PlistValue::string("fromdict"));
        let a = HandCar::new(&world, &dw, IntPair { x: 10, y: 10 }, None, Some(&sd), None).unwrap();
        assert_eq!(a.owner_id().as_deref(), Some("fromdict"));
        let b = HandCar::new(&world, &dw, IntPair { x: 12, y: 10 }, None, Some(&sd), Some("client")).unwrap();
        assert_eq!(b.owner_id().as_deref(), Some("client"));
        let c = HandCar::new(&world, &dw, IntPair { x: 14, y: 10 }, None, None, None).unwrap();
        assert_eq!(c.owner_id(), None);
    }

    /// Riders survive a save: the dict stores their index in the blockhead list, the loaded car takes them back
    /// in `blockheadsLoaded` (and lets go of ones the server controls).
    #[test]
    fn riders_persist_through_a_save() {
        let _h = TestHost::install();
        let (world, dw) = setup("tc_riders");
        lay_rails(&world, &dw, 3, 40);
        let car = place(&dw, 20, 208, 44);
        let (a, b, other) = (bh(&world, &dw), bh(&world, &dw), bh(&world, &dw));
        // `other` comes first in the list so the saved indices are 1 and 2; a vanished one is not recorded
        dw.blockheads.borrow_mut().extend([other.clone(), a.clone(), b.clone()]);
        train_car_add_rider(&car, &a);
        train_car_add_rider(&car, &b);
        let sd = car.get_save_dict(&car);
        assert_eq!(sd.map.get(&"currentBlockheadIndex_0".to_string()).unwrap().int_value(), 1);
        assert_eq!(sd.map.get(&"currentBlockheadIndex_1".to_string()).unwrap().int_value(), 2);
        b.base().borrow_mut().needs_removed = true;
        assert!(!car.get_save_dict(&car).map.contains_key(&"currentBlockheadIndex_1".to_string()));
        b.base().borrow_mut().needs_removed = false;
        // a car loaded from that dict takes the riders back once the blockheads exist
        let loaded = PassengerCar::from_save_dict(&world, &dw, &sd, None).unwrap();
        assert_eq!(tc_of(&loaded).borrow().saved_blockhead_index, [1, 2]);
        loaded.blockheads_loaded(&loaded);
        assert!(tc_of(&loaded).borrow().riders[0].as_ref().is_some_and(|r| ptr_eq(r, &a)));
        assert!(bh_of(&a).bh.borrow().ride_object.is_some());
        assert_eq!(tc_of(&loaded).borrow().saved_blockhead_index, [-1, -1]);
        // an index past the list is ignored
        let mut far = sd.clone();
        far.map.insert("currentBlockheadIndex_0".to_string(), PlistValue::int(9));
        let nobody = PassengerCar::from_save_dict(&world, &dw, &far, None).unwrap();
        nobody.blockheads_loaded(&nobody);
        assert!(tc_of(&nobody).borrow().riders[0].is_none());
        // a rider controlled by the server is let go again
        bh_of(&a).bh.borrow_mut().is_client_blockhead_being_controlled_by_server = true;
        let again = PassengerCar::from_save_dict(&world, &dw, &sd, None).unwrap();
        again.blockheads_loaded(&again);
        assert!(tc_of(&again).borrow().riders[0].is_none());
    }

    /// `addRider:` guards: a server-controlled blockhead is turned away (and leaves a seat it held), a seated
    /// blockhead is not seated twice, a full car takes nobody.
    #[test]
    fn add_rider_guards() {
        let _h = TestHost::install();
        let (world, dw) = setup("tc_addrider");
        lay_rails(&world, &dw, 3, 40);
        let car = place(&dw, 20, 204, 41);
        let (a, b) = (bh(&world, &dw), bh(&world, &dw));
        train_car_add_rider(&car, &a);
        train_car_add_rider(&car, &a);
        assert!(tc_of(&car).borrow().riders[0].is_some());
        // full: the hand car has one seat
        train_car_add_rider(&car, &b);
        assert!(tc_of(&car).borrow().riders[0].as_ref().is_some_and(|r| ptr_eq(r, &a)));
        // the seated one becomes server-controlled: adding it again vacates the seat
        bh_of(&a).bh.borrow_mut().is_client_blockhead_being_controlled_by_server = true;
        car.set_update_needs_to_be_sent(false);
        train_car_add_rider(&car, &a);
        assert!(tc_of(&car).borrow().riders[0].is_none());
        assert!(car.update_needs_to_be_sent());
        // another controlled blockhead just bounces
        bh_of(&b).bh.borrow_mut().is_client_blockhead_being_controlled_by_server = true;
        train_car_add_rider(&car, &b);
        assert!(tc_of(&car).borrow().riders[0].is_none());
        // a rider that is not on the car is a no-op for removal; a loaded rider list leaves with the blockhead
        train_car_remove_rider(&car, &b);
        let c = bh(&world, &dw);
        train_car_add_rider(&car, &c);
        train_car_blockhead_unloaded(&car, &b);
        assert!(tc_of(&car).borrow().riders[0].is_some());
        train_car_blockhead_unloaded(&car, &c);
        assert!(tc_of(&car).borrow().riders[0].is_none());
        // a ridden car does not fall off its rail
        train_car_add_rider(&car, &c);
        tile_at_world_position_loaded(20, 10, &world).unwrap().set_foreground_contents(0);
        car.world_changed(&car, &mut vec![IntPair { x: 20, y: 10 }]);
        assert!(!car.needs_removed());
    }

    /// `remoteUpdate:` hands seats to the right riders and rejects strangers.
    #[test]
    fn remote_update_seats() {
        let _h = TestHost::install();
        let (world, dw) = setup("tc_remote");
        lay_rails(&world, &dw, 3, 40);
        let car = place(&dw, 20, 208, 44);
        let img = |car: &ObjRef, r0: u64, r1: u64| {
            let mut d = train_car_train_car_creation_net_data(car);
            d.rider0_id = r0;
            d.rider1_id = r1;
            d.to_vec()
        };
        // a local rider stays unless the update names someone else
        let local = bh(&world, &dw);
        train_car_add_rider(&car, &local);
        car.remote_update(&car, &img(&car, local.unique_id(), 0));
        assert!(tc_of(&car).borrow().riders[0].is_some());
        car.remote_update(&car, &img(&car, 12345, 0));
        assert!(tc_of(&car).borrow().riders[0].is_none());
        // a remote rider joins when it is a known net blockhead, leaves on id 0
        let net = bh(&world, &dw);
        net.base().borrow_mut().is_net = true;
        dw.net_blockheads.borrow_mut().push(net.clone());
        car.remote_update(&car, &img(&car, net.unique_id(), 0));
        assert!(tc_of(&car).borrow().riders[0].as_ref().is_some_and(|r| ptr_eq(r, &net)));
        car.remote_update(&car, &img(&car, net.unique_id(), 0));
        car.remote_update(&car, &img(&car, 0, 0));
        assert!(tc_of(&car).borrow().riders[0].is_none());
        // an unknown remote id seats nobody; an update for a removed car does nothing
        car.remote_update(&car, &img(&car, 999_999, 0));
        assert!(tc_of(&car).borrow().riders[0].is_none());
        car.base().borrow_mut().needs_removed = true;
        car.remote_update(&car, &img(&car, net.unique_id(), 0));
        assert!(tc_of(&car).borrow().riders[0].is_none());
    }

    /// A replica follows the link ids of the image.
    #[test]
    fn replica_follows_link_ids() {
        let (world, dw) = setup("tc_links");
        lay_rails(&world, &dw, 3, 60);
        let a = place(&dw, 10, 208, 44);
        let b = place(&dw, 30, 206, 43);
        let eng = place(&dw, 45, 205, 42);
        let rep =
            PassengerCar::from_net_data(&world, &dw, None, &a.creation_net_data_for_client(&a, None).unwrap()).unwrap();
        let img = |right: u64, left: u64, engine: u64| {
            let mut d = train_car_train_car_creation_net_data(&a);
            d.right_car_id = right;
            d.left_car_id = left;
            d.engine_car_id = engine;
            d.engine_is_right = 1;
            d.to_vec()
        };
        // known cars are linked at once
        rep.remote_update(&rep, &img(b.unique_id(), eng.unique_id(), eng.unique_id()));
        assert!(tc_of(&rep).borrow().right_car.as_ref().is_some_and(|c| ptr_eq(c, &b)));
        assert!(tc_of(&rep).borrow().engine_is_right);
        // the same ids again keep the links
        rep.remote_update(&rep, &img(b.unique_id(), eng.unique_id(), eng.unique_id()));
        // unknown ids are remembered for later
        rep.remote_update(&rep, &img(777, 778, 779));
        {
            let s = tc_of(&rep).borrow();
            assert_eq!((s.remote_right_car_id, s.remote_left_car_id, s.remote_engine_car_id), (777, 778, 779));
            assert!(s.right_car.is_none() && s.left_car.is_none() && s.engine_car.is_none());
        }
        // id 0 clears
        rep.remote_update(&rep, &img(0, 0, 0));
        assert_eq!(tc_of(&rep).borrow().remote_right_car_id, 0);
        // the pending ids resolve (or are dropped) on the next update of a car that has no link yet
        {
            let mut s = tc_of(&rep).borrow_mut();
            s.remote_left_car_id = b.unique_id();
            s.remote_right_car_id = b.unique_id();
            s.remote_engine_car_id = eng.unique_id();
        }
        rep.update_accurate_dt_is_simulation(&rep, 0.016, 0.016, false);
        assert_eq!(tc_of(&rep).borrow().remote_left_car_id, 0);
        // pending ids with links already in place are simply cleared
        {
            let mut s = tc_of(&rep).borrow_mut();
            s.remote_left_car_id = 5;
            s.remote_right_car_id = 5;
            s.remote_engine_car_id = 5;
        }
        rep.update_accurate_dt_is_simulation(&rep, 0.016, 0.016, false);
        // an engine resolves nothing for itself
        let steam = SteamTrain::new(&world, &dw, IntPair { x: 50, y: 10 }, None, None, None).unwrap();
        tc_of(&steam).borrow_mut().remote_engine_car_id = 5;
        train_car_update_accurate_dt_is_simulation(&steam, 0.016, 0.016, false);
        assert_eq!(tc_of(&steam).borrow().remote_engine_car_id, 0);
    }

    /// Chains of cars: which side the engine is found on decides who loses it when a car goes.
    #[test]
    fn engine_search_walks_the_chain() {
        let (world, dw) = setup("tc_chain");
        lay_rails(&world, &dw, 3, 90);
        let mk = |x: i32, item: i32| -> ObjRef {
            let pos = IntPair { x, y: 10 };
            let c = match item {
                205 => SteamTrain::new(&world, &dw, pos, None, None, None),
                206 => FreightCar::new(&world, &dw, pos, None, None, None),
                _ => PassengerCar::new(&world, &dw, pos, None, None, None),
            }
            .unwrap();
            dw.do_set(c.object_type() as usize, c.unique_id(), c.clone());
            c
        };
        let link = |a: &ObjRef, b: &ObjRef| {
            tc_of(a).borrow_mut().right_car = Some(b.clone());
            tc_of(b).borrow_mut().left_car = Some(a.clone());
        };
        // [engine][a][b][c][d]: c's left neighbour b goes; the engine is two cars further left, so d loses it
        let e = mk(10, 205);
        let (a, b, c, d) = (mk(20, 208), mk(30, 206), mk(40, 208), mk(50, 208));
        link(&e, &a);
        link(&a, &b);
        link(&b, &c);
        link(&c, &d);
        train_car_of(&a).unwrap().set_engine_car(&a, Some(&e));
        assert!(!engine_none(&d));
        b.base().borrow_mut().needs_removed = true;
        c.update_accurate_dt_is_simulation(&c, 0.016, 0.016, false);
        assert!(tc_of(&c).borrow().left_car.is_none());
        assert!(engine_none(&d));
        // no engine anywhere: the walk just ends
        let (x, y, z) = (mk(60, 208), mk(70, 206), mk(80, 208));
        link(&x, &y);
        link(&y, &z);
        y.base().borrow_mut().needs_removed = true;
        z.update_accurate_dt_is_simulation(&z, 0.016, 0.016, false);
        assert!(tc_of(&z).borrow().left_car.is_none());
        // the mirror image on the right: [z2][a2][b2][c2][engine], b2 goes, a2 looks right and z2 loses the engine
        let (z2, a2, b2, c2) = (mk(12, 208), mk(22, 208), mk(32, 206), mk(42, 208));
        let e2 = mk(52, 205);
        link(&z2, &a2);
        link(&a2, &b2);
        link(&b2, &c2);
        link(&c2, &e2);
        train_car_of(&c2).unwrap().set_engine_car(&c2, Some(&e2));
        assert!(!engine_none(&z2));
        b2.base().borrow_mut().needs_removed = true;
        a2.update_accurate_dt_is_simulation(&a2, 0.016, 0.016, false);
        assert!(tc_of(&a2).borrow().right_car.is_none());
        assert!(engine_none(&z2));
        let (p, q, r) = (mk(62, 208), mk(72, 206), mk(82, 208));
        link(&p, &q);
        link(&q, &r);
        q.base().borrow_mut().needs_removed = true;
        p.update_accurate_dt_is_simulation(&p, 0.016, 0.016, false);
        assert!(tc_of(&p).borrow().right_car.is_none());
    }

    fn engine_none(c: &ObjRef) -> bool {
        train_car_of(c).unwrap().engine_car(c).is_none()
    }

    /// A wheel with the rail one tile above it, and an engine-side car off to one side pulling at an angle.
    #[test]
    fn rail_above_and_steep_coupling() {
        let (world, dw) = setup("tc_above");
        lay_rails(&world, &dw, 3, 60);
        let car = place(&dw, 20, 208, 44);
        // the rail under the left wheel moves up one tile: the wheel's tile position follows
        let t = tile_at_world_position_loaded(19, 10, &world).unwrap();
        t.set_foreground_contents(0);
        t.set_type_index(2);
        tile_at_world_position_loaded(19, 11, &world).unwrap().set_foreground_contents(CONTENTS_RAIL);
        car.update_accurate_dt_is_simulation(&car, 0.016, 0.016, false);
        assert_eq!(tc_of(&car).borrow().left_wheel_tile_pos, IntPair { x: 19, y: 11 });
        // engine on the right, its car high above: the pull is clamped to 0.5 in x
        let other = place(&dw, 40, 206, 43);
        let eng = place(&dw, 50, 205, 42);
        {
            let mut s = tc_of(&car).borrow_mut();
            s.right_car = Some(other.clone());
            s.engine_car = Some(eng.clone());
            s.engine_is_right = true;
        }
        tc_of(&other).borrow_mut().left_wheel_pos = Vector2 { x: 20.0, y: 30.0 };
        car.update_accurate_dt_is_simulation(&car, 0.016, 0.016, false);
        // and on the left
        {
            let mut s = tc_of(&car).borrow_mut();
            s.left_car = Some(other.clone());
            s.right_car = None;
            s.engine_is_right = false;
        }
        tc_of(&other).borrow_mut().right_wheel_pos = Vector2 { x: 20.0, y: 30.0 };
        car.update_accurate_dt_is_simulation(&car, 0.016, 0.016, false);
    }

    /// The probing of the tiles beside a new car on slopes and gaps.
    #[test]
    fn placement_probe_on_slopes_and_gaps() {
        let (world, dw) = setup("tc_probe");
        lay_rails(&world, &dw, 3, 60);
        let car = place(&dw, 30, 208, 44);
        let tile = |x: i32, y: i32| tile_at_world_position_loaded(x, y, &world).unwrap();
        let set = |x: i32, y: i32, fg: u8, ty: u8| {
            tile(x, y).set_foreground_contents(fg);
            tile(x, y).set_type_index(ty);
        };
        for offset in [-1i32, 1] {
            let n = 30 + offset;
            let nn = 30 + 2 * offset;
            // neighbour tile is air and the rail is one lower
            set(n, 10, 0, 2);
            set(n, 9, CONTENTS_RAIL, 2);
            set(nn, 10, 0, 2);
            set(nn, 9, CONTENTS_RAIL, 2);
            let _ = neighbour_car_for_placement(&car, offset);
            // ... and the second tile is solid: the rail would be one higher
            set(nn, 10, 0, 1);
            set(nn, 11, CONTENTS_RAIL, 2);
            let _ = neighbour_car_for_placement(&car, offset);
            // ... which is not there: no rail at all
            set(nn, 11, 0, 2);
            assert!(neighbour_car_for_placement(&car, offset).is_none());
            // neighbour tile solid, rail one higher, second tile missing the rail
            set(n, 10, 0, 1);
            set(n, 11, CONTENTS_RAIL, 2);
            set(nn, 10, 0, 2);
            set(nn, 9, 0, 2);
            assert!(neighbour_car_for_placement(&car, offset).is_none());
            // neighbour tile solid and no rail above it
            set(n, 11, 0, 2);
            assert!(neighbour_car_for_placement(&car, offset).is_none());
            // a rail beside the car, then a solid tile whose upper neighbour carries the rail
            set(n, 10, CONTENTS_RAIL, 2);
            set(nn, 10, 0, 1);
            set(nn, 11, CONTENTS_RAIL, 2);
            let _ = neighbour_car_for_placement(&car, offset);
            // ... and an open tile whose lower neighbour does
            set(nn, 10, 0, 2);
            set(nn, 11, 0, 2);
            set(nn, 9, CONTENTS_RAIL, 2);
            let _ = neighbour_car_for_placement(&car, offset);
        }
    }

    /// Cached wheel rails that were removed are forgotten; a rail tile without a rail object leaves the wheels flat.
    #[test]
    fn wheel_rails_and_bare_rail_tiles() {
        let (world, dw) = setup("tc_wheelrails");
        lay_rails(&world, &dw, 3, 38);
        let car = place(&dw, 20, 208, 44);
        car.update_accurate_dt_is_simulation(&car, 0.016, 0.016, false);
        assert!(tc_of(&car).borrow().left_wheel_rail.is_some());
        for x in 18..=23 {
            dw.get_rail_at_pos(IntPair { x, y: 10 }).unwrap().base().borrow_mut().needs_removed = true;
        }
        car.update_accurate_dt_is_simulation(&car, 0.016, 0.016, false);
        assert!(tc_of(&car).borrow().left_wheel_rail.is_none() && tc_of(&car).borrow().right_wheel_rail.is_none());
        // a rail tile no rail object stands on
        tile_at_world_position_loaded(50, 10, &world).unwrap().set_foreground_contents(CONTENTS_RAIL);
        let bare = HandCar::new(&world, &dw, IntPair { x: 50, y: 10 }, None, None, None).unwrap();
        assert_eq!(bare.float_pos(), Vector2 { x: 50.5, y: 10.0 });
    }

    /// Seat availability, the mirror of `worldChanged:` for replicas and removed cars, `blockheadUnloaded:` and
    /// `removeFromMacroBlock` through the object, and the owner tail of a bare image.
    #[test]
    fn seat_availability_and_object_hooks() {
        let _h = TestHost::install();
        let (world, dw) = setup("tc_hooks");
        lay_rails(&world, &dw, 3, 60);
        let car = place(&dw, 20, 204, 41);
        let b = bh(&world, &dw);
        let t = train_car_of(&car).unwrap();
        assert!(t.blockhead_can_ride_using_item(&car, &b, 0));
        train_car_add_rider(&car, &b);
        assert!(!t.blockhead_can_ride_using_item(&car, &b, 0));
        car.blockhead_unloaded(&car, &b);
        assert!(tc_of(&car).borrow().riders[0].is_none());
        // a replica never drops itself
        let nd = car.creation_net_data_for_client(&car, None).unwrap();
        let rep = HandCar::from_net_data(&world, &dw, None, &nd).unwrap();
        tile_at_world_position_loaded(20, 10, &world).unwrap().set_foreground_contents(0);
        rep.world_changed(&rep, &mut vec![IntPair { x: 20, y: 10 }]);
        assert!(!rep.needs_removed());
        // a head-only image has no owner tail
        let bare = PassengerCar::from_net_data(&world, &dw, None, &train_car_creation_net_data_for_client(&car));
        assert_eq!(bare.unwrap().owner_id(), None);
        // removing the car from its macro tile lets go of its links
        let other = place(&dw, 30, 208, 44);
        tc_of(&other).borrow_mut().left_car = Some(car.clone());
        other.remove_from_macro_block(&other);
        assert!(tc_of(&other).borrow().left_car.is_none());
        tc_of(&other).borrow_mut().right_car = Some(car.clone());
        train_car_of(&other).unwrap().cleanup(&other);
        assert!(tc_of(&other).borrow().right_car.is_none());
        let removed = place(&dw, 40, 208, 44);
        removed.base().borrow_mut().needs_removed = true;
        assert!(!train_car_of(&removed).unwrap().blockhead_can_ride_using_item(&removed, &b, 0));
        _use_traits(removed.as_ridable().unwrap(), removed.as_tap_choice().unwrap());
    }

    /// Placement does not couple to a hand car, and an existing neighbour with an engine hands it over.
    #[test]
    fn placement_coupling_skips_hand_cars() {
        let (world, dw) = setup("tc_couple");
        lay_rails(&world, &dw, 3, 60);
        let hand = place(&dw, 21, 204, 41);
        let pass = place(&dw, 20, 208, 44);
        assert!(tc_of(&pass).borrow().right_car.is_none());
        assert!(tc_of(&hand).borrow().left_car.is_none());
    }

    /// The scaffolding itself: an unknown item type builds nothing, a missing car is an error, nothing is protected.
    #[test]
    fn scaffolding_edges() {
        let (world, dw) = setup("tc_scaffold");
        lay_rails(&world, &dw, 3, 40);
        dw.place_train_car_at_pos_of_type_save_dict_placed_by_client(IntPair { x: 20, y: 10 }, 999, None, None);
        assert!(dw.train_car_with_id(2000).is_none());
        let b = bh(&world, &dw);
        assert!(!world_tests::protected_nowhere(&world, &b));
    }

    #[test]
    #[should_panic(expected = "car of type 41 not found")]
    fn find_car_panics_without_a_car() {
        let (_world, dw) = setup("tc_nofind");
        find_car(&dw, 41);
    }
}

#[cfg(test)]
mod class_chain_tests {
    use super::*;
    #[test]
    fn car_classes_inherit_train_car() {
        for c in [
            &crate::trains::hand_car::HAND_CAR_CLASS,
            &crate::trains::passenger_car::PASSENGER_CAR_CLASS,
            &crate::trains::freight_car::FREIGHT_CAR_CLASS,
            &crate::trains::steam_train::STEAM_TRAIN_CLASS,
        ] {
            assert!(std::ptr::eq(c.superclass.unwrap(), &TRAIN_CAR_CLASS));
        }
        assert_eq!(TRAIN_CAR_CLASS.superclass.unwrap().name, "DynamicObject");
    }
}
