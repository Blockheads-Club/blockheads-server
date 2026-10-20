//! `PineTree: Tree` (DynamicObjectType 4, TreeType 4, fruit ITEM_PINE_CONE 27).
//!
//!  PineTree" (lines 507-711);
//!  for the inherited behaviour. Extra ivar `availableFood` (a seconds timer, dropbear every 900 s);
//! `shouldAddFallenFruits` is NO; spawns `NPC_DROPBEAR` (adult from the constructor, child from `update:`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::dynworld::state::DynamicWorld;
use crate::fmath::cconv::f32_to_i32;
use crate::fmath::libm::fmodf;
use crate::objects::base::DynamicObjectBase;
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{alloc_obj, downcast, CacheRef, ObjRef};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::structs::IntPair;
use crate::types::tile::Tile;
use crate::world::state::World;

use super::tree::*;

/// `PineTree: Tree`.
pub static PINE_TREE_CLASS: ClassInfo = ClassInfo { name: "PineTree", superclass: Some(&TREE_CLASS), protocols: &[] };

/// `NPC_DROPBEAR`.
const NPC_DROPBEAR: i32 = 2;

/// The pine tree object (instance size 0xb8).
pub struct PineTree {
    base: RefCell<DynamicObjectBase>,
    tree: RefCell<TreeState>,
    /// 0xb0
    available_food: Cell<f32>,
}

crate::impl_tree_dynamic_object!(PineTree, PINE_TREE_CLASS, OBJECT_TYPE_PINE);

/// `((h+2) - y) * (0.4 + f * 0.2)`; fruit threshold constant 0.4).
static PARAMS: BushyParams = BushyParams {
    trunk: 7,
    bush: 6,
    trunk_bush: 8,
    k: 4,
    amp: 8.0,
    byte: SeasonByte::Clamp,
    pine_widths: true,
    clamp_widths: false,
    fruit: FruitThreshold::Const04,
    apple_rescue: false,
    dw_on_fruit: true,
    cold_death: false,
};

// Note: `tileAtRaw` is read as `tileAtWorldPosition` (no block load), see FRUIT below.
fn idx(season_for_x: f32, t: &Tile, _tso: i32) -> i32 {
    idx_basic(season_for_x + 0.05f32 * ((t.season_offset as i32 - 0x7f) as f32))
}
fn window(i: i32) -> bool {
    i == 2 || i == 3
}
/// `tileAtWorldPosition`, Note:).
static FRUIT: FruitCfg =
    FruitCfg { item: 27, lookup_any: true, idx, window, min_height: 2, count: count_one, gene_data: true };

impl PineTree {
    fn alloc() -> ObjRef {
        alloc_obj(|w| PineTree {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            tree: RefCell::new(TreeState::new()),
            available_food: Cell::new(0.0),
        })
    }

    /// `-availableFood`.
    pub fn available_food(&self) -> f32 {
        self.available_food.get()
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
        let this = downcast::<PineTree>(&me).unwrap();
        let p = me.pos();
        let mh = f32_to_i32(4.0f32 + 24.0f32 * (tree_state(&me).borrow().max_height_gene as f32 / 255.0f32));
        let (htg, ama) = init_height_target(&me, mh, adult_tree, adult_max_age);
        let ww = world.world_width_macro();
        let x = (((p.x as f64) * 1.1467f64) / (ww as f64)) as f32;
        let y = (((p.y as f64) * 4.37253f64) / (ww as f64)) as f32;
        let frac = season_noise_fraction(&me, x, y);
        tree_state(&me).borrow_mut().tree_season_offset = f32_to_i32(127.0f32 * frac);
        pre_grow(&me, htg, ama);
        compute_max_age(&me);
        this.available_food.set(r01t("PineTree.init.availableFood"));
        let (height, max_height) = {
            let s = tree_state(&me).borrow();
            (s.height, s.max_height)
        };
        if height > 7 && (r01t("PineTree.init.dropbear") as f64) < 0.5f64 * ((height as f32 / max_height as f32) as f64)
        {
            let sp = IntPair { x: p.x, y: p.y + height - 2 };
            if let Some(t) = tile_at(world, sp.x, sp.y) {
                if t.get().back_wall_type_index == 2
                    && !dw.npc_exists_at_pos_ignore_npc(sp, None)
                    && !world.tutorial_active()
                {
                    dw.load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
                        sp,
                        NPC_DROPBEAR,
                        None,
                        true,
                        false,
                        None,
                    );
                }
            }
        }
        Some(me)
    }

    /// `-initWithWorld:dynamicWorld:saveDict:cache:treeDensityNoiseFunction:seasonOffsetNoiseFunction:`:
    /// after the Tree load, the food timer catches up `0.5` per offline second (`saveTime` read as a float32).
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
        let get = |k: &str| d.map.get(&k.to_string());
        let food = get("availableFood").map_or(0.0f32, |v| v.float_value());
        let save_time = get("saveTime").map_or(0.0f32, |v| v.float_value());
        let now = world.world_time();
        downcast::<PineTree>(&me).unwrap().available_food.set((food as f64 + 0.5f64 * (now - save_time as f64)) as f32);
        Some(me)
    }
}

impl TreeLike for PineTree {
    fn tree(&self) -> &RefCell<TreeState> {
        &self.tree
    }
    fn tree_type(&self) -> i32 {
        TREE_PINE
    }
    fn fruit_item_type(&self) -> i32 {
        27
    }
    fn should_add_fallen_fruits(&self) -> bool {
        false
    }
    fn tile_is_kind_of_self(&self, t: &Tile) -> bool {
        matches!(t.contents, 7 | 6 | 8 | 0x22 | 0x1d)
    }
    fn make_tile_dead(&self, t: &mut Tile) {
        if t.contents == 6 {
            t.contents = 0x22;
        } else if t.contents == 7 || t.contents == 8 {
            t.contents = 0x1d;
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
        let mut food = self.available_food.get();
        if 900.0f32 <= food {
            food -= 900.0f32;
            if 900.0f32 < food {
                food = fmodf(food, 900.0f32);
            }
            self.available_food.set(food);
            let height = self.tree.borrow().height;
            let sp = IntPair { x: pos.x, y: pos.y + height - 2 };
            if let Some(t) = tile_at(&w, sp.x, sp.y) {
                if t.get().back_wall_type_index == 2 && height > 7 && 0.8f64 < (r01t("PineTree.update.dropbear") as f64)
                {
                    if !dw.npc_exists_at_pos_ignore_npc(sp, None) && !w.tutorial_active() {
                        dw.load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
                            sp,
                            NPC_DROPBEAR,
                            None,
                            false,
                            false,
                            None,
                        );
                    }
                }
            }
        } else {
            self.available_food.set((food as f64 + (dt as f64) * 0.5f64) as f32);
        }
        fruit_season_update(me, &FRUIT);
    }
    fn extend_save_dict(&self, d: &mut PlistDict) {
        d.map.insert("availableFood".to_string(), PlistValue::real_f32(self.available_food.get()));
    }
}
