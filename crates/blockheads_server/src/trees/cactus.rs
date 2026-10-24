//! `CactusTree: Tree` (DynamicObjectType 5, TreeType 5, fruit ITEM_PRICKLY_PEAR 24).
//!
//!  for the inherited behaviour. Ivars `splitHeightA/B`, `splitDirection`, `availableFood`; own
//! `isRequiredSoilType:`; spawns `NPC_SCORPION`. Ignore CarrotPlant / ChilliPlant (plants module).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::dynworld::state::DynamicWorld;
use crate::fmath::cconv::f32_to_i32;
use crate::objects::base::{ub_null_tile_deref, DynamicObjectBase};
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{alloc_obj, downcast, CacheRef, ObjRef};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::structs::IntPair;
use crate::types::tile::Tile;
use crate::world::helpers::{tile_is_dead_tree, tile_is_solid, tile_is_tree};
use crate::world::state::{TileRef, World};

use super::tree::*;

/// `CactusTree: Tree`.
pub static CACTUS_TREE_CLASS: ClassInfo =
    ClassInfo { name: "CactusTree", superclass: Some(&TREE_CLASS), protocols: &[] };

/// `NPC_SCORPION`.
const NPC_SCORPION: i32 = 7;

/// The cactus object (instance size 0xc0).
pub struct CactusTree {
    base: RefCell<DynamicObjectBase>,
    tree: RefCell<TreeState>,
    /// 0xb0
    split_height_a: Cell<i32>,
    /// 0xb4
    split_height_b: Cell<i32>,
    /// 0xb8
    split_direction: Cell<bool>,
    /// 0xbc
    available_food: Cell<f32>,
}

crate::impl_tree_dynamic_object!(CactusTree, CACTUS_TREE_CLASS, OBJECT_TYPE_CACTUS);

/// `s = 256.0f + seasonForX + 0.05f * (float)((int)seasonOffset - 127)`.
fn idx(season_for_x: f32, t: &Tile, _tso: i32) -> i32 {
    idx_basic((256.0f32 + season_for_x) + 0.05f32 * ((t.season_offset as i32 - 127) as f32))
}
fn window(i: i32) -> bool {
    i == 2 || i == 3
}
/// `n = inCompost ? 2: 1` evaluated at spawn time.
fn count(me: &ObjRef, _height: i32) -> i32 {
    if tree_is_growing_in_compost(me) {
        2
    } else {
        1
    }
}
static FRUIT: FruitCfg =
    FruitCfg { item: 24, lookup_any: false, idx, window, min_height: i32::MIN, count, gene_data: true };

impl CactusTree {
    fn alloc() -> ObjRef {
        alloc_obj(|w| CactusTree {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            tree: RefCell::new(TreeState::new()),
            split_height_a: Cell::new(0),
            split_height_b: Cell::new(0),
            split_direction: Cell::new(false),
            available_food: Cell::new(0.0),
        })
    }

    /// `-availableFood`.
    pub fn available_food(&self) -> f32 {
        self.available_food.get()
    }
    /// `-setAvailableFood:` (plain store).
    pub fn set_available_food(&self, v: f32) {
        self.available_food.set(v);
    }

    /// `-initWithWorld:...adultTree:adultMaxAge:`. RNG order: maxHeight, [heightToGrowTo], splitA,
    /// splitB, splitDirection, (rands inside `updateGrowth:`), availableFood.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        max_height: i16,
        growth_rate: i16,
        tdnf: NoiseRef,
        sonf: NoiseRef,
        adult_tree: bool,
        adult_max_age: f32,
    ) -> Option<ObjRef> {
        let me = Self::alloc();
        if !tree_init_with_world_dynamic_world_at_position_cache_max_height_growth_rate_tdnf_sonf_adult_tree_adult_max_age(
            &me, world, dw, pos, cache, max_height, growth_rate, tdnf, sonf, adult_tree, adult_max_age,
        ) {
            return None;
        }
        let this = downcast::<CactusTree>(&me).unwrap();
        let mhf = tree_state(&me).borrow().max_height_gene as f32 / 255.0f32;
        let p = me.pos();
        // maxHeight = (int)(4.0f + (R01 * mhf) * 8.0f): rand #1, always
        let r = r01t("CactusTree.init.maxHeight");
        let mh = f32_to_i32(4.0f32 + (r * mhf) * 8.0f32);
        let (htg, ama) = init_height_target(&me, mh, adult_tree, adult_max_age);
        let max_h = tree_state(&me).borrow().max_height;
        // maxHeight >= 4 so `% (maxHeight - 2)` is safe
        let m2 = max_h - 2;
        let mut a = rn("CactusTree.init.splitA").wrapping_rem(m2) + 2;
        if m2 < a {
            a = 99;
        }
        let mut b = rn("CactusTree.init.splitB").wrapping_rem(m2) + 2;
        if m2 < b {
            b = 99;
        }
        if b < a {
            std::mem::swap(&mut a, &mut b);
        }
        this.split_height_a.set(a);
        this.split_height_b.set(b);
        this.split_direction.set(rn("CactusTree.init.splitDirection") > 0x3fffffff);
        let ww = world.world_width_macro();
        let frac = season_noise_fraction(&me, (p.x as f32 / 32.0f32) / ww as f32, (p.y as f32 / 32.0f32) / ww as f32);
        tree_state(&me).borrow_mut().tree_season_offset = f32_to_i32(64.0f32 * frac);
        pre_grow(&me, htg, ama);
        compute_max_age(&me);
        this.available_food.set(((r01t("CactusTree.init.availableFood") * 900.0f32) as f64 * 0.5f64) as f32);
        Some(me)
    }

    /// `-initWithWorld:dynamicWorld:saveDict:cache:...` (`return [super init...]`; the virtual
    /// `loadSaveDictValues:` below runs inside it).
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

    /// One arm of `updateGrowth:` (A: `split_height_a`, `okA`, slot 1; B: mirror, slot 2, count 3).
    /// Returns the new `ok` flag.
    #[allow(clippy::too_many_arguments)]
    fn arm(
        &self,
        me: &ObjRef,
        w: &World,
        y: i32,
        h: i32,
        s_base: i32,
        split: i32,
        east: bool,
        slot: usize,
        add_new: bool,
    ) -> bool {
        let pos = me.pos();
        let uid = me.unique_id();
        let dw = tree_dynamic_world(me);
        let mut sx = if east { 1 } else { -1 };
        let mut sy = y - 1;
        if (y - split) > 1 {
            sx *= 2;
            sy = y - 2;
        }
        let at: Option<TileRef> = tile_at(w, pos.x + sx, pos.y + sy);
        let Some(atr) = at else {
            // R2: the original dereferences the NULL tile (SIGSEGV with `faithful-crashes`); otherwise "blocked".
            ub_null_tile_deref();
            return false;
        };
        let a = atr.get();
        let mut ok = true;
        if !add_new || ((tile_is_solid(at) || a.contents != 0) && !tile_is_dead_tree(at)) {
            if !self.tile_is_kind_of_self(&a) {
                ok = false;
            } else if a.dynamic_object_owner == 0 {
                atr.update(|t| t.dynamic_object_owner = uid);
            }
        } else {
            atr.update(|t| {
                t.dynamic_object_owner = uid;
                t.contents = 43;
            });
            if (y - split) >= 0 && y == h - 1 {
                {
                    let mut s = self.tree.borrow_mut();
                    s.tree_fruits[slot].pos = IntPair { x: pos.x + sx, y: pos.y + sy };
                    s.tree_fruits[slot].has_created_free_block_this_season = false;
                    s.fruit_count = slot as i32 + 1;
                }
                dw.dynamic_world_changed_at_pos_object_type(pos, me.object_type());
            }
            let sb = jitter_byte(s_base, "CactusTree.updateGrowth.arm");
            atr.update(|t| t.season_offset = sb);
            dw.world_contents_changed_at_pos(IntPair { x: pos.x + sx, y: pos.y + sy });
            ok = false;
        }
        if atr.get().on_fire != 0 {
            ok = false;
        }
        ok
    }
}

/// `v = (int)((float)sBase + 31.0f * (R01 - 0.5f) * 2.0f)` clamped to `0..255`. One `rand()`.
fn jitter_byte(s_base: i32, tag: &'static str) -> u8 {
    let v = f32_to_i32(s_base as f32 + 31.0f32 * (r01t(tag) - 0.5f32) * 2.0f32);
    if v < 0 {
        0
    } else if v > 255 {
        255
    } else {
        v as u8
    }
}

impl TreeLike for CactusTree {
    fn tree(&self) -> &RefCell<TreeState> {
        &self.tree
    }
    fn tree_type(&self) -> i32 {
        TREE_CACTUS
    }
    fn fruit_item_type(&self) -> i32 {
        24
    }
    /// `season == 4` (not used by its own `update:`).
    fn fruit_should_fall_in_season(&self, season: i32) -> bool {
        season == 4
    }
    /// `TILE_SAND(7), SAND_BEACH(8), BLACK_SAND(58), COMPOST(48), COMPOST_GRASS(49), COMPOST_GRASS_FROZEN(50)`.
    fn is_required_soil_type(&self, tile_type: i32) -> bool {
        matches!(tile_type, 7 | 8 | 58 | 48 | 49 | 50)
    }
    fn tile_is_kind_of_self(&self, t: &Tile) -> bool {
        t.contents == 43 || t.contents == 44
    }
    fn make_tile_dead(&self, t: &mut Tile) {
        if t.contents == 43 {
            t.contents = 44;
        }
    }

    fn load_save_dict_values(&self, me: &ObjRef, d: &PlistDict) {
        tree_load_save_dict_values(me, d);
        let get = |k: &str| d.map.get(&k.to_string());
        self.split_height_a.set(get("splitHeightA").map_or(0, |v| v.int_value()));
        self.split_height_b.set(get("splitHeightB").map_or(0, |v| v.int_value()));
        self.split_direction.set(get("splitDirection").map_or(false, |v| v.bool_value()));
        self.available_food.set(get("availableFood").map_or(0.0, |v| v.float_value()));
    }

    fn extend_save_dict(&self, d: &mut PlistDict) {
        d.map.insert("splitHeightA".to_string(), PlistValue::int(self.split_height_a.get() as i64));
        d.map.insert("splitHeightB".to_string(), PlistValue::int(self.split_height_b.get() as i64));
        d.map.insert("splitDirection".to_string(), PlistValue::boolean(self.split_direction.get()));
        d.map.insert("availableFood".to_string(), PlistValue::real_f32(self.available_food.get()));
    }

    ///  The arm blocks are nested inside the "trunk tile
    /// newly written" branch (note 0); one `rand()` per newly written tile in the order trunk, arm A, arm B.
    fn update_growth(&self, me: &ObjRef, add_new_branch_blocks: bool) {
        if self.tree.borrow().dead {
            return;
        }
        let w = tree_world(me);
        let dw = tree_dynamic_world(me);
        let pos = me.pos();
        let uid = me.unique_id();
        let h = self.tree.borrow().height;
        let tso = self.tree.borrow().tree_season_offset;
        let (split_a, split_b, dir) =
            (self.split_height_a.get(), self.split_height_b.get(), self.split_direction.get());
        let mut ok_a = true;
        let mut ok_b = true;
        let mut y = 0;
        while y < h {
            let s_base = (y.wrapping_sub(h / 2).wrapping_mul(16) / h).wrapping_add(127).wrapping_add(tso);
            let t = tile_at(&w, pos.x, pos.y + y);
            if let Some(tr) = t {
                let tt = tr.get();
                if tt.on_fire != 0 || tile_is_solid(t) || (tt.contents != 0 && !tile_is_tree(t)) {
                    break;
                }
            }
            if let Some(tr) = t {
                let tt = tr.get();
                if (!tile_is_solid(t) && tt.contents == 0) || tile_is_dead_tree(t) {
                    // trunk tile
                    tr.update(|tile| {
                        tile.dynamic_object_owner = uid;
                        tile.contents = 43;
                    });
                    let sb = jitter_byte(s_base, "CactusTree.updateGrowth.trunk");
                    tr.update(|tile| tile.season_offset = sb);
                    dw.world_contents_changed_at_pos(IntPair { x: pos.x, y: pos.y + y });
                    if y >= split_a && ok_a {
                        ok_a = self.arm(me, &w, y, h, s_base, split_a, dir, 1, add_new_branch_blocks);
                    }
                    if y >= split_b && ok_b {
                        ok_b = self.arm(me, &w, y, h, s_base, split_b, !dir, 2, add_new_branch_blocks);
                    }
                }
            }
            y += 1;
        }
        if h < 3 {
            if self.tree.borrow().fruit_count != 0 {
                self.tree.borrow_mut().fruit_count = 0;
                dw.dynamic_world_changed_at_pos_object_type(pos, me.object_type());
            }
        } else {
            let first = {
                let mut s = self.tree.borrow_mut();
                s.tree_fruits[0].pos = IntPair { x: pos.x, y: pos.y + h - 1 };
                if s.fruit_count == 0 {
                    s.fruit_count = 1;
                    true
                } else {
                    false
                }
            };
            if first {
                dw.dynamic_world_changed_at_pos_object_type(pos, me.object_type());
            }
        }
    }

    fn species_update(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        tree_update(me, dt, accurate_dt, is_simulation);
        if self.tree.borrow().dead {
            return;
        }
        let w = tree_world(me);
        let dw = tree_dynamic_world(me);
        let pos = me.pos();
        let food = self.available_food.get();
        if food >= 1800.0f32 {
            self.available_food.set(food - 900.0f32);
            if let Some(above) = tile_at(&w, pos.x, pos.y + 1) {
                if above.get().back_wall_type_index == 2 && !w.tutorial_active() {
                    dw.load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
                        pos,
                        NPC_SCORPION,
                        None,
                        true,
                        false,
                        None,
                    );
                }
            }
        } else {
            self.available_food.set(food + 0.1f32 * dt);
        }
        fruit_season_update(me, &FRUIT);
    }
}

#[cfg(test)]
mod scenario_tests {
    use super::*;
    use crate::trees::tree::testenv::gauntlet;

    #[test]
    fn cacti_over_obstacles_and_unloaded_blocks() {
        gauntlet(TREE_CACTUS, "cactus_gauntlet", 90, &|t, seed| {
            let c = downcast::<CactusTree>(t).unwrap();
            c.set_available_food(if seed % 2 == 0 { 2000.0 } else { 3.0 });
            let _ = c.available_food();
            // arms that nobody owns any more are taken over again
            let w = tree_world(t);
            let p = t.pos();
            for y in 0..24 {
                for dx in -3..=3 {
                    for tr in tile_at(&w, p.x + dx, p.y + y) {
                        if tr.get().contents == 43 {
                            // the trunk is written again, the arms found as somebody else's
                            tr.update(|tile| {
                                tile.dynamic_object_owner = 0;
                                if dx == 0 {
                                    tile.contents = 0;
                                }
                            });
                        }
                    }
                }
            }
        });
    }
}
