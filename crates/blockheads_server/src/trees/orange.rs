//! `OrangeTree: Tree` (DynamicObjectType 7, TreeType 7, fruit ITEM_ORANGE 60).
//!

use std::cell::RefCell;
use std::rc::Rc;

use crate::dynworld::state::DynamicWorld;
use crate::fmath::cconv::f32_to_i32;
use crate::objects::base::DynamicObjectBase;
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{alloc_obj, CacheRef, ObjRef};
use crate::plist::value::PlistDict;
use crate::types::enums::TreeType;
use crate::types::structs::IntPair;
use crate::types::tile::Tile;
use crate::world::growth::growth_vigor_for_tree_type_at_pos;
use crate::world::state::World;

use super::tree::*;

/// `OrangeTree: Tree`.
pub static ORANGE_TREE_CLASS: ClassInfo =
    ClassInfo { name: "OrangeTree", superclass: Some(&TREE_CLASS), protocols: &[] };

/// The orange tree object (no added ivars, instance size 0xb0).
pub struct OrangeTree {
    base: RefCell<DynamicObjectBase>,
    tree: RefCell<TreeState>,
}

crate::impl_tree_dynamic_object!(OrangeTree, ORANGE_TREE_CLASS, OBJECT_TYPE_ORANGE);

/// Foliage `updateGrowth:` parameters.
/// A = 31.0; width clamp false; fruit threshold (compost ? -0.2: 0.2) + (expert ? 0.2: 0.0); cold death true).
static PARAMS: BushyParams = BushyParams {
    trunk: 19,
    bush: 18,
    trunk_bush: 20,
    k: 16,
    amp: 31.0,
    byte: SeasonByte::Mod255,
    pine_widths: false,
    clamp_widths: false,
    fruit: FruitThreshold::CompostExpert,
    apple_rescue: false,
    dw_on_fruit: false,
    cold_death: true,
};

fn idx(season_for_x: f32, t: &Tile, _tso: i32) -> i32 {
    idx_basic(season_for_x + 0.003f32 * ((t.season_offset as i32 + 128) as f32))
}
fn window(i: i32) -> bool {
    i == 6 || i == 7
}
/// Seasonal drop.
static FRUIT: FruitCfg =
    FruitCfg { item: 60, lookup_any: true, idx, window, min_height: 2, count: count_one, gene_data: true };

impl OrangeTree {
    fn alloc() -> ObjRef {
        alloc_obj(|w| OrangeTree {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            tree: RefCell::new(TreeState::new()),
        })
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
        let p = me.pos();
        let ww = world.world_width_macro();
        let dist_eq = distance_from_equator_fraction(p.x, ww);
        let mhf = tree_state(&me).borrow().max_height_gene as f32 / 255.0f32;
        let vigor = growth_vigor_for_tree_type_at_pos(TreeType(TREE_ORANGE as u32), p, Some(world));
        // f32 math, left to right: `4.0f + vigor * mhf * S`
        let mh = f32_to_i32(4.0f32 + vigor * mhf * 8.0f32);
        let (htg, ama) = init_height_target(&me, mh, adult_tree, adult_max_age);
        let frac = season_noise_fraction(&me, (p.x as f32 / 32.0f32) / ww as f32, (p.y as f32 / 32.0f32) / ww as f32);
        tree_state(&me).borrow_mut().tree_season_offset =
            f32_to_i32((f32_to_i32(127.0f32 * frac) as f32) * (1.0f32 - dist_eq) + 127.0f32);
        pre_grow(&me, htg, ama);
        compute_max_age(&me);
        Some(me)
    }

    /// `-initWithWorld:dynamicWorld:saveDict:cache:treeDensityNoiseFunction:seasonOffsetNoiseFunction:`
    /// (`return [super init...]`).
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

impl TreeLike for OrangeTree {
    fn tree(&self) -> &RefCell<TreeState> {
        &self.tree
    }
    fn tree_type(&self) -> i32 {
        TREE_ORANGE
    }
    fn fruit_item_type(&self) -> i32 {
        60
    }
    fn fruit_should_fall_in_season(&self, season: i32) -> bool {
        season == 0
    }
    fn tile_is_kind_of_self(&self, t: &Tile) -> bool {
        matches!(t.contents, 18 | 19 | 20 | 37 | 38)
    }
    fn make_tile_dead(&self, t: &mut Tile) {
        if t.contents == 18 {
            t.contents = 37;
        } else if t.contents == 19 || t.contents == 20 {
            t.contents = 38;
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
        fruit_season_update(me, &FRUIT);
    }
}
