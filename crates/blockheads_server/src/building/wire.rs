//! `Wire: DynamicObject` (DynamicObjectType 38, item 178 `ITEM_COPPER_WIRE`).
//!
//! Wire objects only describe tiles; conduction is read from tile bytes. Client-only members are not ported.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::column::{
    append_plist_tail, drop_as_free_block, owner_from_client_or_save, read_plist_tail, sd_string, tail_string, wrap_dx,
    Links,
};
use crate::dynworld::state::DynamicWorld;
use crate::objects::base::{
    base_dynamic_object_net_data, base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_cache_net_data, base_init_with_world_dynamic_world_save_dict_cache,
    base_remote_update, padded_bytes, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::netdata::WireCreationNetData;
use crate::types::structs::{IntPair, WireStruct};
use crate::world::helpers::{tile_conducts_electricity, tile_is_solid, tile_is_water};
use crate::world::state::{TileRef, World};
use crate::world::tiles::tile_at_world_position_loaded;

/// `DYNAMIC_OBJECT_TYPE_WIRE`.
pub const DYNAMIC_OBJECT_TYPE_WIRE: i32 = 38;
/// `ITEM_COPPER_WIRE`.
pub const ITEM_COPPER_WIRE: i32 = 178;
/// `CONTENTS_COPPER_WIRE`.
pub const CONTENTS_COPPER_WIRE: u8 = 0x60;
const CONTENTS_ELEVATOR_MOTOR: u8 = 0x68;
const CONTENTS_WORKBENCH: u8 = 0x2e;
const CONTENTS_WORKBENCH_SPRITE: u8 = 0x2f;

/// `WireConfiguration`.
pub const WIRE_UNDEFINED: i32 = 0;
pub const WIRE_ALL_CONNECTIONS: i32 = 1;
pub const WIRE_NO_CONNECTIONS: i32 = 2;
pub const WIRE_ABOVE_BELOW_ONLY: i32 = 3;
pub const WIRE_ABOVE_BELOW_LEFT: i32 = 4;
pub const WIRE_ABOVE_BELOW_RIGHT: i32 = 5;
pub const WIRE_LEFT_RIGHT_ONLY: i32 = 6;
pub const WIRE_LEFT_RIGHT_UP: i32 = 7;
pub const WIRE_LEFT_RIGHT_DOWN: i32 = 8;
pub const WIRE_LEFT_DOWN: i32 = 9;
pub const WIRE_LEFT_UP: i32 = 10;
pub const WIRE_RIGHT_DOWN: i32 = 11;
pub const WIRE_RIGHT_UP: i32 = 12;

/// `WireSolidConfiguration` (`WIRE_REQUIRES_Z_*`).
pub const WZ_UNDEFINED: i32 = 0;
pub const WZ_NOT_SOLID: i32 = 1;
pub const WZ_ALL_CONNECTIONS: i32 = 2;
pub const WZ_THIS_TILE_ONLY: i32 = 3;
pub const WZ_ABOVE_BELOW_ONLY: i32 = 4;
pub const WZ_ABOVE_BELOW_LEFT: i32 = 5;
pub const WZ_ABOVE_BELOW_RIGHT: i32 = 6;
pub const WZ_LEFT_RIGHT_ONLY: i32 = 7;
pub const WZ_LEFT_ONLY: i32 = 8;
pub const WZ_LEFT_RIGHT_UP: i32 = 9;
pub const WZ_LEFT_RIGHT_DOWN: i32 = 10;
pub const WZ_LEFT_DOWN: i32 = 11;
pub const WZ_LEFT_UP: i32 = 12;
pub const WZ_RIGHT_DOWN: i32 = 13;
pub const WZ_RIGHT_UP: i32 = 14;
pub const WZ_RIGHT_ONLY: i32 = 15;
pub const WZ_UP_ONLY: i32 = 16;
pub const WZ_DOWN_ONLY: i32 = 17;

/// `Wire` class descriptor.
pub static WIRE_CLASS: ClassInfo = ClassInfo { name: "Wire", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &[] };

thread_local! {
    static WORKBENCH_HOOK: Cell<fn(&ObjRef) -> bool> = Cell::new(|_| false);
}

/// Registers `-[Workbench usesStoresConductsOrProducesElectricity]` (the workbench module owns `Workbench`; its file is
/// not available when this layer is built. Default: `false`.
pub fn set_workbench_uses_electricity_hook(f: fn(&ObjRef) -> bool) {
    WORKBENCH_HOOK.with(|h| h.set(f));
}

fn workbench_uses_electricity(wb: &ObjRef) -> bool {
    WORKBENCH_HOOK.with(|h| h.get())(wb)
}

/// Pure decision tree of `updateWireConfiguration` for the connection shape. `[up, down, right, left]` = `has[D]`.
pub fn wire_configuration_for(u: bool, d: bool, r: bool, l: bool) -> i32 {
    if u {
        if d {
            if l {
                if r {
                    WIRE_ALL_CONNECTIONS
                } else {
                    WIRE_ABOVE_BELOW_LEFT
                }
            } else if r {
                WIRE_ABOVE_BELOW_RIGHT
            } else {
                WIRE_ABOVE_BELOW_ONLY
            }
        } else if l {
            if r {
                WIRE_LEFT_RIGHT_UP
            } else {
                WIRE_LEFT_UP
            }
        } else if r {
            WIRE_RIGHT_UP
        } else {
            WIRE_ABOVE_BELOW_ONLY
        }
    } else if d {
        if l {
            if r {
                WIRE_LEFT_RIGHT_DOWN
            } else {
                WIRE_LEFT_DOWN
            }
        } else if r {
            WIRE_RIGHT_DOWN
        } else {
            WIRE_ABOVE_BELOW_ONLY
        }
    } else if l || r {
        WIRE_LEFT_RIGHT_ONLY
    } else {
        WIRE_NO_CONNECTIONS
    }
}

/// Pure decision tree of the solid configuration (only evaluated when the wire's tile is solid); `reqZ` flags.
pub fn wire_solid_configuration_for(u: bool, d: bool, r: bool, l: bool) -> i32 {
    if u {
        if d {
            if l {
                if r {
                    WZ_ALL_CONNECTIONS
                } else {
                    WZ_ABOVE_BELOW_LEFT
                }
            } else if r {
                WZ_ABOVE_BELOW_RIGHT
            } else {
                WZ_ABOVE_BELOW_ONLY
            }
        } else if l {
            if r {
                WZ_LEFT_RIGHT_UP
            } else {
                WZ_LEFT_UP
            }
        } else if r {
            WZ_RIGHT_UP
        } else {
            WZ_UP_ONLY
        }
    } else if d {
        if l {
            if r {
                WZ_LEFT_RIGHT_DOWN
            } else {
                WZ_LEFT_DOWN
            }
        } else if r {
            WZ_RIGHT_DOWN
        } else {
            WZ_DOWN_ONLY
        }
    } else if l {
        if r {
            WZ_LEFT_RIGHT_ONLY
        } else {
            WZ_LEFT_ONLY
        }
    } else if r {
        WZ_RIGHT_ONLY
    } else {
        WZ_THIS_TILE_ONLY
    }
}

/// Wire ivars.
pub struct WireState {
    pub item_type: i32,
    pub current_configuration: i32,
    pub current_solid_configuration: i32,
}

/// `Wire: DynamicObject` (type 38).
pub struct Wire {
    base: RefCell<DynamicObjectBase>,
    /// The Wire ivars.
    pub st: RefCell<WireState>,
    links: Links,
}

impl Wire {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<Wire> {
        let links = Links::new(world, dw);
        handle::alloc_obj(|sw| Wire {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            st: RefCell::new(WireState { item_type: 0, current_configuration: 0, current_solid_configuration: 0 }),
            links,
        })
    }

    /// Placement constructor. Order: base, itemType, owner, `updateWireConfiguration`, `initSubDerivedItems` (empty).
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
        // `base_init_with_world_dynamic_world_at_position_cache` answers true on every path
        let _ = base_init_with_world_dynamic_world_at_position_cache(&me_obj, &wref, &dref, pos, cache);
        me.st.borrow_mut().item_type = item_type;
        if let Some(o) = owner_from_client_or_save(client_id, save_dict) {
            me.base.borrow_mut().owner_id = Some(o);
        }
        me.update_wire_configuration(&me_obj);
        Some(me_obj)
    }

    /// Save constructor (no recompute; legacy `solidConfiguration` 0 becomes 1).
    pub fn init_with_world_dynamic_world_save_dict_cache(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        // `base_init_with_world_dynamic_world_save_dict_cache` answers true on every path
        let _ = base_init_with_world_dynamic_world_save_dict_cache(&me_obj, &wref, &dref, save_dict, cache);
        let g = |k: &str| save_dict.map.get(&k.to_string()).map_or(0, |v| v.int_value());
        {
            let mut st = me.st.borrow_mut();
            st.item_type = g("itemType");
            st.current_configuration = g("configuration");
            st.current_solid_configuration = g("solidConfiguration");
            if st.current_solid_configuration == 0 {
                st.current_solid_configuration = WZ_NOT_SOLID;
            }
        }
        me.base.borrow_mut().owner_id = sd_string(save_dict, "ownerID");
        Some(me_obj)
    }

    /// Net constructor.
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
        let s = WireCreationNetData::read_le(&padded_bytes(net_data, 0x20));
        {
            let mut st = me.st.borrow_mut();
            st.item_type = s.item_type as i32;
            st.current_configuration = s.configuration as i32;
            st.current_solid_configuration = s.solid_configuration as i32;
        }
        if net_data.len() > 0x20 {
            let owner = tail_string(&read_plist_tail(net_data, 0x20), "ownerID");
            me.base.borrow_mut().owner_id = owner;
        }
        Some(me_obj)
    }

    /// `-[Wire itemType]`
    pub fn item_type(&self) -> i32 {
        self.st.borrow().item_type
    }

    /// `has`/`reqZ` of one neighbour.
    fn neighbour(&self, world: &Rc<World>, nx: i32, ny: i32) -> (bool, bool) {
        let t: Option<TileRef> = tile_at_world_position_loaded(nx, ny, world);
        let Some(tr) = t else { return (false, false) };
        let tile = tr.get();
        let mut has = false;
        if tile_conducts_electricity(t) || tile.contents == CONTENTS_ELEVATOR_MOTOR {
            has = true;
        } else if tile.contents == CONTENTS_WORKBENCH || tile.contents == CONTENTS_WORKBENCH_SPRITE {
            let wb = self.links.dw().workbench_at_pos(IntPair { x: nx, y: ny });
            has = wb.is_some_and(|w| workbench_uses_electricity(&w));
        }
        let req_z = has && !tile_is_solid(t);
        (has, req_z)
    }

    /// `-[Wire updateWireConfiguration]`.
    pub fn update_wire_configuration(&self, _me: &ObjRef) {
        let pos = self.base.borrow().pos;
        let world = self.links.world();
        let this_solid = tile_is_solid(tile_at_world_position_loaded(pos.x, pos.y, &world));
        // evaluation order: Up, Down, Right, Left
        let (hu, zu) = self.neighbour(&world, pos.x, pos.y + 1);
        let (hd, zd) = self.neighbour(&world, pos.x, pos.y - 1);
        let (hr, zr) = self.neighbour(&world, pos.x + 1, pos.y);
        let (hl, zl) = self.neighbour(&world, pos.x - 1, pos.y);
        let new_cfg = wire_configuration_for(hu, hd, hr, hl);
        let new_solid = if this_solid { wire_solid_configuration_for(zu, zd, zr, zl) } else { WZ_NOT_SOLID };
        let changed = {
            let mut st = self.st.borrow_mut();
            if new_cfg != st.current_configuration || new_solid != st.current_solid_configuration {
                st.current_configuration = new_cfg;
                st.current_solid_configuration = new_solid;
                true
            } else {
                false
            }
        };
        if changed && !self.base.borrow().is_net {
            self.base.borrow_mut().update_needs_to_be_sent = true;
            self.links.dw().dynamic_world_changed_at_pos_object_type(pos, DYNAMIC_OBJECT_TYPE_WIRE);
        }
    }

    /// Drop sequence of `worldChanged:` (free block with a NIL save dict, then `removeStandardObject:`).
    /// The caller returns afterwards in every case (also when the client/needsRemoved guards fire).
    fn drop_wire(&self, me: &ObjRef, pos: IntPair) {
        if self.base.borrow().needs_removed {
            return;
        }
        drop_as_free_block(me, &self.links, pos, self.item_type(), None);
    }
}

impl DynamicObject for Wire {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &WIRE_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_WIRE
    }
    fn freeblock_creation_item_type(&self) -> i32 {
        self.item_type()
    }
    /// `freeBlockCreationSaveDict` is nil.
    fn free_block_creation_save_dict(&self, _me: &ObjRef) -> Option<PlistDict> {
        None
    }

    /// `-[Wire occupiesForegroundContents]`: live tile read. UB-PARITY: NULL tile (cannot happen for `0 <= y < 0x400`) gives false.
    fn occupies_foreground_contents(&self) -> bool {
        let pos = self.base.borrow().pos;
        tile_at_world_position_loaded(pos.x, pos.y, &self.links.world())
            .is_some_and(|t| t.foreground_contents() == CONTENTS_COPPER_WIRE)
    }
    /// `-[Wire occupiesNormalContents]`. UB-PARITY as above.
    fn occupies_normal_contents(&self) -> bool {
        let pos = self.base.borrow().pos;
        tile_at_world_position_loaded(pos.x, pos.y, &self.links.world())
            .is_some_and(|t| t.contents() == CONTENTS_COPPER_WIRE)
    }

    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = base_get_save_dict(me);
        let (it, cfg, sc) = {
            let st = self.st.borrow();
            (st.item_type, st.current_configuration, st.current_solid_configuration)
        };
        d.map.insert("itemType".to_string(), PlistValue::int(it as i64));
        d.map.insert("configuration".to_string(), PlistValue::int(cfg as i64));
        d.map.insert("solidConfiguration".to_string(), PlistValue::int(sc as i64));
        if let Some(o) = self.base.borrow().owner_id.clone() {
            d.map.insert("ownerID".to_string(), PlistValue::string(o));
        }
        d
    }

    fn creation_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        let st = self.st.borrow();
        let s = WireCreationNetData {
            dynamic_object_net_data: base_dynamic_object_net_data(me),
            item_type: st.item_type as u16,
            configuration: st.current_configuration as u8,
            solid_configuration: st.current_solid_configuration as u8,
            padding: [0; 4],
        };
        let mut out = s.to_vec();
        let owner = self.base.borrow().owner_id.clone();
        append_plist_tail(&mut out, &[("ownerID", owner.as_deref())]);
        Some(out)
    }
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        self.creation_net_data_for_client(me, client_id)
    }

    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        base_remote_update(me, net_data);
        let u = WireCreationNetData::read_le(&padded_bytes(net_data, 0x20));
        let mut st = self.st.borrow_mut();
        if u.configuration as i32 != st.current_configuration
            || u.solid_configuration as i32 != st.current_solid_configuration
        {
            st.current_configuration = u.configuration as i32;
            st.current_solid_configuration = u.solid_configuration as i32;
        }
    }

    /// `-[Wire worldChanged:]`.
    fn world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        if self.item_type() != ITEM_COPPER_WIRE {
            return;
        }
        let world = self.links.world();
        let pos = self.base.borrow().pos;
        for p in positions.iter() {
            // (1) water on the wire tile (unwrapped position compare)
            if *p == pos && tile_is_water(tile_at_world_position_loaded(pos.x, pos.y, &world)) {
                self.drop_wire(me, pos);
                return;
            }
            // (2) horizontal wrap distance
            let dx = wrap_dx(p.x.wrapping_sub(pos.x), world.world_width_macro());
            // (3) neighbourhood
            if dx.abs() < 2 && ((p.x == pos.x && pos.y <= p.y) || (pos.y - 1 <= p.y && p.y <= pos.y + 1)) {
                self.update_wire_configuration(me);
                let no_conn = self.st.borrow().current_configuration == WIRE_NO_CONNECTIONS;
                // UB-PARITY: the original does not NULL-check the wire's own tile; NULL gives false here.
                if no_conn
                    && tile_at_world_position_loaded(pos.x, pos.y, &world)
                        .is_some_and(|t| t.back_wall_type_index() == 2)
                {
                    let free = |nx: i32, ny: i32| -> bool {
                        match tile_at_world_position_loaded(nx, ny, &world) {
                            None => false,
                            Some(t) => {
                                !tile_is_solid(Some(t))
                                    && t.contents() != CONTENTS_COPPER_WIRE
                                    && t.contents() != CONTENTS_WORKBENCH
                            }
                        }
                    };
                    // short-circuit order: Down, Up, Left, Right
                    if free(pos.x, pos.y - 1)
                        && free(pos.x, pos.y + 1)
                        && free(pos.x - 1, pos.y)
                        && free(pos.x + 1, pos.y)
                    {
                        self.drop_wire(me, pos);
                        return;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::super::column::testkit::*;

    fn mk(w: &Rc<World>, d: &Rc<DynamicWorld>, x: i32, y: i32, owner: Option<&str>) -> ObjRef {
        Wire::init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
            w,
            d,
            pos(x, y),
            None,
            ITEM_COPPER_WIRE,
            None,
            owner,
        )
        .unwrap()
    }
    fn cfgs(o: &ObjRef) -> (i32, i32) {
        let w = handle::downcast::<Wire>(o).unwrap();
        let st = w.st.borrow();
        (st.current_configuration, st.current_solid_configuration)
    }

    #[test]
    fn placement_computes_shape_from_tiles() {
        let (w, d) = env();
        let o = mk(&w, &d, 40, 20, Some("own"));
        assert_eq!(cfgs(&o), (WIRE_NO_CONNECTIONS, WZ_NOT_SOLID));
        assert!(o.update_needs_to_be_sent());
        // conducting neighbours: wire contents above and to the left, elevator motor to the right
        tile(&w, 40, 21).set_contents(CONTENTS_COPPER_WIRE);
        tile(&w, 39, 20).set_foreground_contents(CONTENTS_COPPER_WIRE);
        tile(&w, 41, 20).set_contents(0x68);
        handle::downcast::<Wire>(&o).unwrap().update_wire_configuration(&o);
        assert_eq!(cfgs(&o).0, WIRE_LEFT_RIGHT_UP);
        // wire inside a solid tile: neighbours that are non-solid require Z
        solid(&w, 40, 20, true);
        handle::downcast::<Wire>(&o).unwrap().update_wire_configuration(&o);
        assert_eq!(cfgs(&o), (WIRE_LEFT_RIGHT_UP, WZ_LEFT_RIGHT_UP));
        // solid neighbour above does not need Z
        solid(&w, 40, 21, true);
        handle::downcast::<Wire>(&o).unwrap().update_wire_configuration(&o);
        assert_eq!(cfgs(&o), (WIRE_LEFT_RIGHT_UP, WZ_LEFT_RIGHT_ONLY));
    }

    #[test]
    fn save_defaults_and_net_round_trip() {
        let (w, d) = env();
        let o = mk(&w, &d, 40, 20, Some("own"));
        let sd = o.get_save_dict(&o);
        assert_eq!(sd.map.len(), 8);
        let mut legacy = sd.clone();
        legacy.map.insert("solidConfiguration".to_string(), PlistValue::int(0));
        let o2 = Wire::init_with_world_dynamic_world_save_dict_cache(&w, &d, &legacy, None).unwrap();
        assert_eq!(cfgs(&o2).1, WZ_NOT_SOLID);
        assert_eq!(o2.owner_id().as_deref(), Some("own"));
        let b = o.creation_net_data_for_client(&o, None).unwrap();
        assert_eq!(&b[0x18..0x1a], &178u16.to_le_bytes());
        assert_eq!(b[0x1a], WIRE_NO_CONNECTIONS as u8);
        assert_eq!(b[0x1b], WZ_NOT_SOLID as u8);
        assert!(b[0x1c..0x20].iter().all(|x| *x == 0));
        let o3 = Wire::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &b).unwrap();
        assert!(o3.is_net());
        assert_eq!(cfgs(&o3), (WIRE_NO_CONNECTIONS, WZ_NOT_SOLID));
        assert_eq!(o3.owner_id().as_deref(), Some("own"));
        assert!(o.free_block_creation_save_dict(&o).is_none());
    }

    /// Water drops the wire (nil save dict); the occupies flags read the live tile.
    #[test]
    fn water_and_support_loss_drop() {
        let (w, d) = env();
        let o = mk(&w, &d, 40, 20, None);
        tile(&w, 40, 20).set_contents(CONTENTS_COPPER_WIRE);
        assert!(o.occupies_normal_contents() && !o.occupies_foreground_contents());
        tile(&w, 40, 20).set_type_index(3);
        o.world_changed(&o, &mut vec![pos(40, 20)]);
        assert!(o.needs_removed());
        assert_eq!(tile(&w, 40, 20).contents(), 0);

        // NO_CONNECTIONS + open back wall + four non-solid, non-wire neighbours: loses support
        let p = mk(&w, &d, 60, 20, None);
        tile(&w, 60, 20).set_contents(CONTENTS_COPPER_WIRE);
        p.world_changed(&p, &mut vec![pos(60, 21)]);
        assert!(p.needs_removed());
        // a solid neighbour keeps it
        let q = mk(&w, &d, 70, 20, None);
        solid(&w, 70, 19, true);
        q.world_changed(&q, &mut vec![pos(70, 19)]);
        assert!(!q.needs_removed());
        // other item types never react
        let r = Wire::init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
            &w,
            &d,
            pos(90, 20),
            None,
            5,
            None,
            None,
        )
        .unwrap();
        r.world_changed(&r, &mut vec![pos(90, 20)]);
        assert!(!r.needs_removed());
    }

    /// : both trees over all 16 combinations.
    #[test]
    fn trees_exhaustive() {
        for bits in 0..16u32 {
            let (u, d, r, l) = (bits & 1 != 0, bits & 2 != 0, bits & 4 != 0, bits & 8 != 0);
            let c = wire_configuration_for(u, d, r, l);
            let z = wire_solid_configuration_for(u, d, r, l);
            assert!((WIRE_ALL_CONNECTIONS..=WIRE_RIGHT_UP).contains(&c));
            assert!((WZ_ALL_CONNECTIONS..=WZ_DOWN_ONLY).contains(&z));
            // single-neighbour leaves
            if u && !d && !l && !r {
                assert_eq!(c, WIRE_ABOVE_BELOW_ONLY);
                assert_eq!(z, WZ_UP_ONLY);
            }
            if d && !u && !l && !r {
                assert_eq!(c, WIRE_ABOVE_BELOW_ONLY);
                assert_eq!(z, WZ_DOWN_ONLY);
            }
            if bits == 0 {
                assert_eq!(c, WIRE_NO_CONNECTIONS);
                assert_eq!(z, WZ_THIS_TILE_ONLY);
            }
            if bits == 15 {
                assert_eq!(c, WIRE_ALL_CONNECTIONS);
                assert_eq!(z, WZ_ALL_CONNECTIONS);
            }
        }
        assert_eq!(wire_configuration_for(false, false, true, false), WIRE_LEFT_RIGHT_ONLY);
        assert_eq!(wire_configuration_for(true, false, true, true), WIRE_LEFT_RIGHT_UP);
        assert_eq!(wire_configuration_for(false, true, true, true), WIRE_LEFT_RIGHT_DOWN);
        assert_eq!(wire_configuration_for(true, true, true, false), WIRE_ABOVE_BELOW_RIGHT);
        assert_eq!(wire_configuration_for(true, true, false, true), WIRE_ABOVE_BELOW_LEFT);
        assert_eq!(wire_solid_configuration_for(false, false, false, true), WZ_LEFT_ONLY);
        assert_eq!(wire_solid_configuration_for(false, false, true, false), WZ_RIGHT_ONLY);
        assert_eq!(wire_solid_configuration_for(false, false, true, true), WZ_LEFT_RIGHT_ONLY);
    }
}

#[cfg(test)]
mod more_tests {
    use super::super::column::fuzz::*;
    use super::super::column::testkit::*;
    use super::*;

    fn mk(w: &Rc<World>, d: &Rc<DynamicWorld>, x: i32, y: i32, item: i32) -> ObjRef {
        Wire::init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
            w,
            d,
            pos(x, y),
            None,
            item,
            None,
            None,
        )
        .unwrap()
    }

    #[test]
    fn trait_answers_and_net_copies() {
        let (w, d) = env();
        let o = mk(&w, &d, 40, 20, ITEM_COPPER_WIRE);
        let _ = o.class_info();
        assert_eq!(o.freeblock_creation_item_type(), ITEM_COPPER_WIRE);
        let mut net = o.update_net_data_for_client(&o, None).unwrap();
        net[0x1a] = 3;
        net[0x1b] = 4;
        o.remote_update(&o, &net);
        net[0x1b] = 5;
        o.remote_update(&o, &net);
        net[0x1b] = 4;
        o.remote_update(&o, &net);
        let wire = handle::downcast::<Wire>(&o).unwrap();
        assert_eq!((wire.st.borrow().current_configuration, wire.st.borrow().current_solid_configuration), (3, 4));
        let mut lost = net.clone();
        lost[12..16].copy_from_slice(&300u32.to_le_bytes());
        assert!(Wire::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &lost).is_none());
    }

    #[test]
    fn workbenches_next_to_a_wire_count_when_they_use_electricity() {
        crate::objects::registry::install();
        let (w, d) = env();
        let wire = mk(&w, &d, 40, 20, ITEM_COPPER_WIRE);
        tile(&w, 41, 20).set_contents(CONTENTS_WORKBENCH);
        wire.world_changed(&wire, &mut vec![pos(41, 20)]);
        let cfg = handle::downcast::<Wire>(&wire).unwrap().st.borrow().current_configuration;
        // no workbench object there: the wire stays unconnected
        assert_eq!(cfg, WIRE_NO_CONNECTIONS);
        let bench = crate::crafting::workbench::Workbench::init_with_world_dynamic_world_at_position_cache_type_flipped_save_dict_placed_by_client_client_name(
            &w,
            &(d.clone() as crate::objects::base::DynamicWorldRef),
            pos(41, 20),
            None,
            crate::types::enums::WorkbenchType(15),
            false,
            None,
            None,
            None,
        )
        .unwrap();
        let bench: ObjRef = bench;
        d.by_pos_set(45, crate::world::tiles::world_index_at_world_pos(pos(41, 20), &w), &bench);
        let concrete = handle::downcast::<Wire>(&wire).unwrap();
        concrete.update_wire_configuration(&wire);
        assert_ne!(concrete.st.borrow().current_configuration, WIRE_NO_CONNECTIONS);
    }

    #[test]
    fn wires_in_every_neighbourhood() {
        let (w, d) = env();
        let mut r = Lcg(9);
        let contents =
            [0u8, 0, CONTENTS_COPPER_WIRE, CONTENTS_ELEVATOR_MOTOR, CONTENTS_WORKBENCH, CONTENTS_WORKBENCH_SPRITE];
        for item in [ITEM_COPPER_WIRE, ITEM_COPPER_WIRE + 1] {
            for y in [0, 30, 63] {
                for _ in 0..200 {
                    scramble(&w, &mut r, 40, y, &[2, 2, 6, 1, 3], &contents, &[0]);
                    let o = mk(&w, &d, 40, y, item);
                    if r.next() % 5 == 0 {
                        o.base().borrow_mut().pos = pos(40, 300);
                    }
                    if r.next() % 4 == 0 {
                        o.set_needs_removed(&o, true);
                    }
                    let p = pos(40 + (r.next() % 5) as i32 - 2, y + (r.next() % 4) as i32 - 2);
                    o.world_changed(&o, &mut vec![pos(40, y), p]);
                }
            }
        }
    }
}
