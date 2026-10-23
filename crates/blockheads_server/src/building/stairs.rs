//! `Stairs: DynamicObject` (DynamicObjectType 54).
//!
//!  Stairs" (1.4.1 to 1.4.20).
//! There is no water handling in `Stairs` itself. Client-only members are not ported.

use std::cell::RefCell;
use std::rc::Rc;

use super::column::{
    append_plist_tail, drop_as_free_block, ice_melt_step, owner_from_client_or_save, read_plist_tail, sd_get,
    sd_string, tail_string, wrap_dx, Links,
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
use crate::rng::glibc_rand::r01;
use crate::types::netdata::StairsCreationNetData;
use crate::types::structs::{IntPair, WireStruct};
use crate::world::helpers::tile_is_solid;
use crate::world::state::{TileRef, World};
use crate::world::tiles::tile_at_world_position_loaded;

/// `DYNAMIC_OBJECT_TYPE_STAIRS`.
pub const DYNAMIC_OBJECT_TYPE_STAIRS: i32 = 54;
/// `CONTENTS_STAIRS` (tile `contents` byte).
pub const CONTENTS_STAIRS: u8 = 101;
/// `ITEM_ICE_STAIRS`.
pub const ITEM_ICE_STAIRS: i32 = 253;

/// `StairsConfiguration`.
pub const STAIRS_UNDEFINED: i32 = 0;
pub const STAIRS_HIGH_RIGHT_SOLID: i32 = 1;
pub const STAIRS_HIGH_LEFT_SOLID: i32 = 2;
pub const STAIRS_HIGH_RIGHT_FLOATING: i32 = 3;
pub const STAIRS_HIGH_LEFT_FLOATING: i32 = 4;

/// `Stairs` class descriptor.
pub static STAIRS_CLASS: ClassInfo =
    ClassInfo { name: "Stairs", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &[] };

/// Stairs ivars.
pub struct StairsState {
    pub item_type: i32,
    pub current_configuration: i32,
    pub paint_color: u16,
    pub ice_melt_timer: f32,
}

/// `Stairs: DynamicObject` (type 54).
pub struct Stairs {
    base: RefCell<DynamicObjectBase>,
    /// The Stairs ivars.
    pub st: RefCell<StairsState>,
    links: Links,
}

impl Stairs {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<Stairs> {
        let links = Links::new(world, dw);
        handle::alloc_obj(|sw| Stairs {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            st: RefCell::new(StairsState {
                item_type: 0,
                current_configuration: 0,
                paint_color: 0,
                ice_melt_timer: 0.0,
            }),
            links,
        })
    }

    /// Placement constructor. Order: base, itemType, owner, paint, `initSubDerivedItems` (rand), `updateConfiguration`.
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
        me.st.borrow_mut().paint_color = sd_get(save_dict, "paintColor").map_or(0, |v| v.unsigned_int_value()) as u16;
        me.init_sub_derived_items();
        me.update_configuration(&me_obj);
        Some(me_obj)
    }

    /// Save constructor (no `updateConfiguration`).
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
        let g = |k: &str| save_dict.map.get(&k.to_string());
        {
            let mut st = me.st.borrow_mut();
            st.item_type = g("itemType").map_or(0, |v| v.int_value());
            st.current_configuration = g("configuration").map_or(0, |v| v.int_value());
            st.paint_color = g("paintColor").map_or(0, |v| v.unsigned_int_value()) as u16;
        }
        me.base.borrow_mut().owner_id = sd_string(save_dict, "ownerID");
        me.init_sub_derived_items();
        Some(me_obj)
    }

    /// Net constructor (no `updateConfiguration`).
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
        let s = StairsCreationNetData::read_le(&padded_bytes(net_data, 0x20));
        {
            let mut st = me.st.borrow_mut();
            st.item_type = s.item_type as i32;
            st.current_configuration = s.configuration as i32;
            st.paint_color = s.paint_color;
        }
        if net_data.len() > 0x20 {
            let owner = tail_string(&read_plist_tail(net_data, 0x20), "ownerID");
            me.base.borrow_mut().owner_id = owner;
        }
        me.init_sub_derived_items();
        Some(me_obj)
    }

    /// `-[Stairs initSubDerivedItems]`: exactly one `rand()`.
    pub fn init_sub_derived_items(&self) {
        self.st.borrow_mut().ice_melt_timer = r01() * 5.0f32;
    }

    /// `-[Stairs itemType]`
    pub fn item_type(&self) -> i32 {
        self.st.borrow().item_type
    }

    /// `-[Stairs isTransparent]`.
    pub fn is_transparent(&self) -> bool {
        matches!(self.item_type(), 265 | 267 | 253 | 339..=343)
    }

    /// `-[Stairs paint:]`.
    pub fn paint(&self, _me: &ObjRef, color_index: u16) {
        self.st.borrow_mut().paint_color = color_index;
        if !self.base.borrow().is_net {
            self.base.borrow_mut().update_needs_to_be_sent = true;
            let pos = self.base.borrow().pos;
            self.links.dw().dynamic_world_changed_at_pos_object_type(pos, DYNAMIC_OBJECT_TYPE_STAIRS);
        }
    }

    /// `-[Stairs updateConfiguration]`: tile lookups in the exact order of (NULL tiles matter).
    pub fn update_configuration(&self, _me: &ObjRef) {
        let pos = self.base.borrow().pos;
        let world = self.links.world();
        let t = |dx: i32, dy: i32| -> Option<TileRef> { tile_at_world_position_loaded(pos.x + dx, pos.y + dy, &world) };
        let mut high_right = true;
        let l = t(-1, 0);
        if l.is_some() && tile_is_solid(l) {
            let ar = t(1, 1);
            if let Some(ar) = ar {
                if ar.contents() != CONTENTS_STAIRS {
                    if let Some(bl) = t(-1, -1) {
                        high_right = bl.contents() == CONTENTS_STAIRS;
                    }
                }
            }
        }
        if high_right {
            if let Some(x) = t(-1, 1) {
                if x.contents() == CONTENTS_STAIRS {
                    high_right = false;
                }
            }
        }
        if high_right {
            if let Some(x) = t(1, -1) {
                if x.contents() == CONTENTS_STAIRS {
                    high_right = false;
                }
            }
        }
        let mut solid_base = true;
        if let Some(b) = t(0, -1) {
            solid_base = tile_is_solid(Some(b));
        }
        let cur = t(0, 0);
        if solid_base {
            if let Some(cur) = cur {
                if cur.back_wall_type_index() != 2 {
                    let side = if high_right { t(1, 0) } else { t(-1, 0) };
                    if !tile_is_solid(side) {
                        solid_base = false;
                    }
                }
            }
        }
        let new = if solid_base {
            if high_right {
                STAIRS_HIGH_RIGHT_SOLID
            } else {
                STAIRS_HIGH_LEFT_SOLID
            }
        } else if high_right {
            STAIRS_HIGH_RIGHT_FLOATING
        } else {
            STAIRS_HIGH_LEFT_FLOATING
        };
        let changed = {
            let mut st = self.st.borrow_mut();
            if new != st.current_configuration {
                st.current_configuration = new;
                true
            } else {
                false
            }
        };
        if changed && !self.base.borrow().is_net {
            self.base.borrow_mut().update_needs_to_be_sent = true;
            self.links.dw().dynamic_world_changed_at_pos_object_type(pos, DYNAMIC_OBJECT_TYPE_STAIRS);
        }
    }
}

impl DynamicObject for Stairs {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &STAIRS_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_STAIRS
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
        let (it, cfg, pc) = {
            let st = self.st.borrow();
            (st.item_type, st.current_configuration, st.paint_color)
        };
        d.map.insert("itemType".to_string(), PlistValue::int(it as i64));
        d.map.insert("configuration".to_string(), PlistValue::int(cfg as i64));
        d.map.insert("paintColor".to_string(), PlistValue::int(pc as i64));
        if let Some(o) = self.base.borrow().owner_id.clone() {
            d.map.insert("ownerID".to_string(), PlistValue::string(o));
        }
        d
    }

    fn creation_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        let st = self.st.borrow();
        let s = StairsCreationNetData {
            dynamic_object_net_data: base_dynamic_object_net_data(me),
            item_type: st.item_type as u16,
            configuration: st.current_configuration as u8,
            padding_a: 0,
            paint_color: st.paint_color,
            padding: [0; 2],
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
        let u = StairsCreationNetData::read_le(&padded_bytes(net_data, 0x20));
        let mut st = self.st.borrow_mut();
        if u.configuration as i32 != st.current_configuration {
            st.current_configuration = u.configuration as i32;
        }
        if u.paint_color != st.paint_color {
            st.paint_color = u.paint_color;
        }
    }

    /// `-[Stairs update:accurateDT:isSimulation:]`: only ice stairs act.
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, _accurate_dt: f32, _is_simulation: bool) {
        let dw = self.links.dw();
        let (nr, pos) = {
            let b = self.base.borrow();
            (b.needs_removed, b.pos)
        };
        if !dw.is_client() && !nr && self.item_type() == ITEM_ICE_STAIRS {
            let mut timer = self.st.borrow().ice_melt_timer;
            ice_melt_step(me, &self.links, pos, dt, &mut timer);
            self.st.borrow_mut().ice_melt_timer = timer;
        }
    }

    /// `-[Stairs worldChanged:]`.
    fn world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        let world = self.links.world();
        let pos = self.base.borrow().pos;
        for p in positions.iter() {
            let dx = wrap_dx(p.x.wrapping_sub(pos.x), world.world_width_macro());
            if dx.abs() < 2 && p.y >= pos.y - 1 {
                self.update_configuration(me);
                let cfg = self.st.borrow().current_configuration;
                if cfg == STAIRS_HIGH_RIGHT_FLOATING || cfg == STAIRS_HIGH_LEFT_FLOATING {
                    let no_back_wall = tile_at_world_position_loaded(pos.x, pos.y, &world)
                        .map_or(false, |t| t.back_wall_type_index() == 2);
                    if no_back_wall {
                        if self.base.borrow().needs_removed {
                            return;
                        }
                        let sd = self.get_save_dict(me);
                        drop_as_free_block(me, &self.links, pos, self.item_type(), Some(sd));
                        return;
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
    use crate::rng::glibc_rand::{rand, srand};

    fn mk(w: &Rc<World>, d: &Rc<DynamicWorld>, x: i32, y: i32, item: i32, owner: Option<&str>) -> ObjRef {
        Stairs::init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
            w,
            d,
            pos(x, y),
            None,
            item,
            None,
            owner,
        )
        .unwrap()
    }
    fn st(o: &ObjRef) -> &Stairs {
        handle::downcast::<Stairs>(o).unwrap()
    }
    fn cfg(o: &ObjRef) -> i32 {
        st(o).st.borrow().current_configuration
    }

    /// Placement always changes 0 -> 1..4 and flags the update; solid floor + solid side gives SOLID.
    #[test]
    fn configuration_cases() {
        let (w, d) = env();
        // floating: nothing below
        let a = mk(&w, &d, 40, 20, 229, None);
        assert_eq!(cfg(&a), STAIRS_HIGH_RIGHT_FLOATING);
        assert!(a.update_needs_to_be_sent());
        // solid below (B), solid on the high side (+1,0): HIGH_RIGHT_SOLID
        solid(&w, 40, 19, true);
        solid(&w, 41, 20, true);
        st(&a).update_configuration(&a);
        assert_eq!(cfg(&a), STAIRS_HIGH_RIGHT_SOLID);
        // a stairs tile at (-1,+1) forces high-left; the open back wall (type 2) skips the side-support test
        tile(&w, 39, 21).set_contents(CONTENTS_STAIRS);
        st(&a).update_configuration(&a);
        assert_eq!(cfg(&a), STAIRS_HIGH_LEFT_SOLID);
        // with a real back wall the left side must be solid
        tile(&w, 40, 20).set_back_wall_type_index(1);
        st(&a).update_configuration(&a);
        assert_eq!(cfg(&a), STAIRS_HIGH_LEFT_FLOATING);
        solid(&w, 39, 20, true);
        st(&a).update_configuration(&a);
        assert_eq!(cfg(&a), STAIRS_HIGH_LEFT_SOLID);
        // a NULL side/below tile (y outside the world) counts as floating / solid-base respectively
        let top = mk(&w, &d, 80, 0, 229, None);
        assert!(matches!(cfg(&top), STAIRS_HIGH_RIGHT_SOLID | STAIRS_HIGH_RIGHT_FLOATING));
    }

    /// L solid and AR/BL logic decides the side.
    #[test]
    fn left_solid_chain() {
        let (w, d) = env();
        let a = mk(&w, &d, 60, 20, 229, None);
        solid(&w, 59, 20, true); // L solid
        solid(&w, 60, 19, true);
        st(&a).update_configuration(&a);
        // AR=(61,21) contents != 101, BL=(59,19) contents != 101 -> highRight=false
        assert!(matches!(cfg(&a), STAIRS_HIGH_LEFT_SOLID | STAIRS_HIGH_LEFT_FLOATING));
        tile(&w, 59, 19).set_contents(CONTENTS_STAIRS);
        st(&a).update_configuration(&a);
        assert!(matches!(cfg(&a), STAIRS_HIGH_RIGHT_SOLID | STAIRS_HIGH_RIGHT_FLOATING));
        // AR is stairs: highRight stays true
        tile(&w, 59, 19).set_contents(0);
        tile(&w, 61, 21).set_contents(CONTENTS_STAIRS);
        st(&a).update_configuration(&a);
        assert!(matches!(cfg(&a), STAIRS_HIGH_RIGHT_SOLID | STAIRS_HIGH_RIGHT_FLOATING));
    }

    #[test]
    fn rand_once_in_every_path_and_roundtrip() {
        let (w, d) = env();
        srand(11);
        let first: Vec<i32> = (0..3).map(|_| rand()).collect();
        srand(11);
        let a = mk(&w, &d, 40, 20, 253, Some("ow"));
        assert_eq!(rand(), first[1]);
        let sd = a.get_save_dict(&a);
        assert_eq!(sd.map.len(), 8);
        srand(11);
        let b = Stairs::init_with_world_dynamic_world_save_dict_cache(&w, &d, &sd, None).unwrap();
        assert_eq!(rand(), first[1]);
        assert_eq!(cfg(&b), cfg(&a));
        let net = a.creation_net_data_for_client(&a, None).unwrap();
        assert!(net[0x1b] == 0 && net[0x1e..0x20].iter().all(|x| *x == 0));
        srand(11);
        let c = Stairs::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &net).unwrap();
        assert_eq!(rand(), first[1]);
        assert!(c.is_net());
        assert_eq!(c.owner_id().as_deref(), Some("ow"));
        assert!(st(&a).is_transparent());
        assert!(!mk(&w, &d, 50, 20, 229, None).as_any().downcast_ref::<Stairs>().unwrap().is_transparent());
    }

    /// Floating stairs on an open back wall drop as a free block; supported ones stay.
    #[test]
    fn floating_without_back_wall_drops() {
        let (w, d) = env();
        let a = mk(&w, &d, 40, 20, 229, None);
        assert_eq!(cfg(&a), STAIRS_HIGH_RIGHT_FLOATING);
        tile(&w, 40, 20).set_contents(CONTENTS_STAIRS);
        a.world_changed(&a, &mut vec![pos(40, 18)]); // below the window: ignored
        assert!(!a.needs_removed());
        a.world_changed(&a, &mut vec![pos(40, 19)]);
        assert!(a.needs_removed());
        assert_eq!(tile(&w, 40, 20).contents(), 0);
        let b = mk(&w, &d, 50, 20, 229, None);
        tile(&w, 50, 20).set_back_wall_type_index(1);
        b.world_changed(&b, &mut vec![pos(50, 19)]);
        assert!(!b.needs_removed());
    }
}

#[cfg(test)]
mod more_tests {
    use super::super::column::fuzz::*;
    use super::super::column::testkit::*;
    use super::*;

    fn mk(w: &Rc<World>, d: &Rc<DynamicWorld>, x: i32, y: i32, item: i32) -> ObjRef {
        Stairs::init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
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
    fn trait_answers_paint_and_net_updates() {
        let (w, d) = env();
        let s = mk(&w, &d, 40, 20, 229);
        let st = handle::downcast::<Stairs>(&s).unwrap();
        let _ = s.class_info();
        assert_eq!(s.freeblock_creation_item_type(), 229);
        assert!(s.free_block_creation_save_dict(&s).is_some());
        st.paint(&s, 5);
        assert_eq!(st.st.borrow().paint_color, 5);
        let mut net = s.update_net_data_for_client(&s, None).unwrap();
        net[0x1a] = (st.st.borrow().current_configuration as u8).wrapping_add(1);
        net[0x1c] = 9;
        s.remote_update(&s, &net);
        assert_eq!((st.st.borrow().current_configuration, st.st.borrow().paint_color), (net[0x1a] as i32, 9));
        s.remote_update(&s, &net);
        // a stair whose own tile is not there
        let gone = mk(&w, &d, 41, 20, 229);
        gone.base().borrow_mut().pos = pos(41, 300);
        handle::downcast::<Stairs>(&gone).unwrap().update_configuration(&gone);
        // a net stair on a tile that is not loaded cannot be created
        let mut lost = net.clone();
        lost[12..16].copy_from_slice(&300u32.to_le_bytes());
        assert!(Stairs::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &lost).is_none());
        // a net stair is not sent back when painted
        let n = Stairs::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &net).unwrap();
        handle::downcast::<Stairs>(&n).unwrap().paint(&n, 2);
    }

    #[test]
    fn ice_stairs_melt() {
        let (w, d) = env();
        let s = mk(&w, &d, 40, 20, ITEM_ICE_STAIRS);
        handle::downcast::<Stairs>(&s).unwrap().st.borrow_mut().ice_melt_timer = 9.0;
        s.update_accurate_dt_is_simulation(&s, 1.0, 1.0, false);
        assert!(s.needs_removed());
        s.update_accurate_dt_is_simulation(&s, 1.0, 1.0, false);
        let plain = mk(&w, &d, 50, 20, 229);
        plain.update_accurate_dt_is_simulation(&plain, 1.0, 1.0, false);
        assert!(!plain.needs_removed());
    }

    #[test]
    fn every_neighbourhood_gives_a_configuration() {
        let (w, d) = env();
        let mut r = Lcg(3);
        let mut seen = std::collections::HashSet::new();
        for y in [0, 1, 30, 62, 63] {
            for _ in 0..400 {
                scramble(&w, &mut r, 40, y, &[0, 0, CONTENTS_STAIRS], &[0]);
                let s = mk(&w, &d, 40, y, 229);
                let st = handle::downcast::<Stairs>(&s).unwrap();
                seen.insert(st.st.borrow().current_configuration);
                if r.next() % 3 == 0 {
                    s.set_needs_removed(&s, true);
                }
                s.world_changed(&s, &mut vec![pos(40 + (r.next() % 5) as i32 - 2, y + (r.next() % 3) as i32 - 1)]);
            }
        }
        assert!(seen.len() >= 4, "{seen:?}");
    }
}
