//! `CoconutTree: Tree` (DynamicObjectType 6, TreeType 6, fruit ITEM_COCONUT 46).
//!
//!  for the inherited behaviour. Own `isRequiredSoilType:`; the crown of fronds is written by its own
//! `updateGrowth:` (no `rand()` in it).

use std::cell::RefCell;
use std::rc::Rc;

use crate::dynworld::state::DynamicWorld;
use crate::fmath::cconv::f32_to_i32;
use crate::objects::base::DynamicObjectBase;
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{alloc_obj, CacheRef, ObjRef};
use crate::plist::value::PlistDict;
use crate::types::structs::IntPair;
use crate::types::tile::Tile;
use crate::world::helpers::{tile_is_bush, tile_is_dead_tree, tile_is_solid, tile_is_tree};
use crate::world::state::World;

use super::tree::*;

/// `CoconutTree: Tree`.
pub static COCONUT_TREE_CLASS: ClassInfo =
    ClassInfo { name: "CoconutTree", superclass: Some(&TREE_CLASS), protocols: &[] };

/// The coconut palm object (no added ivars).
pub struct CoconutTree {
    base: RefCell<DynamicObjectBase>,
    tree: RefCell<TreeState>,
}

crate::impl_tree_dynamic_object!(CoconutTree, COCONUT_TREE_CLASS, OBJECT_TYPE_COCONUT);

/// Uses `Tree.treeSeasonOffset`, NOT the tile's `seasonOffset`; `0.2f` and the `+ 256.0f` offset.
fn idx(season_for_x: f32, _t: &Tile, tso: i32) -> i32 {
    idx_basic((256.0f32 + season_for_x) + 0.2f32 * ((tso - 127) as f32))
}
fn window(i: i32) -> bool {
    matches!(i, 0 | 2 | 4 | 6)
}
/// `n = (height - 2) / 3 + 1`, expert: `max(1, n / 2)`.
fn count(me: &ObjRef, height: i32) -> i32 {
    let mut n = (height - 2) / 3 + 1;
    if tree_world(me).expert_mode() {
        n = std::cmp::max(1, n / 2);
    }
    n
}
static FRUIT: FruitCfg =
    FruitCfg { item: 46, lookup_any: false, idx, window, min_height: i32::MIN, count, gene_data: true };

impl CoconutTree {
    fn alloc() -> ObjRef {
        alloc_obj(|w| CoconutTree {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            tree: RefCell::new(TreeState::new()),
        })
    }

    /// `-initWithWorld:...adultTree:adultMaxAge:`.
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
        let mhf = tree_state(&me).borrow().max_height_gene as f32 / 255.0f32;
        let p = me.pos();
        // deterministic, no vigor, no rand
        let mh = f32_to_i32(6.0f32 + 8.0f32 * mhf);
        let (htg, ama) = init_height_target(&me, mh, adult_tree, adult_max_age);
        let ww = world.world_width_macro();
        let frac = season_noise_fraction(&me, (p.x as f32 / 32.0f32) / ww as f32, (p.y as f32 / 32.0f32) / ww as f32);
        tree_state(&me).borrow_mut().tree_season_offset = f32_to_i32(64.0f32 * frac);
        pre_grow(&me, htg, ama);
        compute_max_age(&me);
        Some(me)
    }

    /// `-initWithWorld:dynamicWorld:saveDict:cache:...` (`return [super init...]`).
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
}

impl TreeLike for CoconutTree {
    fn tree(&self) -> &RefCell<TreeState> {
        &self.tree
    }
    fn tree_type(&self) -> i32 {
        TREE_COCONUT
    }
    fn fruit_item_type(&self) -> i32 {
        46
    }
    /// `season in {1,3,5,7}` (not used by its own `update:`).
    fn fruit_should_fall_in_season(&self, season: i32) -> bool {
        matches!(season, 1 | 3 | 5 | 7)
    }
    /// `TILE_DIRT(6), DIRT_GRASS(27), SAND_BEACH(8), BLACK_SAND(58), COMPOST(48), COMPOST_GRASS(49)`.
    fn is_required_soil_type(&self, tile_type: i32) -> bool {
        matches!(tile_type, 6 | 27 | 8 | 58 | 48 | 49)
    }
    fn tile_is_kind_of_self(&self, t: &Tile) -> bool {
        matches!(t.contents, 15 | 16 | 17 | 35 | 36)
    }
    fn make_tile_dead(&self, t: &mut Tile) {
        if t.contents == 15 {
            t.contents = 35;
        } else if t.contents == 16 || t.contents == 17 {
            t.contents = 36;
        }
    }

    ///  No `rand()`.
    fn update_growth(&self, me: &ObjRef, add_new_branch_blocks: bool) {
        if self.tree.borrow().dead {
            return;
        }
        let w = tree_world(me);
        let dw = tree_dynamic_world(me);
        let pos = me.pos();
        let uid = me.unique_id();
        let height = self.tree.borrow().height;
        let tso = self.tree.borrow().tree_season_offset;
        let obj_type = me.object_type();
        let sbyte = tso as u8;
        let can_add_fn = |c: Option<crate::world::state::TileRef>, bx: i32| -> bool {
            let Some(tr) = c else { return false };
            let tt = tr.get();
            let mut can_add = add_new_branch_blocks;
            if add_new_branch_blocks {
                can_add = (!tile_is_solid(c) && tt.contents == 0) || tile_is_dead_tree(c);
                if !can_add && tile_is_bush(c) {
                    can_add = tt.dynamic_object_owner == 0 || bx < 2;
                }
            }
            can_add
        };
        let mut y = 0;
        while y < height {
            let t = tile_at(&w, pos.x, pos.y + y);
            if let Some(tr) = t {
                let tt = tr.get();
                if tt.on_fire != 0 || tile_is_solid(t) || (tt.contents != 0 && !tile_is_tree(t)) {
                    break;
                }
            }
            let mut top_size = 1;
            if height > 2 {
                top_size = 2;
                if height > 7 {
                    top_size = 3;
                }
            }
            if y < height - top_size {
                // ---- bare trunk
                if let Some(tr) = t {
                    tr.update(|tile| {
                        tile.dynamic_object_owner = uid;
                        tile.contents = 16;
                        tile.season_offset = sbyte;
                    });
                    dw.world_contents_changed_at_pos(IntPair { x: pos.x, y: pos.y + y });
                    for dx in -2..=2 {
                        if let Some(ctr) = tile_at(&w, pos.x + dx, pos.y + y) {
                            let c = ctr.get();
                            if (c.dynamic_object_owner == uid || c.dynamic_object_owner == 0) && c.contents == 15 {
                                remove_tile(&w, pos.x + dx, pos.y + y, 0);
                            }
                        }
                    }
                }
            } else {
                // ---- crown rows
                let (mut max_r, mut min_l) = (3, -2);
                if top_size == 1 || y == height - 1 {
                    max_r = 1;
                    min_l = 0;
                } else if top_size == 2 || y == height - 2 {
                    max_r = 2;
                    min_l = -1;
                }
                let mut trunk_added = false;
                for bx in 0..max_r {
                    let c = tile_at(&w, pos.x + bx, pos.y + y);
                    if let Some(ctr) = c {
                        if !can_add_fn(c, bx) {
                            break;
                        }
                        ctr.update(|tile| {
                            tile.dynamic_object_owner = uid;
                            tile.contents = 15;
                            tile.season_offset = sbyte;
                        });
                        dw.world_contents_changed_at_pos(IntPair { x: pos.x + bx, y: pos.y + y });
                        if bx == 0 {
                            trunk_added = true;
                        }
                    }
                }
                if trunk_added {
                    let mut bx = -1;
                    while bx >= min_l {
                        let c = tile_at(&w, pos.x + bx, pos.y + y);
                        if let Some(ctr) = c {
                            if !can_add_fn(c, bx) {
                                break;
                            }
                            ctr.update(|tile| {
                                tile.dynamic_object_owner = uid;
                                tile.contents = 15;
                                tile.season_offset = sbyte;
                            });
                            dw.world_contents_changed_at_pos(IntPair { x: pos.x + bx, y: pos.y + y });
                        }
                        bx -= 1;
                    }
                }
            }
            y += 1;
        }
        // ---- after the loop (also after `break`)
        if height < 3 {
            if self.tree.borrow().fruit_count != 0 {
                self.tree.borrow_mut().fruit_count = 0;
                dw.dynamic_world_changed_at_pos_object_type(pos, obj_type);
            }
        } else {
            // quirk: in compost fruitCount becomes 2 but only slot 0 is written (slot 1 keeps stale data)
            let compost = tree_is_growing_in_compost(me);
            let mut s = self.tree.borrow_mut();
            s.tree_fruits[0].pos = IntPair { x: pos.x, y: pos.y + height - 1 };
            if s.fruit_count == 0 {
                s.fruit_count = if compost { 2 } else { 1 };
                drop(s);
                dw.dynamic_world_changed_at_pos_object_type(pos, obj_type);
            }
        }
    }

    fn species_update(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        tree_update(me, dt, accurate_dt, is_simulation);
        let (dead, height) = {
            let s = self.tree.borrow();
            (s.dead, s.height)
        };
        if dead || height <= 2 {
            return;
        }
        fruit_season_update(me, &FRUIT);
    }
}
