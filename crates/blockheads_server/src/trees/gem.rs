//! `GemTree: Tree` (DynamicObjectType 57, TreeType 11..15 = `gemTreeType`, fruit = the gem item).
//!
//! (lines 386-610); for the inherited behaviour. A STATIC tree: it builds itself in `init`, never grows or
//! dies, drops its gem during the "fruit year" of an 8-year cycle, and its `worldChanged:` (which does NOT chain
//! to `Tree.worldChanged:`) flood-fills from the soil.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use crate::dynworld::state::DynamicWorld;
use crate::fmath::cconv::{f32_to_i32, f64_to_i32};
use crate::objects::base::{ub_null_tile_deref, DynamicObjectBase};
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{alloc_obj, downcast, CacheRef, ObjRef};
use crate::plist::value::{PlistDict, PlistValue};
use crate::rng::glibc_rand::r01_from;
use crate::types::item_type::ItemType;
use crate::types::structs::IntPair;
use crate::types::tile::Tile;
use crate::world::helpers::{tile_is_solid, tile_is_tree};
use crate::world::state::World;
use crate::world::tiles::world_index_at_world_position;

use super::tree::*;

/// `GemTree: Tree`.
pub static GEM_TREE_CLASS: ClassInfo = ClassInfo { name: "GemTree", superclass: Some(&TREE_CLASS), protocols: &[] };

/// The gem tree object (instance size 0xb8).
pub struct GemTree {
    base: RefCell<DynamicObjectBase>,
    tree: RefCell<TreeState>,
    /// 0xb0 (`TreeType`, 11..15)
    gem_tree_type: Cell<i32>,
    /// 0xb4 (0..7)
    fruit_year: Cell<i32>,
}

crate::impl_tree_dynamic_object!(GemTree, GEM_TREE_CLASS, OBJECT_TYPE_GEM);

/// `(gemItemType, trunk, bush, trunkBush)` per `gemTreeType`; the default column applies outside 11..15.
fn gem_table(gem_tree_type: i32) -> (u32, u8, u8, u8) {
    match gem_tree_type {
        11 => (87, 109, 110, 111),
        12 => (86, 112, 113, 114),
        13 => (76, 115, 116, 117),
        14 => (75, 118, 119, 120),
        15 => (88, 121, 122, 123),
        _ => (87, 109, 110, 111),
    }
}

/// `(float)rand()/2147483648.0f - 0.5f` jitter byte of the trunk, clamped `0..255`.
fn clamp_u8(v: i32) -> u8 {
    if v < 0 {
        0
    } else if v > 255 {
        255
    } else {
        v as u8
    }
}

/// `(int)(fx * (float)w...)` style clamp into `[-w/2, w/2]`.
fn clamp_w(v: i32, w: i32) -> i32 {
    clampi(v, -w / 2, w / 2)
}

impl GemTree {
    fn alloc() -> ObjRef {
        alloc_obj(|w| GemTree {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            tree: RefCell::new(TreeState::new()),
            gem_tree_type: Cell::new(0),
            fruit_year: Cell::new(0),
        })
    }

    /// `-gemTreeType` / `-treeType`.
    pub fn gem_tree_type(&self) -> i32 {
        self.gem_tree_type.get()
    }
    /// `-gemItemType`.
    pub fn gem_item_type(&self) -> u32 {
        gem_table(self.gem_tree_type.get()).0
    }
    /// `-fruitYear`.
    pub fn fruit_year(&self) -> i32 {
        self.fruit_year.get()
    }
    fn trunk_contents_type(&self) -> u8 {
        gem_table(self.gem_tree_type.get()).1
    }
    fn bush_contents_type(&self) -> u8 {
        gem_table(self.gem_tree_type.get()).2
    }
    fn trunk_bush_contents_type(&self) -> u8 {
        gem_table(self.gem_tree_type.get()).3
    }

    /// `-initWithWorld:dynamicWorld:atPosition:cache:treeDensityNoiseFunction:seasonOffsetNoiseFunction:
    /// gemTreeType:` (R3: the last argument is the `treeType` of the load request).
    /// RNG: `fruitYear = rand() % 8` first, then the trunk, then the canopy.
    pub fn new(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        tdnf: NoiseRef,
        sonf: NoiseRef,
        gem_tree_type: i32,
    ) -> Option<ObjRef> {
        let me = Self::alloc();
        if !tree_init_static_tree(&me, world, dw, pos, cache, tdnf.clone(), sonf.clone()) {
            return None;
        }
        let this = downcast::<GemTree>(&me).unwrap();
        this.gem_tree_type.set(gem_tree_type);
        tree_state(&me).borrow_mut().height = 0;
        this.fruit_year.set(rn("GemTree.init.fruitYear").wrapping_rem(8));
        let uid = me.unique_id();
        let p = me.pos();
        let (px, py) = (p.x, p.y);
        let wm = world.world_width_macro();
        // --- season offset
        let nx = (((px as f64) * 1.1467f64) / (wm as f64)) as f32 as f64;
        let ny = (((py as f64) * 4.37253f64) / (wm as f64)) as f32 as f64;
        let n = noise(&sonf, nx, ny, 6);
        let tso = f32_to_i32(127.0f32 * (n as f32));
        tree_state(&me).borrow_mut().tree_season_offset = tso;
        // --- height
        let dx = ((px as f32) / (wm.wrapping_shl(5) as f32)) as f64;
        let dy = ((py as f32) / 1024.0f32) as f64;
        let d = noise(&tdnf, dx, dy, 6);
        let hh = clampi(f32_to_i32(8.0f32 + 8.0f32 * (d as f32)), 8, 24);
        let trunk = this.trunk_contents_type();
        let bush = this.bush_contents_type();
        let trunk_bush = this.trunk_bush_contents_type();

        // --- trunk growth
        let mut x_off = [0i32; 3];
        let mut ys = [0i32; 3];
        let mut branch_count: usize = 1;
        let mut max_y = 0;
        let mut i = 0;
        while i < hh {
            let mut b = 0usize;
            while b < branch_count {
                let tp = IntPair { x: px + x_off[b], y: py + ys[b] };
                if ys[b] > max_y {
                    max_y = ys[b];
                }
                let tile = tile_at(world, tp.x, tp.y);
                if let Some(tr) = tile {
                    let tt = tr.get();
                    if tt.on_fire != 0 || tile_is_solid(tile) || (tt.contents != 0 && !tile_is_tree(tile)) {
                        break;
                    }
                }
                let Some(tr) = tile else {
                    // R2: NULL tile dereference in the original (SIGSEGV with `faithful-crashes`); otherwise leave the branch loop.
                    ub_null_tile_deref();
                    break;
                };
                let r = rn("GemTree.init.trunkJitter");
                let v = f32_to_i32(
                    (((ys[b] - hh / 2) * 4) / hh + 127 + tso) as f32 + 8.0f32 * (r01_from(r) - 0.5f32) * 2.0f32,
                );
                let byte = clamp_u8(v);
                let mut w = (hh - i) / 8;
                if w < 2 {
                    w = 1;
                }
                tr.update(|t| t.season_offset = byte);
                dw.world_contents_changed_at_pos(tp);
                tr.update(|t| {
                    t.contents = trunk;
                    t.dynamic_object_owner = uid;
                });
                if w > 1 {
                    for k in 0..w {
                        let sp = IntPair { x: px + x_off[b] + k - w / 2, y: py + ys[b] };
                        let t2 = tile_at(world, sp.x, sp.y);
                        if let Some(t2r) = t2 {
                            let t2t = t2r.get();
                            if t2t.on_fire == 0 && !tile_is_solid(t2) && (t2t.contents == 0 || tile_is_tree(t2)) {
                                t2r.update(|t| t.season_offset = byte);
                                dw.world_contents_changed_at_pos(sp);
                                t2r.update(|t| {
                                    t.contents = trunk;
                                    t.dynamic_object_owner = uid;
                                });
                            }
                        }
                    }
                }
                let fx = (r01t("GemTree.init.fx") - 0.5f32) * 2.0f32;
                let fy = (r01t("GemTree.init.fy") - 0.5f32) * 2.0f32;
                if 0.0f32 < fy || ys[b] < 2 {
                    // grow up
                    ys[b] += 1;
                    let dx_ = clamp_w(f32_to_i32(fx * (w as f32) * 2.0f32), w);
                    if i > 2 && dx_.abs() > 0 && branch_count < 3 {
                        ys[branch_count] = ys[b];
                        x_off[branch_count] = x_off[b] - dx_;
                        branch_count += 1;
                    }
                    x_off[b] += dx_;
                } else if -0.5f32 < fy || ys[b] < 4 {
                    // sideways
                    let mut dx_ = clamp_w(f32_to_i32(fx * (w as f32)), w);
                    if dx_ == 0 {
                        dx_ = if 0.0f32 < fx { 1 } else { -1 };
                    }
                    if i > 2 && dx_.abs() > 0 && branch_count < 3 {
                        ys[branch_count] = ys[b];
                        x_off[branch_count] = x_off[b] - dx_;
                        branch_count += 1;
                    }
                    x_off[b] += dx_;
                } else {
                    // downward
                    ys[b] -= 1;
                    x_off[b] += clamp_w(f32_to_i32((fx * (w as f32)) / 2.0f32), w);
                }
                b += 1;
            }
            i += 1;
        }

        // --- canopy
        for br in 0..branch_count {
            let tp = IntPair { x: px + x_off[br], y: py + ys[br] + 2 };
            if tile_at(world, tp.x, tp.y).is_none() {
                continue;
            }
            for i1 in 0..3 {
                let yy = ys[br] - i1 + 2;
                let mut l = f32_to_i32(((i1 * 2 + 1) as f32) + (r01t("GemTree.init.canopyL") - 0.5f32) * 2.0f32);
                let mut r = f32_to_i32(((i1 * 2 + 1) as f32) + (r01t("GemTree.init.canopyR") - 0.5f32) * 2.0f32);
                if l < 2 {
                    l = 1;
                }
                if r < 2 {
                    r = 1;
                }
                let mut x = tp.x - l;
                while x < tp.x + r + 1 {
                    let f = (r01t("GemTree.init.canopyF") - 1.0f32) * 8.0f32;
                    let mut tl_ = f32_to_i32(2.0f32 * (f + ((x - tp.x).abs() as f32)));
                    if tl_ < 2 {
                        tl_ = 1;
                    }
                    for t in 0..tl_ {
                        let bp = IntPair { x, y: py + yy - t };
                        let bt = tile_at(world, bp.x, bp.y);
                        let Some(btr) = bt else { continue };
                        let bc = btr.get();
                        if bc.on_fire != 0 || tile_is_solid(bt) || (bc.contents != 0 && !tile_is_tree(bt)) {
                            break;
                        }
                        let fruit_count = tree_state(&me).borrow().fruit_count;
                        if fruit_count < 64 && bc.contents == 0 && 0.95f64 < (r01t("GemTree.init.fruit") as f64) {
                            {
                                let mut s = tree_state(&me).borrow_mut();
                                s.tree_fruits[fruit_count as usize] =
                                    TreeFruit { pos: bp, has_created_free_block_this_season: false };
                                s.fruit_count += 1;
                            }
                            dw_changed(&me);
                        }
                        let nc = if bc.contents == trunk || bc.contents == trunk_bush { trunk_bush } else { bush };
                        btr.update(|t| t.contents = nc);
                        dw.world_contents_changed_at_pos(bp);
                        btr.update(|t| t.dynamic_object_owner = uid);
                        if yy - t > max_y {
                            max_y = yy - t;
                        }
                    }
                    x += 1;
                }
            }
        }
        tree_state(&me).borrow_mut().height = max_y;
        Some(me)
    }

    /// `-initWithWorld:dynamicWorld:saveDict:cache:...` (`return [super init...]`; the virtual
    /// `loadSaveDictValues:` below runs inside it, `growInTimeSinceSaved:` returns at once for a static tree).
    pub fn from_save_dict(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        d: &PlistDict,
        cache: CacheRef,
        tdnf: NoiseRef,
        sonf: NoiseRef,
    ) -> Option<ObjRef> {
        let me = Self::alloc();
        tree_init_with_world_dynamic_world_save_dict_cache_tdnf_sonf(&me, world, dw, d, cache, tdnf, sonf);
        Some(me)
    }

    /// `-recursivelyAddOwnedTile:toPositions:`: the flood fill over tiles that are of this tree's kind and owned
    /// by it, 4-neighbour order W, E, S, N (membership only; implemented with an explicit stack).
    fn recursively_add_owned_tile(&self, me: &ObjRef, w: &World, tile_pos: IntPair, owned: &mut HashSet<i32>) {
        let uid = me.unique_id();
        let mut stack = vec![tile_pos];
        while let Some(p) = stack.pop() {
            let tile = tile_at(w, p.x, p.y);
            let idx = world_index_at_world_position(p.x, p.y, w);
            if let Some(tr) = tile {
                let tt = tr.get();
                if self.tile_is_kind_of_self(&tt) && tt.dynamic_object_owner == uid && !owned.contains(&idx) {
                    owned.insert(idx);
                    stack.push(IntPair { x: p.x, y: p.y + 1 });
                    stack.push(IntPair { x: p.x, y: p.y - 1 });
                    stack.push(IntPair { x: p.x + 1, y: p.y });
                    stack.push(IntPair { x: p.x - 1, y: p.y });
                }
            }
        }
    }
}

/// `((int)(8.0f * s + 4.0f + 8.0f)) % 8` with `s = seasonForX + 0.003f * (float)(int)(seasonOffset - 127)`.
fn idx(season_for_x: f32, t: &Tile, _tso: i32) -> i32 {
    let s = season_for_x + 0.003f32 * ((t.season_offset as i32 - 127) as f32);
    f32_to_i32((8.0f32 * s + 4.0f32) + 8.0f32).wrapping_rem(8)
}
fn window(i: i32) -> bool {
    i == 2 || i == 3
}

impl TreeLike for GemTree {
    fn tree(&self) -> &RefCell<TreeState> {
        &self.tree
    }
    fn tree_type(&self) -> i32 {
        self.gem_tree_type.get()
    }
    fn fruit_item_type(&self) -> i32 {
        self.gem_item_type() as i32
    }
    fn should_add_fallen_fruits(&self) -> bool {
        false
    }
    /// `((int)(worldTime() / 3600.0) % 8 == fruitYear) && season == 4`.
    fn fruit_should_fall_in_season(&self, season: i32) -> bool {
        let wt = self.tree.borrow().world.upgrade().map_or(0.0, |w| w.world_time());
        f64_to_i32(wt / 3600.0f64).wrapping_rem(8) == self.fruit_year.get() && season == 4
    }
    fn is_static_tree(&self) -> bool {
        true
    }
    fn tile_is_kind_of_self(&self, t: &Tile) -> bool {
        if t.contents == 0x22 || t.contents == 0x1d {
            return true;
        }
        match self.gem_tree_type.get() {
            11..=15 => {
                let (_, tr, bu, tb) = gem_table(self.gem_tree_type.get());
                t.contents == tr || t.contents == bu || t.contents == tb
            }
            _ => false,
        }
    }
    fn make_tile_dead(&self, t: &mut Tile) {
        if t.contents == self.trunk_contents_type() || t.contents == self.trunk_bush_contents_type() {
            t.contents = 0x1d;
        } else if t.contents == self.bush_contents_type() {
            t.contents = 0x22;
        }
    }

    fn load_save_dict_values(&self, me: &ObjRef, d: &PlistDict) {
        let get = |k: &str| d.map.get(&k.to_string());
        self.gem_tree_type.set(get("gemTreeType").map_or(0, |v| v.int_value()));
        self.fruit_year.set(get("fruitYear").map_or(0, |v| v.int_value()));
        tree_load_save_dict_values(me, d);
    }

    fn extend_save_dict(&self, d: &mut PlistDict) {
        d.map.insert("gemTreeType".to_string(), PlistValue::int(self.gem_tree_type.get() as i64));
        d.map.insert("fruitYear".to_string(), PlistValue::int(self.fruit_year.get() as i64));
    }

    ///  No `rand()` here.
    fn species_update(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        tree_update(me, dt, accurate_dt, is_simulation);
        if self.tree.borrow().dead {
            return;
        }
        let w = tree_world(me);
        let year_index = f64_to_i32(w.world_time() / 3600.0f64);
        if year_index.wrapping_rem(8) != self.fruit_year.get() {
            return;
        }
        let cfg = FruitCfg {
            item: self.gem_item_type(),
            lookup_any: true,
            idx,
            window,
            min_height: i32::MIN,
            count: count_one,
            gene_data: false,
        };
        fruit_season_update(me, &cfg);
    }

    /// a broken tile that are no longer connected to the soil-anchored owned set are removed, with `0.8f`
    /// then `0.95f` rolls per removed tile. `owned` is not refreshed after removals.
    fn species_world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        let w = tree_world(me);
        let dw = tree_dynamic_world(me);
        let pos = me.pos();
        let uid = me.unique_id();
        let height = self.tree.borrow().height;
        let mut check: Vec<IntPair> = Vec::new();
        for &p in positions.iter() {
            if p.y <= pos.y.wrapping_add(height)
                && pos.y - 8 <= p.y
                && (p.x.wrapping_sub(pos.x)).wrapping_abs() < height.wrapping_mul(2)
            {
                if let Some(tr) = tile_at(&w, p.x, p.y) {
                    if !self.tile_is_kind_of_self(&tr.get()) {
                        check.push(p);
                    }
                }
            }
        }
        if check.is_empty() {
            return;
        }
        let mut owned: HashSet<i32> = HashSet::new();
        if let Some(b) = tile_at(&w, pos.x, pos.y - 1) {
            if self.is_required_soil_type(b.get().type_index as i32) {
                self.recursively_add_owned_tile(me, &w, pos, &mut owned);
            }
        }
        for p in check {
            let neighbours = [
                IntPair { x: p.x - 1, y: p.y },
                IntPair { x: p.x + 1, y: p.y },
                IntPair { x: p.x, y: p.y - 1 },
                IntPair { x: p.x, y: p.y + 1 },
            ];
            for n in neighbours {
                let idx = world_index_at_world_position(n.x, n.y, &w);
                if owned.contains(&idx) {
                    continue;
                }
                if let Some(tr) = tile_at(&w, n.x, n.y) {
                    let tt = tr.get();
                    if tt.dynamic_object_owner == uid && tt.on_fire == 0 && self.tile_is_kind_of_self(&tt) {
                        let cnt = if 0.8f32 < r01t("GemTree.worldChanged.drop") { 1 } else { 0 };
                        remove_tile(&w, n.x, n.y, cnt);
                        if 0.95f32 < r01t("GemTree.worldChanged.gem") {
                            dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                                n,
                                ItemType(self.gem_item_type()),
                                0,
                                0,
                                None,
                                None,
                                false,
                                0,
                                None,
                            );
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod sweep_tests {
    use super::*;
    use crate::trees::tree::testenv::{env, scatter, wipe, GROUND};

    struct Cn(f64);
    impl crate::dynworld::state::DwNoise for Cn {
        fn get_x_y_octaves(&self, _x: f64, _y: f64, _o: i32) -> f64 {
            self.0
        }
    }

    fn noise_of(v: f64) -> NoiseRef {
        Some(Rc::new(Cn(v)))
    }

    /// Whole gem trees over many seeds, types, noise extremes and obstacle fields, then broken apart.
    #[test]
    fn gem_trees_over_obstacles_and_broken_apart() {
        let (w, d) = env("gem_sweep");
        let mt = w.macro_tiles();
        let mut lcg = 99u64;
        for seed in 0..500u32 {
            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let right = seed % 11 == 7;
            let x = if right { 31 } else { 14 + (seed % 36) as i32 };
            let tdnf = noise_of([2.0, 1.0, -1.0, 0.3][(seed % 4) as usize]);
            let sonf = noise_of([-2.0, 2.0, 0.0, 0.5][((seed / 4) % 4) as usize]);
            let gt = 11 + (seed % 6) as i32;
            let high = seed % 7 == 3;
            let (px, py) = if high { (x, 31) } else { (x, GROUND + 1) };
            let saved: Vec<_> = (0..4).map(|i| *mt.get(i).unwrap().physical_block.borrow()).collect();
            let unload: &[usize] = if seed % 11 == 5 {
                &[2, 3]
            } else if right {
                &[1, 3]
            } else {
                &[]
            };
            for &i in unload {
                *mt.get(i).unwrap().physical_block.borrow_mut() = None;
            }
            scatter(&w, x, lcg >> 7, [0, 4, 12, 30][(seed % 4) as usize]);
            crate::rng::glibc_rand::srand(seed + 1);
            let g = GemTree::new(&w, &d, IntPair { x: px, y: py }, None, tdnf, sonf, gt);
            if let Some(g) = g {
                let gtr = downcast::<GemTree>(&g).unwrap();
                let _ = (gtr.gem_tree_type(), gtr.gem_item_type());
                let uid = g.unique_id();
                // knock out owned tiles and report them
                let mut changed = vec![];
                for y in GROUND + 1..GROUND + 40 {
                    for dx in -12..=12 {
                        if let Some(tr) = tile_at(&w, x + dx, y) {
                            lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                            if tr.get().dynamic_object_owner == uid && (lcg >> 40) % 5 == 0 {
                                tr.update(|t| t.contents = 0);
                                changed.push(IntPair { x: x + dx, y });
                            }
                        }
                    }
                }
                tree_state(&g).borrow_mut().height = 6 + (seed % 9) as i32;
                // a position in an unloaded block, and the soil below the root in an unloaded block
                changed.push(IntPair { x: 63, y: GROUND + 2 });
                for i in [0, 1] {
                    *mt.get(i).unwrap().physical_block.borrow_mut() = None;
                }
                as_tree(&g).unwrap().species_world_changed(&g, &mut changed);
                for (i, pb) in (0..4).zip(&saved) {
                    *mt.get(i).unwrap().physical_block.borrow_mut() = *pb;
                }
                // the flood fill walks into unloaded neighbours
                for i in [1, 2, 3] {
                    *mt.get(i).unwrap().physical_block.borrow_mut() = None;
                }
                let mut again = vec![IntPair { x: x + 1, y: py + 1 }, IntPair { x: x - 1, y: py + 2 }];
                again.extend(changed.iter().copied().take(3));
                as_tree(&g).unwrap().species_world_changed(&g, &mut again);
                for (i, pb) in (0..4).zip(&saved) {
                    *mt.get(i).unwrap().physical_block.borrow_mut() = *pb;
                }
                if seed % 5 == 0 {
                    tree_state(&g).borrow_mut().dead = true;
                    as_tree(&g).unwrap().species_update(&g, 0.2, 0.2, false);
                    tree_state(&g).borrow_mut().dead = false;
                }
                let year = gtr.fruit_year();
                w.set_world_time(f64::from(year) * 3600.0 + f64::from(seed % 3600));
                as_tree(&g).unwrap().species_update(&g, 0.2, 0.2, false);
                g.set_needs_removed(&g, true);
                g.remove_from_macro_block(&g);
            }
            for (i, pb) in (0..4).zip(saved) {
                *mt.get(i).unwrap().physical_block.borrow_mut() = pb;
            }
            wipe(&w, x);
        }
        // type 16 and 0 use the default table column
        assert_eq!(gem_table(16).0, 87);
        assert_eq!(clamp_u8(-3), 0);
        assert_eq!(clamp_u8(300), 255);
    }
}
