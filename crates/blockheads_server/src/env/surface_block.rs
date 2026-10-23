//! `SurfaceBlock` (DynamicObject type 22, instance size 0x58): flowing-water simulation per active tile.
//!
//!  SurfaceBlock" (4.1-4.3.9);
//! sections 3.5, 6 (R5) and 7 item 8. Never saved or sent (no `getSaveDict` override). The lava branch keeps its
//! particle `rand()` draws (4 per particle, in the order x, y, vx, vy) even though the emitter is a no-op.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::env::particle_emitter::ParticleEmitter;
use crate::fmath::cconv::f32_to_i32;
use crate::fmath::vector::Vector;
use crate::lighting::artificial_light::services;
use crate::objects::base::{
    base_init_with_world_dynamic_world_at_position_cache, base_init_with_world_dynamic_world_save_dict_cache,
    DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::plist::value::PlistDict;
use crate::rng::glibc_rand::r01_from;
use crate::types::structs::IntPair;
use crate::world::helpers::{tile_is_air_or_snow, tile_is_solid, tile_is_water};
use crate::world::state::{TileRef, World};
use crate::world::tiles::tile_at_world_position_loaded;

/// `SurfaceBlock` class descriptor.
pub static SURFACE_BLOCK_CLASS: ClassInfo =
    ClassInfo { name: "SurfaceBlock", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &[] };

/// `DYNAMIC_OBJECT_TYPE_SURFACEBLOCK`
pub const DYNAMIC_OBJECT_TYPE_SURFACEBLOCK: i32 = 22;

/// `debugX` / `debugY` bss globals: both 0, never written. A block at tile (0,0) toggles `pingPong` twice.
const DEBUG_X: i32 = 0;
const DEBUG_Y: i32 = 0;

const ITEM_BASALT_UNMINED: i32 = 0x428;
const ITEM_WATER: i32 = 0x414;

/// SurfaceBlock ivars 0x50..0x58.
#[derive(Default)]
pub struct SurfaceBlockState {
    /// `lastSaveContent` (0x50)
    pub last_save_content: i32,
    /// `pingPong` (0x54)
    pub ping_pong: bool,
    /// `currentlyRequiresPhysicalBlock` (0x55)
    pub currently_requires_physical_block: bool,
    /// `removeNextFrame` (0x56)
    pub remove_next_frame: bool,
    /// `removeNextFrameEmpty` (0x57)
    pub remove_next_frame_empty: bool,
}

/// `SurfaceBlock: DynamicObject` (type 22).
pub struct SurfaceBlock {
    base: RefCell<DynamicObjectBase>,
    /// The SurfaceBlock ivars.
    pub st: RefCell<SurfaceBlockState>,
    world: Weak<World>,
}

impl SurfaceBlock {
    fn alloc(world: &Rc<World>) -> Rc<SurfaceBlock> {
        let w = Rc::downgrade(world);
        handle::alloc_obj(|sw| SurfaceBlock {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            st: RefCell::new(SurfaceBlockState::default()),
            world: w,
        })
    }

    fn world(&self) -> Rc<World> {
        self.world.upgrade().expect("SurfaceBlock: world dropped")
    }

    fn pos(&self) -> IntPair {
        self.base.borrow().pos
    }

    /// `-[SurfaceBlock initWithWorld:dynamicWorld:atPosition:cache:]`: ONE `rand()`.
    pub fn init_with_world_dynamic_world_at_position_cache(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        pos: IntPair,
        cache: CacheRef,
    ) -> Option<Rc<SurfaceBlock>> {
        let me = Self::alloc(world);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        // The base init always succeeds for a new object (it ignores a failed registration).
        let _ = base_init_with_world_dynamic_world_at_position_cache(&me_obj, &wref, dw, pos, cache);
        let mut st = me.st.borrow_mut();
        st.last_save_content = 0;
        st.ping_pong = crate::rng_rand!() > 0x3fffffff;
        drop(st);
        Some(me)
    }

    /// `-[SurfaceBlock initWithWorld:dynamicWorld:saveDict:cache:]`: `pingPong` stays NO.
    pub fn init_with_world_dynamic_world_save_dict_cache(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<Rc<SurfaceBlock>> {
        let me = Self::alloc(world);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        // The base init always succeeds for a saved object.
        let _ = base_init_with_world_dynamic_world_save_dict_cache(&me_obj, &wref, dw, save_dict, cache);
        Some(me)
    }

    /// `-[SurfaceBlock subtractWater:fromOtherTile:atPos:]`
    pub fn subtract_water_from_other_tile_at_pos(&self, amount: i32, other_tile: &TileRef, other_pos: IntPair) {
        if amount != 0 {
            let v = other_tile.partial_content_left().wrapping_sub(amount as u8);
            other_tile.set_partial_content_left(v);
            services().load_surface_block_at_pos(other_pos);
        }
    }

    /// `-[SurfaceBlock takeAnyWaterFromTileAtPos:tile:]`. `tile` is this block's own tile.
    pub fn take_any_water_from_tile_at_pos_tile(&self, other_pos: IntPair, tile: &TileRef) -> bool {
        if self.base.borrow().needs_removed {
            return false;
        }
        let world = self.world();
        let pos = self.pos();
        let sv = services();
        let other = tile_at_world_position_loaded(other_pos.x, other_pos.y, &world);
        if !tile_is_water(other) {
            return false;
        }
        let other = other.unwrap();
        let mut multiplier = 1.0f32;
        let difference: i32;
        if pos.y < other_pos.y {
            // source is above
            let a = other.partial_content_left() as i32;
            let b = 255 - tile.partial_content_left() as i32;
            difference = if a < b { a } else { b };
        } else {
            let a = other.partial_content_left() as i32 - tile.partial_content_left() as i32;
            let b = 255 - tile.partial_content_left() as i32;
            difference = if a < b { a } else { b };
            multiplier = 0.4f32;
        }
        if difference <= 0 {
            return false;
        }
        let mut f = (difference as f32) * multiplier;
        if f <= 1.0f32 {
            f = 1.0f32;
        }
        let amount = f32_to_i32(f);
        self.subtract_water_from_other_tile_at_pos(amount, &other, other_pos);
        if tile.type_index() != 3 {
            sv.water_changed_at_pos_full_block(pos, true);
        }
        let neighbours = [
            IntPair { x: pos.x, y: pos.y - 1 },
            IntPair { x: pos.x - 1, y: pos.y },
            IntPair { x: pos.x + 1, y: pos.y },
        ];
        if (tile.partial_content_left() as i32) + amount < 0xfe {
            if !tile_is_water(Some(*tile)) {
                tile.set_type_index(3);
                tile.set_partial_content_left(amount as u8);
            } else {
                tile.set_partial_content_left(tile.partial_content_left().wrapping_add(amount as u8));
            }
            if tile.partial_content_left() > 1 {
                for n in neighbours {
                    if tile_is_air_or_snow(tile_at_world_position_loaded(n.x, n.y, &world)) {
                        sv.load_surface_block_at_pos(n);
                    }
                }
            }
        } else {
            tile.set_partial_content_left(0xff);
            self.st.borrow_mut().remove_next_frame = true;
            if !tile_is_water(Some(*tile)) {
                tile.set_type_index(3);
            }
            for n in neighbours {
                if tile_is_air_or_snow(tile_at_world_position_loaded(n.x, n.y, &world)) {
                    sv.load_surface_block_at_pos(n);
                }
            }
        }
        sv.water_moved_from_from_tile_to_to_tile_amount(&world, other_pos, other, pos, *tile, amount);
        sv.water_changed_at_pos_full_block(pos, false);
        sv.water_changed_at_pos_full_block(other_pos, false);
        self.st.borrow_mut().currently_requires_physical_block = true;
        true
    }

    /// `-[SurfaceBlock removeIfFloatingAndEmptyOfWater]`: two-strike removal.
    pub fn remove_if_floating_and_empty_of_water(&self) {
        let world = self.world();
        let pos = self.pos();
        let t = tile_at_world_position_loaded(pos.x, pos.y, &world);
        let clear = |s: &SurfaceBlock| s.st.borrow_mut().remove_next_frame_empty = false;
        if tile_is_water(t) && t.unwrap().partial_content_left() != 0 {
            clear(self);
            return;
        }
        let a = tile_at_world_position_loaded(pos.x, pos.y + 1, &world);
        if a.is_none() || (tile_is_water(a) && a.unwrap().partial_content_left() != 0) {
            clear(self);
            return;
        }
        let l = tile_at_world_position_loaded(pos.x - 1, pos.y, &world);
        if l.is_none() || (tile_is_water(l) && l.unwrap().partial_content_left() > 1) {
            clear(self);
            return;
        }
        let r = tile_at_world_position_loaded(pos.x + 1, pos.y, &world);
        if r.is_none() || (tile_is_water(r) && r.unwrap().partial_content_left() > 1) {
            clear(self);
            return;
        }
        let first_strike = !self.st.borrow().remove_next_frame_empty;
        if first_strike {
            self.st.borrow_mut().remove_next_frame_empty = true;
        } else {
            let me = self.me();
            self.set_needs_removed(&me, true);
            // ![dynamicWorld isClient]
            services().remove_water_tile_at_pos(&world, pos);
        }
    }

    /// The lava steam / spark particles: `n` particles x 4 `rand()` draws (x, y, vx, vy), then the (empty) emitter call.
    fn lava_particles(&self, n: i32) {
        let pos = self.pos();
        let emitter = ParticleEmitter::instance();
        for _ in 0..n {
            let r1 = r01_from(crate::rng_rand!());
            let r2 = r01_from(crate::rng_rand!());
            let r3 = r01_from(crate::rng_rand!());
            let r4 = r01_from(crate::rng_rand!());
            let xoff = r1 - 0.5f32;
            let yoff = r2 - 0.5f32;
            let vx = r3 - 0.5f32;
            let vy = r4 + r4;
            // dayColorForPosition: constant white on the server; the colour only feeds the dropped particle
            emitter.add_particle_at_pos_velocity_color_gravity_type_life_scale(
                Vector::new((pos.x as f32) + 0.5f32 + xoff, (pos.y as f32) + yoff, -0.5f32),
                Vector::new(vx, vy, 0.0f32),
                Vector::new4(0.8078f32, 0.8549f32, 0.8863f32, 1.0f32),
                3,
                8.0f32,
                0.0f32,
            );
        }
    }
}

impl SurfaceBlock {
    /// `-[DynamicObject initWithWorld:dynamicWorld:cache:netData:]` inherited unchanged (SurfaceBlock defines no net init; object_registry A3).
    /// Zero-state species ivars + the DynamicObject net init. `None` when the base init yields nil.
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<Rc<SurfaceBlock>> {
        let me = Self::alloc(world);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        if !crate::objects::base::base_init_with_world_dynamic_world_cache_net_data(&me_obj, &wref, dw, cache, net_data)
        {
            return None;
        }
        Some(me)
    }
}

impl DynamicObject for SurfaceBlock {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &SURFACE_BLOCK_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_SURFACEBLOCK
    }
    fn requires_physical_block(&self) -> bool {
        self.st.borrow().currently_requires_physical_block
    }

    /// : `dt` / `accurateDT` / `isSimulation` are unused.
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, _dt: f32, _accurate_dt: f32, _is_simulation: bool) {
        let pos = self.pos();
        if pos.x == DEBUG_X && pos.y == DEBUG_Y {
            let mut st = self.st.borrow_mut();
            st.ping_pong = !st.ping_pong;
        }
        if self.base.borrow().needs_removed {
            return;
        }
        let was_remove_next_frame = self.st.borrow().remove_next_frame;
        let world = self.world();
        let sv = services();
        let Some(tile) = tile_at_world_position_loaded(pos.x, pos.y, &world) else { return };
        if tile_is_solid(Some(tile)) {
            self.set_needs_removed(me, true);
            return;
        }
        let below = tile_at_world_position_loaded(pos.x, pos.y - 1, &world);
        let above = tile_at_world_position_loaded(pos.x, pos.y + 1, &world);
        let (Some(below), Some(above)) = (below, above) else { return };

        // --- lava interaction ---
        if below.type_index() == 31 {
            if tile.partial_content_left() < 0x7f {
                let p = tile.partial_content_left();
                tile.set_partial_content_left(if p < 5 { 0 } else { p.wrapping_add(0xfc) });
                self.lava_particles(2);
            } else {
                tile.set_partial_content_left(0);
                sv.fill_tile_at_pos_with_type(&world, tile, pos, ITEM_BASALT_UNMINED);
                self.lava_particles(16);
            }
        }

        {
            let mut st = self.st.borrow_mut();
            st.ping_pong = !st.ping_pong;
        }
        let mut is_static = true;
        if tile_is_water(Some(above)) {
            is_static = !self.take_any_water_from_tile_at_pos_tile(IntPair { x: pos.x, y: pos.y + 1 }, &tile);
        }
        for offset in [-1i32, 1] {
            let o = if self.st.borrow().ping_pong { -offset } else { offset };
            let side = tile_at_world_position_loaded(pos.x + o, pos.y, &world);
            if tile_is_water(side)
                && (tile.partial_content_left() as i32 + 1) < side.unwrap().partial_content_left() as i32
            {
                if self.take_any_water_from_tile_at_pos_tile(IntPair { x: pos.x + o, y: pos.y }, &tile) {
                    is_static = false;
                }
            }
        }
        if is_static && tile.partial_content_left() == 1 {
            tile.set_partial_content_left(0);
        }
        let remove_next_frame = self.st.borrow().remove_next_frame;
        if !(remove_next_frame && was_remove_next_frame && tile.partial_content_left() == 0xff) {
            self.remove_if_floating_and_empty_of_water();
            self.st.borrow_mut().remove_next_frame = false;
        } else {
            self.set_needs_removed(me, true);
            sv.fill_tile_at_pos_with_type(&world, tile, pos, ITEM_WATER);
        }
        if is_static {
            self.st.borrow_mut().currently_requires_physical_block = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::artificial_light::testkit::*;
    use crate::rng::glibc_rand::{rand, srand};

    fn mk(world: &Rc<World>, x: i32, y: i32) -> (Rc<SurfaceBlock>, ObjRef) {
        let (_d, dwr) = test_dw();
        let b =
            SurfaceBlock::init_with_world_dynamic_world_at_position_cache(world, &dwr, IntPair { x, y }, None).unwrap();
        let o: ObjRef = b.clone();
        (b, o)
    }

    fn water(w: &Rc<World>, x: i32, y: i32, pcl: u8) {
        tile(w, x, y).update(|t| {
            t.type_index = 3;
            t.partial_content_left = pcl;
        });
    }

    fn setup() -> Rc<World> {
        let w = world_with_blocks(4, &[(0, 0), (1, 0), (0, 1), (1, 1)]);
        // floor at y=39
        for x in 30..60 {
            tile(&w, x, 39).update(|t| t.type_index = 1);
        }
        w
    }

    #[test]
    fn init_draws_exactly_one_rand() {
        install_rec();
        let w = setup();
        srand(99);
        let first = rand();
        srand(99);
        let (b, _o) = mk(&w, 40, 40);
        assert_eq!(b.st.borrow().ping_pong, first > 0x3fffffff);
        assert_eq!(rand(), {
            srand(99);
            rand();
            rand()
        });
        assert_eq!(b.object_type(), 22);
    }

    #[test]
    fn pull_from_above_table() {
        let rec = install_rec();
        let w = setup();
        let (b, o) = mk(&w, 40, 40);
        water(&w, 40, 41, 100);
        let t = tile(&w, 40, 40);
        // above: min(100, 255-0) = 100, multiplier 1.0 -> amount 100; tile is air -> becomes water pcl 100
        assert!(b.take_any_water_from_tile_at_pos_tile(IntPair { x: 40, y: 41 }, &t));
        assert_eq!((t.type_index(), t.partial_content_left()), (3, 100));
        assert_eq!(tile(&w, 40, 41).get().partial_content_left, 0);
        assert!(b.requires_physical_block());
        let log = rec.take();
        assert!(log.contains(&"waterChanged(40,40,true)".to_string()));
        assert!(log.contains(&"waterMoved(40,41->40,40 100)".to_string()));
        assert!(log.contains(&"loadSurface(40,41)".to_string()));
        // below (40,39) is solid: not an air tile -> no surface block loads for it; (39,40),(41,40) are air
        assert!(log.contains(&"loadSurface(39,40)".to_string()) && log.contains(&"loadSurface(41,40)".to_string()));
        assert!(!log.contains(&"loadSurface(40,39)".to_string()));
        // flagged removed: no more pulls
        o.set_needs_removed(&o, true);
        assert!(!b.take_any_water_from_tile_at_pos_tile(IntPair { x: 40, y: 41 }, &t));
    }

    #[test]
    fn pull_from_side_and_full_split() {
        install_rec();
        let w = setup();
        let (b, _o) = mk(&w, 40, 40);
        let t = tile(&w, 40, 40);
        water(&w, 41, 40, 200);
        // side: min(200-0, 255) = 200 * 0.4 = 80
        assert!(b.take_any_water_from_tile_at_pos_tile(IntPair { x: 41, y: 40 }, &t));
        assert_eq!(t.partial_content_left(), 80);
        assert_eq!(tile(&w, 41, 40).get().partial_content_left, 120);
        // small difference: f <= 1.0 -> amount 1
        water(&w, 39, 40, 82);
        assert!(b.take_any_water_from_tile_at_pos_tile(IntPair { x: 39, y: 40 }, &t));
        assert_eq!(t.partial_content_left(), 81);
        // equal level: difference <= 0 -> NO
        water(&w, 39, 40, 81);
        assert!(!b.take_any_water_from_tile_at_pos_tile(IntPair { x: 39, y: 40 }, &t));
        // reaching >= 0xfe: becomes 0xff + removeNextFrame
        t.set_partial_content_left(200);
        water(&w, 40, 41, 100);
        assert!(b.take_any_water_from_tile_at_pos_tile(IntPair { x: 40, y: 41 }, &t));
        // difference = min(100, 55) = 55 -> 200+55 = 255 >= 0xfe
        assert_eq!(t.partial_content_left(), 0xff);
        assert!(b.st.borrow().remove_next_frame);
        // non-water source: NO
        tile(&w, 41, 40).update(|x| x.type_index = 2);
        assert!(!b.take_any_water_from_tile_at_pos_tile(IntPair { x: 41, y: 40 }, &t));
    }

    #[test]
    fn update_flow_ping_pong_and_removal() {
        let rec = install_rec();
        let w = setup();
        let (b, o) = mk(&w, 40, 40);
        b.st.borrow_mut().ping_pong = false;
        water(&w, 40, 41, 50);
        o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false);
        // pingPong toggled once, water pulled from above
        assert!(b.st.borrow().ping_pong);
        assert_eq!(tile(&w, 40, 40).get().partial_content_left, 50);
        assert!(b.requires_physical_block());
        rec.take();
        // static run: nothing to pull; pcl 50 not touched; stays
        o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false);
        assert!(!b.requires_physical_block());
        // empty tile with all-clear neighbours: two strikes, then removal and removeWaterTile
        let (b2, o2) = mk(&w, 50, 40);
        o2.update_accurate_dt_is_simulation(&o2, 0.1, 0.1, false);
        assert!(!o2.needs_removed());
        assert!(b2.st.borrow().remove_next_frame_empty);
        rec.take();
        o2.update_accurate_dt_is_simulation(&o2, 0.1, 0.1, false);
        assert!(o2.needs_removed());
        assert!(rec.take().contains(&"removeWater(50,40)".to_string()));
        // solid own tile -> removal
        let (_b3, o3) = mk(&w, 55, 40);
        tile(&w, 55, 40).update(|t| t.type_index = 1);
        o3.update_accurate_dt_is_simulation(&o3, 0.1, 0.1, false);
        assert!(o3.needs_removed());
    }

    #[test]
    fn lava_rng_draw_counts_and_basalt() {
        let rec = install_rec();
        let w = setup();
        // lava below (type 31)
        tile(&w, 40, 39).update(|t| t.type_index = 31);
        let (b, o) = mk(&w, 40, 40);
        water(&w, 40, 40, 100); // < 0x7f: minus 4, 2 particles = 8 draws
        srand(5);
        o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false);
        assert_eq!(tile(&w, 40, 40).get().partial_content_left, 96);
        let after = rand();
        srand(5);
        for _ in 0..8 {
            rand();
        }
        assert_eq!(after, rand());
        // < 5 -> 0
        tile(&w, 40, 40).set_partial_content_left(3);
        b.st.borrow_mut().ping_pong = false;
        o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false);
        assert_eq!(tile(&w, 40, 40).get().partial_content_left, 0);
        // >= 0x7f: pcl 0 and basalt, 16 particles = 64 draws
        tile(&w, 40, 40).set_partial_content_left(200);
        rec.take();
        srand(5);
        o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false);
        let after = rand();
        srand(5);
        for _ in 0..64 {
            rand();
        }
        assert_eq!(after, rand());
        assert!(rec.take().contains(&"fill(40,40,0x428)".to_string()));
    }

    #[test]
    fn full_water_converts_after_two_frames() {
        let rec = install_rec();
        let w = setup();
        let (b, o) = mk(&w, 40, 40);
        water(&w, 40, 40, 0xff);
        b.st.borrow_mut().remove_next_frame = true;
        // wasRemoveNextFrame (true) && removeNextFrame (true) && pcl == 0xff -> removed + fillTile(0x414)
        rec.take();
        o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false);
        assert!(o.needs_removed());
        assert!(rec.take().contains(&"fill(40,40,0x414)".to_string()));
    }

    #[test]
    fn a_full_pull_fills_the_tile_and_wakes_the_neighbours() {
        let rec = install_rec();
        let w = setup();
        let (b, _o) = mk(&w, 40, 40);
        water(&w, 40, 41, 255);
        let t = tile(&w, 40, 40);
        assert!(b.take_any_water_from_tile_at_pos_tile(IntPair { x: 40, y: 41 }, &t));
        assert_eq!((t.type_index(), t.partial_content_left()), (3, 0xff));
        assert!(b.st.borrow().remove_next_frame);
        assert!(rec.take().contains(&"loadSurface(39,40)".to_string()));
    }

    #[test]
    fn the_two_strike_removal_resets_when_water_is_near() {
        install_rec();
        let w = setup();
        let strike = |b: &SurfaceBlock| {
            b.st.borrow_mut().remove_next_frame_empty = true;
            b.remove_if_floating_and_empty_of_water();
            b.st.borrow().remove_next_frame_empty
        };
        // Water in the tile itself, above, or beside it with more than one unit resets the first strike.
        let (b, _o) = mk(&w, 40, 40);
        assert!(strike(&b)); // nothing near: the flag was already set, so this is the second strike and it removes
        let (b, _o) = mk(&w, 41, 40);
        water(&w, 41, 40, 5);
        assert!(!strike(&b));
        let (b, _o) = mk(&w, 42, 40);
        water(&w, 42, 41, 5);
        assert!(!strike(&b));
        let (b, _o) = mk(&w, 43, 40);
        water(&w, 42, 40, 5);
        assert!(!strike(&b));
        let (b, _o) = mk(&w, 45, 40);
        water(&w, 46, 40, 5);
        assert!(!strike(&b));
        // A tile with a single unit of water does not wake its neighbours.
        let (b1, _o1) = mk(&w, 47, 40);
        let t1 = tile(&w, 47, 40);
        water(&w, 47, 41, 1);
        assert!(b1.take_any_water_from_tile_at_pos_tile(IntPair { x: 47, y: 41 }, &t1));
        // Saved blocks start with the flip-flop off.
        let (_d, dwr) = test_dw();
        let saved = SurfaceBlock::init_with_world_dynamic_world_save_dict_cache(
            &w,
            &dwr,
            &crate::objects::base::new_mutable_dict(),
            None,
        )
        .unwrap();
        assert!(!saved.st.borrow().ping_pong);
        // At the edge of the loaded area the missing neighbour counts as something to keep the block for.
        for (x, y) in [(40, 63), (0, 40), (63, 40)] {
            let (b, _o) = mk(&w, x, y);
            assert!(!strike(&b), "{x},{y}");
        }
    }

    #[test]
    fn side_flow_static_blocks_and_the_origin_marker() {
        install_rec();
        let w = setup();
        // Water beside a tile with less water flows over; either neighbour order works.
        for ping in [false, true] {
            let (b, o) = mk(&w, 40, 40);
            b.st.borrow_mut().ping_pong = ping;
            water(&w, 41, 40, 200);
            water(&w, 39, 40, 200);
            o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false);
            assert!(tile(&w, 40, 40).partial_content_left() > 0);
            water(&w, 40, 40, 0);
            water(&w, 41, 40, 0);
            water(&w, 39, 40, 0);
        }
        // A tile holding exactly one unit of water with nothing to pull from dries up.
        let (_b, o) = mk(&w, 50, 40);
        water(&w, 50, 40, 1);
        o.update_accurate_dt_is_simulation(&o, 0.1, 0.1, false);
        assert_eq!(tile(&w, 50, 40).partial_content_left(), 0);
        // The debug position toggles its flip-flop even when the block is already removed.
        let (b0, o0) = mk(&w, 0, 0);
        let before = b0.st.borrow().ping_pong;
        o0.set_needs_removed(&o0, true);
        o0.update_accurate_dt_is_simulation(&o0, 0.1, 0.1, false);
        assert_ne!(b0.st.borrow().ping_pong, before);
        // The class descriptor, the downcast hook and the network init.
        assert_eq!(o.class_info().name, "SurfaceBlock");
        assert!(o.as_any().is::<SurfaceBlock>());
        let (_d, dwr) = test_dw();
        let at = |x: u32, y: u32| {
            let nd =
                crate::types::netdata::DynamicObjectNetData { unique_id: 5, pos_x: x, pos_y: y, ..Default::default() };
            crate::types::structs::WireStruct::to_vec(&nd)
        };
        assert!(SurfaceBlock::init_with_world_dynamic_world_cache_net_data(&w, &dwr, None, &at(40, 40)).is_some());
        assert!(SurfaceBlock::init_with_world_dynamic_world_cache_net_data(&w, &dwr, None, &at(100, 100)).is_none());
    }
}
