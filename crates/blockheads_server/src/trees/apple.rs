//! `AppleTree: Tree` (DynamicObjectType 1, TreeType 1, fruit ITEM_APPLE 21).
//!
//! (every `## -[AppleTree...]` heading); for the inherited behaviour.
//!
//! Extra ivar `availableFood` @0xb0 (+ own save key). Spawns `NPC_DODO` from the food accumulator.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::dynworld::state::DynamicWorld;
use crate::fmath::cconv::f32_to_i32;
use crate::objects::base::DynamicObjectBase;
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{alloc_obj, downcast, CacheRef, ObjRef};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::enums::TreeType;
use crate::types::structs::IntPair;
use crate::types::tile::Tile;
use crate::world::growth::growth_vigor_for_tree_type_at_pos;
use crate::world::state::World;

use super::tree::*;

/// `AppleTree: Tree`.
pub static APPLE_TREE_CLASS: ClassInfo = ClassInfo { name: "AppleTree", superclass: Some(&TREE_CLASS), protocols: &[] };

/// `NPC_DODO`. type:1`).
const NPC_DODO: i32 = 1;

/// The apple tree object.
pub struct AppleTree {
    base: RefCell<DynamicObjectBase>,
    tree: RefCell<TreeState>,
    /// 0xb0
    available_food: Cell<f32>,
}

crate::impl_tree_dynamic_object!(AppleTree, APPLE_TREE_CLASS, OBJECT_TYPE_APPLE);

/// Foliage `updateGrowth:` parameters.
static PARAMS: BushyParams = BushyParams {
    trunk: 4,
    bush: 3,
    trunk_bush: 5,
    k: 4,
    amp: 8.0,
    byte: SeasonByte::Mod255,
    pine_widths: false,
    clamp_widths: false,
    fruit: FruitThreshold::CompostExpert,
    apple_rescue: true,
    dw_on_fruit: true,
    cold_death: false,
};

fn idx(season_for_x: f32, t: &Tile, _tso: i32) -> i32 {
    idx_basic(season_for_x + 0.003f32 * ((t.season_offset as i32 - 127) as f32))
}
fn window(i: i32) -> bool {
    i == 1 || i == 2
}
/// Seasonal drop.
static FRUIT: FruitCfg =
    FruitCfg { item: 21, lookup_any: false, idx, window, min_height: 2, count: count_one, gene_data: true };

impl AppleTree {
    fn alloc() -> ObjRef {
        alloc_obj(|w| AppleTree {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            tree: RefCell::new(TreeState::new()),
            available_food: Cell::new(0.0),
        })
    }

    /// `-availableFood`.
    pub fn available_food(&self) -> f32 {
        self.available_food.get()
    }

    /// `-setAvailableFood:`: store and mark the block dirty.
    pub fn set_available_food(&self, me: &ObjRef, food: f32) {
        self.available_food.set(food);
        dw_changed(me);
    }

    /// `-initWithWorld:dynamicWorld:atPosition:cache:maxHeight:growthRate:treeDensityNoiseFunction:
    /// seasonOffsetNoiseFunction:adultTree:adultMaxAge:`.
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
        let this = downcast::<AppleTree>(&me).unwrap();
        let mhf = tree_state(&me).borrow().max_height_gene as f32 / 255.0f32;
        let vigor = growth_vigor_for_tree_type_at_pos(TreeType(TREE_APPLE as u32), me.pos(), Some(world));
        let mh = f32_to_i32(3.0f32 + (vigor * mhf) * 6.0f32);
        let (htg, ama) = init_height_target(&me, mh, adult_tree, adult_max_age);
        let ww = world.world_width_macro();
        let p = me.pos();
        let frac = season_noise_fraction(&me, p.x as f32 / ww as f32, p.y as f32 / ww as f32);
        tree_state(&me).borrow_mut().tree_season_offset = f32_to_i32(127.0f32 * frac);
        pre_grow(&me, htg, ama);
        compute_max_age(&me);
        this.available_food.set(((r01t("AppleTree.init.availableFood") * 900.0f32) as f64 * 0.5f64) as f32);
        Some(me)
    }

    /// `-initWithWorld:dynamicWorld:saveDict:cache:treeDensityNoiseFunction:seasonOffsetNoiseFunction:`.
    pub fn from_save_dict(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        d: &PlistDict,
        cache: CacheRef,
        tdnf: NoiseRef,
        sonf: NoiseRef,
    ) -> Option<ObjRef> {
        let me = Self::alloc();
        if !tree_init_with_world_dynamic_world_save_dict_cache_tdnf_sonf(&me, world, dw, d, cache, tdnf, sonf) {
            return None;
        }
        let v = d.map.get(&"availableFood".to_string());
        downcast::<AppleTree>(&me).unwrap().available_food.set(v.map_or(0.0, |v| v.float_value()));
        Some(me)
    }
}

impl TreeLike for AppleTree {
    fn tree(&self) -> &RefCell<TreeState> {
        &self.tree
    }
    fn tree_type(&self) -> i32 {
        TREE_APPLE
    }
    fn fruit_item_type(&self) -> i32 {
        21
    }
    fn fruit_should_fall_in_season(&self, season: i32) -> bool {
        season == 3
    }
    fn tile_is_kind_of_self(&self, t: &Tile) -> bool {
        matches!(t.contents, 3 | 4 | 5 | 0x1c | 0x1b)
    }
    fn make_tile_dead(&self, t: &mut Tile) {
        if t.contents == 3 {
            t.contents = 0x1b;
        } else if t.contents == 4 || t.contents == 5 {
            t.contents = 0x1c;
        }
    }
    fn update_growth(&self, me: &ObjRef, add_new_branch_blocks: bool) {
        bushy_update_growth(me, add_new_branch_blocks, &PARAMS)
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
            if let Some(tile) = tile_at(&w, pos.x, pos.y + 1) {
                if tile.get().back_wall_type_index == 2 && (r01t("AppleTree.update.dodo") as f64) > 0.8f64 {
                    if !dw.npc_exists_at_pos_ignore_npc(pos, None) {
                        dw.load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
                            pos, NPC_DODO, None, true, false, None,
                        );
                    }
                }
            }
        } else {
            self.available_food.set(0.5f32 * dt + food);
        }
        fruit_season_update(me, &FRUIT);
    }
    fn extend_save_dict(&self, d: &mut PlistDict) {
        d.map.insert("availableFood".to_string(), PlistValue::real_f32(self.available_food.get()));
    }
}
