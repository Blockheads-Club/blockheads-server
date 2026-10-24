//! `Rail` (DynamicObject type 40): one track tile whose shape is derived from its neighbours.
//!
//! and section 6 "Rail drop rule". Client-only parts (`draw:`, `staticGeometry*`, `addDrawCubeData:`,
//! `reloadDrawBlockDynamicObjectStaticGeometryForTile`) are not ported.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::dynworld::state::DynamicWorld;
use crate::objects::base::{
    base_dynamic_object_net_data, base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_cache_net_data, base_init_with_world_dynamic_world_save_dict_cache,
    padded_bytes, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::item_type::ItemType;
use crate::types::netdata::RailCreationNetData;
use crate::types::structs::{IntPair, WireStruct};
use crate::world::helpers::tile_is_solid;
use crate::world::state::{TileRef, World};
use crate::world::tiles::{tile_at_world_position, tile_at_world_position_loaded};

/// `DYNAMIC_OBJECT_TYPE_RAIL`.
pub const DOT_RAIL: i32 = 40;
/// `CONTENTS_RAIL` ('b') in `Tile.foregroundContents`.
pub const CONTENTS_RAIL: u8 = 98;
/// `CONTENTS_COLUMN` in `Tile.foregroundContents`.
pub const CONTENTS_COLUMN: u8 = 100;

/// `RailConfiguration` values.
pub const RAIL_UNDEFINED: i32 = 0;
pub const RAIL_FLAT: i32 = 1;
pub const RAIL_DIAGONAL_UP_LEFT: i32 = 2;
pub const RAIL_DIAGONAL_HALF_UP_LEFT_BOT: i32 = 3;
pub const RAIL_DIAGONAL_HALF_UP_LEFT_TOP: i32 = 4;
pub const RAIL_DIAGONAL_UP_RIGHT: i32 = 5;
pub const RAIL_DIAGONAL_HALF_UP_RIGHT_BOT: i32 = 6;
pub const RAIL_DIAGONAL_HALF_UP_RIGHT_TOP: i32 = 7;
pub const RAIL_DIAGONAL_HALF_FLAT: i32 = 8;

/// `Rail` class descriptor.
pub static RAIL_CLASS: ClassInfo = ClassInfo { name: "Rail", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &[] };

/// `drawPoles = (configuration in {7, 3, 8, 2, 5})`.
pub fn draw_poles_for_configuration(cfg: i32) -> bool {
    matches!(cfg, 7 | 3 | 8 | 2 | 5)
}

/// The two facts `updateRailConfiguration` reads from a probed tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RailProbe {
    /// `Tile.foregroundContents`
    pub fg: u8,
    /// `tileIsSolid(tile)`
    pub solid: bool,
}

/// Pure core of `-[Rail updateRailConfiguration]`: `probe(dx, dy)` is `T(dx, dy)` (`None` = NULL tile).
/// Starts from `current` (it stays when the right neighbour is NULL). The left pass uses the STALE right
/// neighbour fetched at the top (deliberate, note).
pub fn compute_rail_configuration(current: i32, probe: &dyn Fn(i32, i32) -> Option<RailProbe>) -> i32 {
    let is_rail = |t: Option<RailProbe>| t.is_some_and(|t| t.fg == CONTENTS_RAIL);
    // `s = tileIsSolid(t); if !s { u = T(dx, 1); s = (u is a rail) }`
    let up = |t: RailProbe, dx: i32| -> bool {
        let mut s = t.solid;
        if !s {
            s = is_rail(probe(dx, 1));
        }
        s
    };
    let mut new = current;
    let r1 = probe(1, 0);
    if let Some(r1t) = r1 {
        if r1t.fg == CONTENTS_RAIL {
            match probe(2, 0) {
                None => new = RAIL_FLAT,
                Some(r2) if r2.fg == CONTENTS_RAIL => new = RAIL_FLAT,
                Some(r2) => {
                    new = if up(r2, 2) { RAIL_DIAGONAL_HALF_UP_RIGHT_BOT } else { RAIL_FLAT };
                }
            }
        } else if up(r1t, 1) {
            let l = probe(-1, 0);
            new = if l.is_none_or(|l| l.fg != CONTENTS_RAIL) {
                RAIL_DIAGONAL_UP_RIGHT
            } else {
                RAIL_DIAGONAL_HALF_UP_RIGHT_TOP
            };
        } else {
            new = RAIL_FLAT;
        }
    }
    if new == RAIL_FLAT || new == RAIL_DIAGONAL_HALF_UP_RIGHT_BOT {
        if let Some(l) = probe(-1, 0) {
            if l.fg == CONTENTS_RAIL {
                match probe(-2, 0) {
                    None => {
                        if new != RAIL_DIAGONAL_HALF_UP_RIGHT_BOT {
                            new = RAIL_FLAT;
                        }
                    }
                    Some(ll) if ll.fg == CONTENTS_RAIL => {
                        if new != RAIL_DIAGONAL_HALF_UP_RIGHT_BOT {
                            new = RAIL_FLAT;
                        }
                    }
                    Some(ll) => {
                        if up(ll, -2) {
                            new = if new == RAIL_DIAGONAL_HALF_UP_RIGHT_BOT {
                                RAIL_DIAGONAL_HALF_FLAT
                            } else {
                                RAIL_DIAGONAL_HALF_UP_LEFT_BOT
                            };
                        }
                    }
                }
            } else if up(l, -1) {
                // `R1` is the stale right neighbour (may be NULL)
                new = if r1.is_none_or(|r| r.fg != CONTENTS_RAIL) {
                    RAIL_DIAGONAL_UP_LEFT
                } else {
                    RAIL_DIAGONAL_HALF_UP_LEFT_TOP
                };
            }
        }
    }
    new
}

/// Horizontal shortest distance `cp.x - pos.x` on the cylindrical world (all `i32`, wrapping like C).
pub fn rail_wrapped_dx(cp_x: i32, pos_x: i32, world_width_macro: i32) -> i32 {
    let w = world_width_macro.wrapping_shl(5);
    let raw = cp_x.wrapping_sub(pos_x);
    if raw >= w / 2 {
        raw.wrapping_sub(w)
    } else if raw < (world_width_macro.wrapping_mul(-32)) / 2 {
        raw.wrapping_add(w)
    } else {
        raw
    }
}

/// Rail ivars 0x50..0x5a.
pub struct RailState {
    /// 0x50 `itemType`
    pub item_type: i32,
    /// 0x54 `currentConfiguration`
    pub current_configuration: i32,
    /// 0x58 `drawPoles` (derived; maintained although only the client renders it)
    pub draw_poles: bool,
    /// 0x59 `ownedByStation`
    pub owned_by_station: bool,
}

/// `Rail: DynamicObject` (type 40, instance size 0x5a).
pub struct Rail {
    base: RefCell<DynamicObjectBase>,
    /// The Rail ivars.
    pub st: RefCell<RailState>,
    world: Weak<World>,
    dw: Weak<DynamicWorld>,
}

impl Rail {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<Rail> {
        let (w, d) = (Rc::downgrade(world), Rc::downgrade(dw));
        handle::alloc_obj(|sw| Rail {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            st: RefCell::new(RailState {
                item_type: 0,
                current_configuration: 0,
                draw_poles: false,
                owned_by_station: false,
            }),
            world: w,
            dw: d,
        })
    }

    fn world(&self) -> Rc<World> {
        self.world.upgrade().expect("Rail: world dropped")
    }
    fn dw(&self) -> Rc<DynamicWorld> {
        self.dw.upgrade().expect("Rail: dynamic world dropped")
    }

    /// `-[Rail initWithWorld:dynamicWorld:atPosition:cache:type:ownedByStation:]`.
    ///  `None` = `return nil`.
    pub fn new(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        item_type: i32,
        owned_by_station: bool,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        // the base initialiser always succeeds when the position is given
        let inited = base_init_with_world_dynamic_world_at_position_cache(&me_obj, &wref, &dref, pos, cache);
        debug_assert!(inited);
        {
            let mut st = me.st.borrow_mut();
            st.item_type = item_type;
            st.owned_by_station = owned_by_station;
            st.current_configuration = RAIL_FLAT;
        }
        me.update_rail_configuration(&me_obj);
        me.init_sub_derived_items();
        Some(me_obj)
    }

    /// `-[Rail initWithWorld:dynamicWorld:saveDict:cache:]`. The configuration is NOT recomputed on load
    /// (missing keys give 0 / NO).
    pub fn from_save_dict(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        let inited = base_init_with_world_dynamic_world_save_dict_cache(&me_obj, &wref, &dref, save_dict, cache);
        debug_assert!(inited);
        let get = |k: &str| save_dict.map.get(&k.to_string());
        {
            let mut st = me.st.borrow_mut();
            st.item_type = get("itemType").map_or(0, |v| v.int_value());
            st.owned_by_station = get("ownedByStation").is_some_and(|v| v.bool_value());
            st.current_configuration = get("configuration").map_or(0, |v| v.int_value());
        }
        me.init_sub_derived_items();
        Some(me_obj)
    }

    /// `-[Rail initWithWorld:dynamicWorld:cache:netData:]`: reads 32 bytes without a length check
    /// (short input is zero padded).
    pub fn from_net_data(world: &Rc<World>, dw: &Rc<DynamicWorld>, cache: CacheRef, net_data: &[u8]) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        if !base_init_with_world_dynamic_world_cache_net_data(&me_obj, &wref, &dref, cache, net_data) {
            return None;
        }
        let d = RailCreationNetData::read_le(&padded_bytes(net_data, 0x20));
        {
            let mut st = me.st.borrow_mut();
            st.item_type = d.item_type as i32;
            st.current_configuration = d.configuration as i32;
        }
        me.init_sub_derived_items();
        Some(me_obj)
    }

    /// `-[Rail initSubDerivedItems]`: `drawPoles` only (the geometry reload is client-only).
    pub fn init_sub_derived_items(&self) {
        let mut st = self.st.borrow_mut();
        st.draw_poles = draw_poles_for_configuration(st.current_configuration);
    }

    /// `-[Rail currentConfiguration]`.
    pub fn current_configuration(&self) -> i32 {
        self.st.borrow().current_configuration
    }
    /// `itemType` getter.
    pub fn item_type(&self) -> i32 {
        self.st.borrow().item_type
    }
    /// `ownedByStation` getter.
    pub fn owned_by_station(&self) -> bool {
        self.st.borrow().owned_by_station
    }

    /// `-[Rail updateRailConfiguration]`. Uses the NON-loading `tileAtWorldPosition` (NULL for unloaded
    /// blocks).
    pub fn update_rail_configuration(&self, me: &ObjRef) {
        let world = self.world();
        let pos = self.base.borrow().pos;
        let macro_tiles = world.macro_tiles();
        let probe = |dx: i32, dy: i32| -> Option<RailProbe> {
            let t: Option<TileRef> =
                tile_at_world_position(pos.x.wrapping_add(dx), pos.y.wrapping_add(dy), &macro_tiles, &world);
            t.map(|tt| RailProbe { fg: tt.foreground_contents(), solid: tile_is_solid(Some(tt)) })
        };
        let current = self.st.borrow().current_configuration;
        let new = compute_rail_configuration(current, &probe);
        if new != current {
            self.st.borrow_mut().current_configuration = new;
            if !self.base.borrow().is_net {
                self.base.borrow_mut().update_needs_to_be_sent = true;
                self.dw().dynamic_world_changed_at_pos_object_type(pos, me.object_type());
            }
            self.st.borrow_mut().draw_poles = draw_poles_for_configuration(new);
            // reloadDrawBlockDynamicObjectStaticGeometryForTile: render geometry only
        }
    }
}

impl DynamicObject for Rail {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &RAIL_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn object_type(&self) -> i32 {
        DOT_RAIL
    }
    fn occupies_foreground_contents(&self) -> bool {
        true
    }
    fn freeblock_creation_item_type(&self) -> i32 {
        self.st.borrow().item_type
    }
    /// : nil
    fn free_block_creation_save_dict(&self, _me: &ObjRef) -> Option<PlistDict> {
        None
    }
    fn free_block_creation_data_a(&self) -> u16 {
        0
    }
    fn free_block_creation_data_b(&self) -> u16 {
        0
    }

    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = base_get_save_dict(me);
        let st = self.st.borrow();
        d.map.insert("itemType".to_string(), PlistValue::int(st.item_type as i64));
        d.map.insert("configuration".to_string(), PlistValue::int(st.current_configuration as i64));
        d.map.insert("ownedByStation".to_string(), PlistValue::boolean(st.owned_by_station));
        d
    }

    /// : 32 bytes, padding zero (uninitialised in the original).
    fn creation_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        let st = self.st.borrow();
        let mut d = RailCreationNetData::default();
        d.dynamic_object_net_data = base_dynamic_object_net_data(me);
        d.item_type = st.item_type as u16;
        d.configuration = st.current_configuration as u8;
        Some(d.to_vec())
    }
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        self.creation_net_data_for_client(me, client_id)
    }

    /// : does NOT call super.
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        let d = RailCreationNetData::read_le(&padded_bytes(net_data, 0x20));
        let changed = d.configuration as i32 != self.st.borrow().current_configuration;
        if changed {
            self.st.borrow_mut().current_configuration = d.configuration as i32;
            if !self.base.borrow().is_net {
                self.base.borrow_mut().update_needs_to_be_sent = true;
                let pos = self.base.borrow().pos;
                self.dw().dynamic_world_changed_at_pos_object_type(pos, me.object_type());
            }
            self.st.borrow_mut().draw_poles = draw_poles_for_configuration(d.configuration as i32);
        }
    }

    /// Drops the rail as item `itemType` when it lost its support, else recomputes the shape.
    fn world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        let world = self.world();
        let dw = self.dw();
        for cp in positions.iter().copied() {
            let pos = self.base.borrow().pos;
            let dx = rail_wrapped_dx(cp.x, pos.x, world.world_width_macro());
            if dx.wrapping_abs() < 3 && cp.y >= pos.y.wrapping_sub(1) && cp.y <= pos.y.wrapping_add(1) {
                let (needs_removed, owned) = (self.base.borrow().needs_removed, self.st.borrow().owned_by_station);
                if !dw.is_client() && !needs_removed && !owned {
                    let b = tile_at_world_position_loaded(pos.x, pos.y.wrapping_sub(1), &world);
                    let col = b.is_some_and(|t| t.foreground_contents() == CONTENTS_COLUMN);
                    if !tile_is_solid(b) && !col {
                        let r = tile_at_world_position_loaded(pos.x.wrapping_add(1), pos.y, &world);
                        if !tile_is_solid(r) {
                            let l = tile_at_world_position_loaded(pos.x.wrapping_sub(1), pos.y, &world);
                            if !tile_is_solid(l) {
                                let item = self.st.borrow().item_type;
                                dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                                    pos,
                                    ItemType(item as u32),
                                    0,
                                    0,
                                    None,
                                    None,
                                    false,
                                    1,
                                    None,
                                );
                                dw.remove_standard_object(Some(me));
                                dw.rail_or_station_name_changed();
                                return;
                            }
                        }
                    }
                }
                self.update_rail_configuration(me);
            }
        }
    }
}

/// `[rail currentConfiguration]` through an `ObjRef`; 0 if `o` is not a `Rail` (message to nil / wrong class).
pub fn rail_current_configuration(o: &ObjRef) -> i32 {
    handle::downcast::<Rail>(o).map_or(0, |r| r.current_configuration())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Builds a probe from a map `(dx, dy) -> (fg, solid)`; missing keys are NULL tiles.
    fn run(current: i32, tiles: &[((i32, i32), (u8, bool))]) -> i32 {
        let m: HashMap<(i32, i32), RailProbe> =
            tiles.iter().map(|&(k, (fg, solid))| (k, RailProbe { fg, solid })).collect();
        compute_rail_configuration(current, &|dx, dy| m.get(&(dx, dy)).copied())
    }
    const R: (u8, bool) = (98, false);
    const SOLID: (u8, bool) = (0, true);
    const AIR: (u8, bool) = (0, false);

    /// : truth table, right pass.
    #[test]
    fn right_pass() {
        // R1 NULL: configuration stays; left pass only runs for 1 / 6
        assert_eq!(run(5, &[]), 5);
        assert_eq!(run(1, &[]), 1);
        // R1 rail, R2 NULL or rail -> FLAT
        assert_eq!(run(7, &[((1, 0), R)]), 1);
        assert_eq!(run(7, &[((1, 0), R), ((2, 0), R)]), 1);
        // R1 rail, R2 non-rail: solid -> 6, else (rail above) -> 6, else FLAT
        assert_eq!(run(1, &[((1, 0), R), ((2, 0), SOLID)]), 6);
        assert_eq!(run(1, &[((1, 0), R), ((2, 0), AIR), ((2, 1), R)]), 6);
        assert_eq!(run(1, &[((1, 0), R), ((2, 0), AIR)]), 1);
        // R1 not a rail: solid/up-rail -> 5 (no left rail) or 7 (left rail); else FLAT
        assert_eq!(run(1, &[((1, 0), SOLID)]), 5);
        assert_eq!(run(1, &[((1, 0), AIR), ((1, 1), R)]), 5);
        assert_eq!(run(1, &[((1, 0), SOLID), ((-1, 0), R)]), 7);
        assert_eq!(run(1, &[((1, 0), AIR)]), 1);
    }

    /// : left pass incl. the stale-R1 case.
    #[test]
    fn left_pass() {
        // L rail, LL NULL / rail -> FLAT stays FLAT, 6 stays 6
        assert_eq!(run(1, &[((1, 0), R), ((2, 0), AIR), ((-1, 0), R), ((-2, 0), R)]), 1);
        assert_eq!(run(1, &[((1, 0), R), ((2, 0), SOLID), ((-1, 0), R)]), 6);
        // L rail, LL non-rail solid: FLAT -> 3, 6 -> 8
        assert_eq!(run(1, &[((1, 0), R), ((2, 0), R), ((-1, 0), R), ((-2, 0), SOLID)]), 3);
        assert_eq!(run(1, &[((1, 0), R), ((2, 0), SOLID), ((-1, 0), R), ((-2, 0), SOLID)]), 8);
        assert_eq!(run(1, &[((1, 0), R), ((2, 0), R), ((-1, 0), R), ((-2, 0), AIR), ((-2, 1), R)]), 3);
        assert_eq!(run(1, &[((1, 0), R), ((2, 0), R), ((-1, 0), R), ((-2, 0), AIR)]), 1);
        // L not a rail but solid: 2 when R1 is not a rail (or NULL), 4 when R1 (stale) is a rail
        assert_eq!(run(1, &[((-1, 0), SOLID)]), 2);
        assert_eq!(run(1, &[((1, 0), AIR), ((-1, 0), SOLID)]), 2);
        assert_eq!(run(1, &[((1, 0), R), ((-1, 0), SOLID)]), 4);
        assert_eq!(run(1, &[((1, 0), R), ((2, 0), R), ((-1, 0), AIR), ((-1, 1), R)]), 4);
        // L NULL: nothing
        assert_eq!(run(1, &[((1, 0), R), ((2, 0), R)]), 1);
        // L rail, LL NULL: a flat piece stays flat (and the half slope keeps its shape)
        assert_eq!(run(1, &[((1, 0), R), ((2, 0), R), ((-1, 0), R)]), 1);
        assert_eq!(run(1, &[((1, 0), R), ((2, 0), SOLID), ((-1, 0), R)]), 6);
        // the second pass is skipped for configurations other than 1 / 6
        assert_eq!(run(3, &[((-1, 0), SOLID)]), 3);
        assert_eq!(run(5, &[((1, 0), SOLID), ((-1, 0), SOLID)]), 5);
    }

    #[test]
    fn wrapped_dx_and_poles() {
        // width 4 macros -> W = 128
        assert_eq!(rail_wrapped_dx(3, 125, 4), 6);
        assert_eq!(rail_wrapped_dx(125, 3, 4), -6);
        assert_eq!(rail_wrapped_dx(10, 5, 4), 5);
        assert_eq!(rail_wrapped_dx(69, 5, 4), 64 - 128);
        assert_eq!(rail_wrapped_dx(68, 5, 4), 63);
        for (cfg, p) in
            [(0, false), (1, false), (2, true), (3, true), (4, false), (5, true), (6, false), (7, true), (8, true)]
        {
            assert_eq!(draw_poles_for_configuration(cfg), p, "{cfg}");
        }
    }

    ///  layout.
    #[test]
    fn net_data_layout() {
        let mut d = RailCreationNetData::default();
        d.item_type = 199;
        d.configuration = 6;
        let b = d.to_vec();
        assert_eq!(b.len(), 0x20);
        assert_eq!(&b[0x18..0x1a], &199u16.to_le_bytes());
        assert_eq!(b[0x1a], 6);
        assert!(b[0x1b..].iter().all(|&x| x == 0));
    }
}

#[cfg(test)]
mod world_tests {
    use super::*;
    use crate::trains::train_car::world_tests::{lay_rails, setup, setup_wide};
    use crate::trains::train_car::ITEM_RAIL;

    fn rail_of(o: &ObjRef) -> &Rail {
        handle::downcast::<Rail>(o).unwrap()
    }

    /// Save dict and net image round trips, the free-block face and the class descriptor.
    #[test]
    fn round_trips_and_faces() {
        let (world, dw) = setup("rail_rt");
        lay_rails(&world, &dw, 5, 14);
        let r = dw.get_rail_at_pos(IntPair { x: 8, y: 10 }).unwrap();
        assert_eq!(r.class_info().name, RAIL_CLASS.name);
        assert!(r.occupies_foreground_contents());
        assert_eq!(r.freeblock_creation_item_type(), ITEM_RAIL);
        assert!(r.free_block_creation_save_dict(&r).is_none());
        assert_eq!((r.free_block_creation_data_a(), r.free_block_creation_data_b()), (0, 0));
        assert_eq!(rail_of(&r).item_type(), ITEM_RAIL);
        assert!(!rail_of(&r).owned_by_station());
        let owned = Rail::new(&world, &dw, IntPair { x: 30, y: 10 }, None, ITEM_RAIL, true).unwrap();
        assert!(rail_of(&owned).owned_by_station());
        // save dict
        let sd = owned.get_save_dict(&owned);
        let again = Rail::from_save_dict(&world, &dw, &sd, None).unwrap();
        assert_eq!(rail_of(&again).item_type(), ITEM_RAIL);
        assert!(rail_of(&again).owned_by_station());
        assert_eq!(rail_current_configuration(&again), rail_current_configuration(&owned));
        // net image
        let nd = r.creation_net_data_for_client(&r, None).unwrap();
        assert_eq!(nd, r.update_net_data_for_client(&r, None).unwrap());
        let rep = Rail::from_net_data(&world, &dw, None, &nd).unwrap();
        assert!(rep.is_net());
        assert_eq!(rail_of(&rep).item_type(), ITEM_RAIL);
        // an image at an unloaded tile is refused
        let (w2, d2) = setup_wide("rail_far", 4);
        lay_rails(&w2, &d2, 5, 14);
        let src = d2.get_rail_at_pos(IntPair { x: 8, y: 10 }).unwrap();
        src.base().borrow_mut().pos = IntPair { x: 70, y: 10 };
        let nd = src.creation_net_data_for_client(&src, None).unwrap();
        assert!(Rail::from_net_data(&w2, &d2, None, &nd).is_none());
    }

    /// `remoteUpdate:` adopts a changed configuration (and marks an authoritative rail for sending).
    #[test]
    fn remote_update_changes_configuration() {
        let (world, dw) = setup("rail_remote");
        lay_rails(&world, &dw, 5, 14);
        let r = dw.get_rail_at_pos(IntPair { x: 8, y: 10 }).unwrap();
        let mut img =
            RailCreationNetData { configuration: RAIL_DIAGONAL_UP_RIGHT as u8, ..RailCreationNetData::default() };
        r.set_update_needs_to_be_sent(false);
        r.remote_update(&r, &img.to_vec());
        assert_eq!(rail_current_configuration(&r), RAIL_DIAGONAL_UP_RIGHT);
        assert!(r.update_needs_to_be_sent());
        assert!(rail_of(&r).st.borrow().draw_poles);
        // the same configuration again changes nothing
        r.set_update_needs_to_be_sent(false);
        r.remote_update(&r, &img.to_vec());
        assert!(!r.update_needs_to_be_sent());
        // a replica takes the value without announcing it
        let nd = r.creation_net_data_for_client(&r, None).unwrap();
        let rep = Rail::from_net_data(&world, &dw, None, &nd).unwrap();
        img.configuration = RAIL_FLAT as u8;
        rep.remote_update(&rep, &img.to_vec());
        assert_eq!(rail_current_configuration(&rep), RAIL_FLAT);
        assert!(!rep.update_needs_to_be_sent());
    }

    /// A rail that lost its support (nothing solid below or beside it) drops as an item; one with a neighbour or
    /// ground stays; a rail owned by a station never drops.
    #[test]
    fn unsupported_rail_drops() {
        let (world, dw) = setup("rail_drop");
        lay_rails(&world, &dw, 5, 14);
        let rail_at = |x: i32| dw.get_rail_at_pos(IntPair { x, y: 10 }).unwrap();
        let tile = |x: i32, y: i32| crate::world::tiles::tile_at_world_position_loaded(x, y, &world).unwrap();
        // ground below: stays and recomputes its shape
        let r = rail_at(8);
        r.world_changed(&r, &mut vec![IntPair { x: 8, y: 9 }]);
        assert!(!r.needs_removed());
        // a position far away is ignored
        r.world_changed(&r, &mut vec![IntPair { x: 30, y: 9 }, IntPair { x: 8, y: 20 }]);
        // support below gone, but solid to the right: stays
        tile(8, 9).set_type_index(2);
        tile(9, 10).set_type_index(1);
        r.world_changed(&r, &mut vec![IntPair { x: 8, y: 9 }]);
        assert!(!r.needs_removed());
        // solid to the left instead
        tile(9, 10).set_type_index(2);
        tile(7, 10).set_type_index(1);
        r.world_changed(&r, &mut vec![IntPair { x: 8, y: 9 }]);
        assert!(!r.needs_removed());
        // a column below counts as support
        tile(7, 10).set_type_index(2);
        tile(8, 9).set_foreground_contents(CONTENTS_COLUMN);
        r.world_changed(&r, &mut vec![IntPair { x: 8, y: 9 }]);
        assert!(!r.needs_removed());
        // nothing around: the rail drops and is removed
        tile(8, 9).set_foreground_contents(0);
        r.world_changed(&r, &mut vec![IntPair { x: 8, y: 9 }]);
        assert!(r.needs_removed());
        // a rail owned by a station and an already removed rail never drop
        let owned = Rail::new(&world, &dw, IntPair { x: 12, y: 10 }, None, ITEM_RAIL, true).unwrap();
        tile(12, 9).set_type_index(2);
        owned.world_changed(&owned, &mut vec![IntPair { x: 12, y: 9 }]);
        assert!(!owned.needs_removed());
    }
}
