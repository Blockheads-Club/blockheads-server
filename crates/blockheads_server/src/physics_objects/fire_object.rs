//! `FireObject` (DynamicObject type 16): a burning tile that owns an `ArtificialLight`, counts down a burn
//! time, spreads to its four neighbours on random timers and destroys the burnable contents of its tile when it
//! burns out.
//!
//!  FireObject"
//! (lines 142-253). Client-only ivars (shader, texture, animation loop) and `draw:` are not ported.
//! RNG: libc `rand()` through `rng::glibc_rand`; draw counts and order are in.

use std::any::Any;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::dynworld::state::DynamicWorld;
use crate::lighting::artificial_light::ArtificialLight;
use crate::objects::base::{
    base_dynamic_object_net_data, base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_cache_net_data, base_init_with_world_dynamic_world_save_dict_cache,
    base_remove_from_macro_block, new_mutable_dict, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{alloc_obj, CacheRef, ObjRef};
use crate::plist::value::{PlistDict, PlistValue};
use crate::rng::glibc_rand::{r01_tagged, rand_tagged};
use crate::types::netdata::FireCreationNetData;
use crate::types::structs::{IntPair, WireStruct};
use crate::world::helpers::{tile_is_burnable_block, tile_is_dead_tree, tile_is_plant, tile_is_tree, tile_is_water};
use crate::world::state::{TileRef, World};
use crate::world::tiles::{tile_at_world_position, tile_at_world_position_loaded};

/// `DYNAMIC_OBJECT_TYPE_FIRE`.
pub const DYNAMIC_OBJECT_TYPE_FIRE: i32 = 16;
/// `CONTENTS_LADDER`.
const CONTENTS_LADDER: u8 = 66;
/// `CONTENTS_INTERACTION_OBJECT`.
const CONTENTS_INTERACTION_OBJECT: u8 = 48;
/// `INTERACTION_OBJECT_TYPE_BED`.
const INTERACTION_OBJECT_TYPE_BED: u16 = 3;
/// `ITEM_GOLD_BED`: gold beds survive the fire.
const ITEM_GOLD_BED: i32 = 170;

/// `FireObject: DynamicObject`.
pub static FIRE_OBJECT_CLASS: ClassInfo =
    ClassInfo { name: "FireObject", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &[] };

crate::hooks::hook_trait! {
/// Seam to the plants module (not implemented when this module was written): `removePlantWithoutCreatingFreeblocks`
/// on whichever plant class `getPlantAtPos:` returned. The integrator installs
/// the real dispatcher with [`set_fire_host`]; the default does nothing.
pub trait FireHost, stub FireHostStub {
    /// `-[Plant removePlantWithoutCreatingFreeblocks]`
    fn remove_plant_without_creating_freeblocks(&self, _plant: &ObjRef) {}
}
}

struct NullFireHost;
impl FireHostStub for NullFireHost {}

thread_local! {
    static FIRE_HOST: RefCell<Rc<dyn FireHost>> = RefCell::new(Rc::new(NullFireHost));
}

/// Install the plants seam (see [`FireHost`]).
pub fn set_fire_host(h: Rc<dyn FireHost>) {
    FIRE_HOST.with(|c| *c.borrow_mut() = h);
}

fn fire_host() -> Rc<dyn FireHost> {
    FIRE_HOST.with(|c| c.borrow().clone())
}

/// FireObject ivars (client ones omitted).
pub struct FireState {
    /// 0x50 seconds left
    pub burn_timer: f32,
    /// 0x54..0x60: 0 up (y+1), 1 right (x+1), 2 left (x-1), 3 down (y-1)
    pub spread_timers: [f32; 4],
    /// 0x68 owned
    pub light: Option<Rc<ArtificialLight>>,
}

/// `FireObject`.
pub struct FireObject {
    base: RefCell<DynamicObjectBase>,
    /// The FireObject ivars.
    pub st: RefCell<FireState>,
    world: Weak<World>,
    dw: Weak<DynamicWorld>,
}

/// Downcast helper.
pub fn as_fire_object(o: &ObjRef) -> Option<&FireObject> {
    o.as_any().downcast_ref::<FireObject>()
}

/// `tileIsBurnable(tile, dynamicWorld, pos)` against the real `DynamicWorld` (the helper in `world::helpers` takes
/// the world_state placeholder type).
fn tile_is_burnable(t: Option<TileRef>, dw: &Rc<DynamicWorld>, pos: IntPair) -> bool {
    if let Some(o) = dw.interaction_object_at_pos(pos) {
        if let Some(io) = o.as_interaction_like() {
            if io.interaction_object_type() == INTERACTION_OBJECT_TYPE_BED && dw.hooks().item_type(&o) != ITEM_GOLD_BED
            {
                return true;
            }
        }
    }
    match &t {
        Some(tile) => {
            !tile_is_water(t.clone())
                && (tile_is_tree(t.clone())
                    || tile_is_dead_tree(t.clone())
                    || tile_is_burnable_block(tile.clone())
                    || tile.foreground_contents() == 0x42
                    || tile_is_plant(t.clone()))
        }
        None => false,
    }
}

impl FireObject {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<FireObject> {
        let (w, d) = (Rc::downgrade(world), Rc::downgrade(dw));
        alloc_obj(|sw| FireObject {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            st: RefCell::new(FireState { burn_timer: 0.0, spread_timers: [0.0; 4], light: None }),
            world: w,
            dw: d,
        })
    }

    fn world(&self) -> Rc<World> {
        self.world.upgrade().expect("FireObject: world dropped")
    }

    fn dw(&self) -> Rc<DynamicWorld> {
        self.dw.upgrade().expect("FireObject: dynamic world dropped")
    }

    fn pos(&self) -> IntPair {
        self.base.borrow().pos
    }

    fn light(&self) -> Option<Rc<ArtificialLight>> {
        self.st.borrow().light.clone()
    }

    /// `-initSubDerivedItems`: `animationLoopIndex = rand() % 8` (C signed modulo; value discarded on the
    /// server).
    fn init_sub_derived_items(&self) {
        let _animation_loop_index = rand_tagged("fire::anim_loop") % 8;
    }

    /// `-initWithWorld:dynamicWorld:atPosition:cache:`: 5 `rand()` draws (burn, 4 spread timers), the light,
    /// then 1 more draw.
    pub fn new(world: &Rc<World>, dw: &Rc<DynamicWorld>, pos: IntPair, cache: CacheRef) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        let dref: DynamicWorldRef = dw.clone();
        if !base_init_with_world_dynamic_world_at_position_cache(&me_obj, &wref, &dref, pos, cache.clone()) {
            return None;
        }
        {
            let mut st = me.st.borrow_mut();
            st.burn_timer = r01_tagged("fire::burn") * 5.0f32 + 10.0f32;
            st.spread_timers[0] = r01_tagged("fire::spread0") * 1.0f32 + 2.0f32;
            st.spread_timers[1] = r01_tagged("fire::spread1") * 5.0f32 + 5.0f32;
            st.spread_timers[2] = r01_tagged("fire::spread2") * 5.0f32 + 5.0f32;
            st.spread_timers[3] = r01_tagged("fire::spread3") * 5.0f32 + 10.0f32;
        }
        let p = me.pos();
        let light = ArtificialLight::init_with_world_dynamic_world_at_position_cache_parent_object_color_r_color_g_color_b_heat_radius_light_direction(
            world,
            &dref,
            p,
            cache,
            Some(&me_obj),
            0x80,
            0x40,
            1,
            0x14,
            8,
            0,
        );
        me.st.borrow_mut().light = light;
        me.init_sub_derived_items();
        Some(me_obj)
    }

    /// `-initWithWorld:dynamicWorld:saveDict:cache:`: one `rand() % 8`. A missing `lightDict` is passed as
    /// an empty dictionary (messages to nil read as 0).
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
        if !base_init_with_world_dynamic_world_save_dict_cache(&me_obj, &wref, &dref, save_dict, cache.clone()) {
            return None;
        }
        let fv = |k: &str| save_dict.map.get(&k.to_string()).map_or(0.0f32, |v| v.float_value());
        {
            let mut st = me.st.borrow_mut();
            st.burn_timer = fv("burnTimer");
            st.spread_timers = [fv("spreadTimer_0"), fv("spreadTimer_1"), fv("spreadTimer_2"), fv("spreadTimer_3")];
        }
        let light_dict = match save_dict.map.get(&"lightDict".to_string()) {
            Some(PlistValue::Dict(d)) => d.clone(),
            _ => new_mutable_dict(),
        };
        let light = ArtificialLight::init_with_world_dynamic_world_save_dict_cache_parent_object(
            world,
            &dref,
            &light_dict,
            cache,
            Some(&me_obj),
        );
        me.st.borrow_mut().light = light;
        me.init_sub_derived_items();
        Some(me_obj)
    }

    /// `-initWithWorld:dynamicWorld:cache:netData:`: no light (client mirror); one `rand() % 8`.
    pub fn from_net_data(world: &Rc<World>, dw: &Rc<DynamicWorld>, cache: CacheRef, net_data: &[u8]) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        let dref: DynamicWorldRef = dw.clone();
        if !base_init_with_world_dynamic_world_cache_net_data(&me_obj, &wref, &dref, cache, net_data) {
            return None;
        }
        me.init_sub_derived_items();
        Some(me_obj)
    }

    /// `burnOut()` of `update`: removed, flagged, change notification.
    fn burn_out(&self, me: &ObjRef) {
        me.set_needs_removed(me, true);
        self.base.borrow_mut().update_needs_to_be_sent = true;
        self.dw().dynamic_world_changed_at_pos_object_type(self.pos(), DYNAMIC_OBJECT_TYPE_FIRE);
    }

    /// `-update:accurateDT:isSimulation:`.
    fn update_impl(&self, me: &ObjRef, dt: f32) {
        if me.is_net() || me.needs_removed() {
            return;
        }
        let world = self.world();
        let dw = self.dw();
        let macro_tiles = world.macro_tiles();
        let p = self.pos();
        self.st.borrow_mut().burn_timer -= dt;
        let burn = self.st.borrow().burn_timer;
        if 0.0f32 < burn {
            let tile = tile_at_world_position(p.x, p.y, &macro_tiles, &world);
            if !tile_is_burnable(tile, &dw, p) {
                self.burn_out(me);
                return;
            }
            // (neighbour offset, reset base, reset scale, requires on_fire == 0)
            let spec: [(IntPair, f32, f32, bool); 4] = [
                (IntPair { x: p.x, y: p.y + 1 }, 2.0f32, 1.0f32, false),
                (IntPair { x: p.x + 1, y: p.y }, 5.0f32, 5.0f32, true),
                (IntPair { x: p.x - 1, y: p.y }, 5.0f32, 5.0f32, true),
                (IntPair { x: p.x, y: p.y - 1 }, 10.0f32, 5.0f32, true),
            ];
            for (i, (np, add, scale, needs_not_on_fire)) in spec.iter().enumerate() {
                let t = {
                    let mut st = self.st.borrow_mut();
                    st.spread_timers[i] -= dt;
                    st.spread_timers[i]
                };
                // `t <= 0.0f && t != 0.0f` (equals `t < 0` for non-NaN)
                if t <= 0.0f32 && t != 0.0f32 {
                    let u = r01_tagged("fire::spread_reset");
                    self.st.borrow_mut().spread_timers[i] = u * *scale + *add;
                    let n = tile_at_world_position(np.x, np.y, &macro_tiles, &world);
                    if tile_is_burnable(n.clone(), &dw, *np) {
                        // UB-PARITY: the original dereferences a NULL `n` here; None is treated as `on_fire == 0`.
                        if !*needs_not_on_fire || n.as_ref().map_or(0, |t| t.on_fire()) == 0 {
                            let _ = dw.place_fire_at_position(*np);
                        }
                    }
                }
            }
        } else {
            self.burn_out(me);
        }
    }
}

impl DynamicObject for FireObject {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &FIRE_OBJECT_CLASS
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    /// `-objectType`
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_FIRE
    }
    /// `-getLightRGB`
    fn get_light_rgb(&self) -> [f32; 3] {
        [128.0, 64.0, 1.0]
    }
    /// `-requiresPhysicalBlock`: `!isNet`.
    fn requires_physical_block(&self) -> bool {
        !self.base.borrow().is_net
    }
    /// `-getSaveDict`
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = base_get_save_dict(me);
        {
            let st = self.st.borrow();
            d.map.insert("burnTimer".to_string(), PlistValue::real_f32(st.burn_timer));
            for i in 0..4 {
                d.map.insert(format!("spreadTimer_{i}"), PlistValue::real_f32(st.spread_timers[i]));
            }
        }
        if let Some(l) = self.light() {
            let lo: ObjRef = l;
            let ld = lo.get_save_dict(&lo);
            d.map.insert("lightDict".to_string(), PlistValue::Dict(ld));
        }
        d
    }
    /// `-updateNetDataForClient:` = creation data.
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        self.creation_net_data_for_client(me, client_id)
    }
    /// `-creationNetDataForClient:`: the 0x18-byte `DynamicObjectNetData`.
    fn creation_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        Some(FireCreationNetData { dynamic_object_net_data: base_dynamic_object_net_data(me) }.to_vec())
    }
    /// `-dealloc`: `[light release]`.
    fn dealloc(&self, _me: &ObjRef) {
        let light = self.st.borrow_mut().light.take();
        if let Some(l) = light {
            l.dealloc_light();
        }
    }
    /// `-setNeedsRemoved:`
    fn set_needs_removed(&self, _me: &ObjRef, v: bool) {
        self.base.borrow_mut().needs_removed = v;
        if v {
            if let Some(l) = self.light() {
                l.remove_from_tiles();
            }
        }
    }
    /// `-removeFromMacroBlock`.
    fn remove_from_macro_block(&self, me: &ObjRef) {
        let light = self.st.borrow_mut().light.take();
        if let Some(l) = light {
            l.remove_from_macro_block_light();
            l.dealloc_light();
        }
        let world = self.world();
        let dw = self.dw();
        let p = self.pos();
        // loads the block; the block holding a fire object is loaded (None would crash the original: UB-PARITY)
        // Order per rule: on_fire cleared BEFORE worldChanged.
        let tile = tile_at_world_position_loaded(p.x, p.y, &world);
        if let Some(t) = &tile {
            t.set_on_fire(0);
        }
        dw.world_changed_at_pos_send_reliably(p, true);
        if let Some(tile) = tile {
            let fg = tile.foreground_contents();
            if fg != 0 {
                if fg == CONTENTS_LADDER {
                    let _ = dw.remove_ladder_at_pos(p);
                } else if tile_is_plant(Some(tile.clone())) {
                    if let Some(plant) = dw.get_plant_at_pos(p) {
                        fire_host().remove_plant_without_creating_freeblocks(&plant);
                    }
                }
            }
            if tile.contents() == CONTENTS_INTERACTION_OBJECT {
                if let Some(io) = dw.interaction_object_at_pos(p) {
                    if let Some(il) = io.as_interaction_like() {
                        if il.interaction_object_type() == INTERACTION_OBJECT_TYPE_BED
                            && il.destroy_item_type() != ITEM_GOLD_BED
                        {
                            let _ = dw.remove_interaction_object_at_pos_remove_blockhead(p, None);
                        }
                    }
                }
            }
            if tile_is_tree(Some(tile.clone())) || tile_is_burnable_block(tile.clone()) {
                world.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents(
                    p.x, p.y, 0, 0, None, false, false,
                );
            }
        }
        base_remove_from_macro_block(me);
    }
    /// `-update:accurateDT:isSimulation:`
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, _accurate_dt: f32, _is_simulation: bool) {
        self.update_impl(me, dt);
    }
    /// `-worldChanged:`
    fn world_changed(&self, _me: &ObjRef, positions: &mut Vec<IntPair>) {
        if let Some(l) = self.light() {
            l.world_changed_positions(positions);
        }
    }
    /// `-addArtificialLightContributionForPhysicalBlockLoadedAtXPos:yPos:`
    fn add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(
        &self,
        _me: &ObjRef,
        macro_x: i32,
        macro_y: i32,
    ) {
        if let Some(l) = self.light() {
            l.add_contribution_for_physical_block_loaded_at_x_pos_y_pos(macro_x, macro_y);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::artificial_light::testkit::tile;
    use crate::physics_objects::free_block::testkit::flat;
    use crate::rng::glibc_rand;

    fn fire(o: &ObjRef) -> &FireObject {
        as_fire_object(o).unwrap()
    }

    /// : six draws at creation, timer ranges.
    #[test]
    fn creation_draws_and_ranges() {
        let (w, dw, _r) = flat("fire_ctor", 5);
        glibc_rand::srand(3);
        let o = FireObject::new(&w, &dw, IntPair { x: 10, y: 6 }, None).unwrap();
        let st = fire(&o).st.borrow();
        assert!((10.0..=15.0).contains(&st.burn_timer));
        assert!((2.0..=3.0).contains(&st.spread_timers[0]));
        assert!((5.0..=10.0).contains(&st.spread_timers[1]));
        assert!((5.0..=10.0).contains(&st.spread_timers[2]));
        assert!((10.0..=15.0).contains(&st.spread_timers[3]));
        assert!(st.light.is_some());
        drop(st);
        // exactly six draws: the seventh matches a fresh stream
        glibc_rand::srand(3);
        for _ in 0..6 {
            glibc_rand::rand();
        }
        let seventh = glibc_rand::rand();
        glibc_rand::srand(3);
        let _o2 = FireObject::new(&w, &dw, IntPair { x: 14, y: 6 }, None).unwrap();
        assert_eq!(glibc_rand::rand(), seventh);
        assert_eq!(o.object_type(), 16);
        assert_eq!(o.get_light_rgb(), [128.0, 64.0, 1.0]);
        assert!(o.requires_physical_block());
    }

    /// Save round trip keeps the timers and the light; creation blob is the 0x18-byte net data.
    #[test]
    fn save_round_trip() {
        let (w, dw, _r) = flat("fire_save", 5);
        let o = FireObject::new(&w, &dw, IntPair { x: 10, y: 6 }, None).unwrap();
        let d = o.get_save_dict(&o);
        for k in ["burnTimer", "spreadTimer_0", "spreadTimer_1", "spreadTimer_2", "spreadTimer_3", "lightDict"] {
            assert!(d.map.get(&k.to_string()).is_some(), "missing {k}");
        }
        glibc_rand::srand(1);
        let r = FireObject::from_save_dict(&w, &dw, &d, None).unwrap();
        assert_eq!(fire(&r).st.borrow().burn_timer, fire(&o).st.borrow().burn_timer);
        assert_eq!(fire(&r).st.borrow().spread_timers, fire(&o).st.borrow().spread_timers);
        glibc_rand::srand(1);
        glibc_rand::rand();
        let second = glibc_rand::rand();
        glibc_rand::srand(1);
        let _ = FireObject::from_save_dict(&w, &dw, &d, None).unwrap();
        assert_eq!(glibc_rand::rand(), second);
        let c = o.creation_net_data_for_client(&o, None).unwrap();
        assert_eq!(c.len(), 0x18);
        assert_eq!(o.update_net_data_for_client(&o, None).unwrap(), c);
        let n = FireObject::from_net_data(&w, &dw, None, &c).unwrap();
        assert!(n.is_net());
        assert!(fire(&n).light().is_none());
        assert!(!n.requires_physical_block());
        // net objects do not simulate
        n.update_accurate_dt_is_simulation(&n, 100.0, 100.0, false);
        assert!(!n.needs_removed());
    }

    /// Burn-out when the tile is not burnable, and when the burn timer ends.
    #[test]
    fn burn_out_rules() {
        let (w, dw, _r) = flat("fire_out", 5);
        let o = FireObject::new(&w, &dw, IntPair { x: 10, y: 6 }, None).unwrap();
        // tile (10,6) is air: not burnable
        o.update_accurate_dt_is_simulation(&o, 0.2, 0.2, false);
        assert!(o.needs_removed());
        assert!(o.update_needs_to_be_sent());
        let t0 = tile(&w, 10, 6);
        assert_eq!(t0.artificial_light_r(), 0);
        // burnable tile: stays; each expired spread timer consumes one draw
        tile(&w, 20, 6).update(|t| t.type_index = 9);
        let o2 = FireObject::new(&w, &dw, IntPair { x: 20, y: 6 }, None).unwrap();
        fire(&o2).st.borrow_mut().spread_timers = [0.1, 100.0, 100.0, 100.0];
        glibc_rand::srand(9);
        o2.update_accurate_dt_is_simulation(&o2, 0.2, 0.2, false);
        assert!(!o2.needs_removed());
        let t = fire(&o2).st.borrow().spread_timers[0];
        let r = (1804289383i32 as f32) / 2147483648.0f32;
        glibc_rand::srand(1);
        let r1 = (glibc_rand::rand() as f32) / 2147483648.0f32;
        let _ = r;
        glibc_rand::srand(9);
        let r9 = (glibc_rand::rand() as f32) / 2147483648.0f32;
        assert_eq!(t, r9 * 1.0f32 + 2.0f32);
        assert_ne!(r9, r1);
        // burn timer expiry
        fire(&o2).st.borrow_mut().burn_timer = 0.1;
        o2.update_accurate_dt_is_simulation(&o2, 0.2, 0.2, false);
        assert!(o2.needs_removed());
    }

    /// Removal clears `on_fire` and removes a burnable block.
    #[test]
    fn remove_clears_tile() {
        let (w, dw, _r) = flat("fire_rm", 5);
        tile(&w, 30, 6).update(|t| {
            t.type_index = 9;
            t.on_fire = 1;
        });
        let o = FireObject::new(&w, &dw, IntPair { x: 30, y: 6 }, None).unwrap();
        o.remove_from_macro_block(&o);
        assert_eq!(tile(&w, 30, 6).on_fire(), 0);
    }
}
