//! `Tree`: abstract base of every tree (growth, light, ageing, death, tile ownership, save/load) plus the shared
//! building blocks the eleven species files use.
//!
//!
//! ## Shape
//! * [`TreeState`] holds the Tree ivars `0x4e..0xb0`; a concrete species embeds it (`tree: RefCell<TreeState>`)
//!   next to the [`DynamicObjectBase`](crate::objects::base::DynamicObjectBase).
//! * [`TreeLike`] is the virtual-dispatch trait for every Tree selector that is not a `DynamicObject` selector.
//!   Its defaults are the Tree base behaviour; the free `tree_*` functions are the `[super...]` targets.
//! * [`as_tree`] reaches the [`TreeLike`] view of an [`ObjRef`].
//! * [`BushyParams`] / [`bushy_update_growth`] and [`FruitCfg`] / [`fruit_season_update`] are the two code shapes
//!   that the species `updateGrowth:` / `update:` bodies share (Apple, Cherry, Coffee, Mango, Maple, Orange, Lime,
//!   Pine). Every per-species difference of the specs is a field of the parameter struct.
//!
//! ## RNG
//! libc `rand()` through `rng::glibc_rand`; `R01 = (float)rand() / 2147483648.0f` and may equal exactly 1.0.
//! The draw order of every function follows the spec text.
//!
//! ## Open points
//!
use std::any::Any;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::dynworld::state::{DwNoise, DynamicWorld};
use crate::fmath::cconv::{f32_to_i32, f64_to_i32};
use crate::objects::base::{
    base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_save_dict_cache, base_remove_from_macro_block, new_mutable_dict, DynamicObject,
    DynamicWorldRef, WorldRef,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{CacheRef, ObjRef};
use crate::plist::value::{PlistDict, PlistValue};
use crate::rng::glibc_rand;
use crate::types::enums::TreeType;
use crate::types::item_type::ItemType;
use crate::types::structs::IntPair;
use crate::types::tile::Tile;
use crate::world::climate::{current_temperature_for_tile_at_world_pos, season_for_world_x};
use crate::world::growth::growth_vigor_for_tree_type_at_pos;
use crate::world::helpers::{tile_is_bush, tile_is_dead_tree, tile_is_solid, tile_is_tree, tile_is_tree_trunk};
use crate::world::state::{TileRef, World};
use crate::world::tiles::{tile_at_world_position, tile_at_world_position_loaded};

use super::apple::AppleTree;
use super::cactus::CactusTree;
use super::cherry::CherryTree;
use super::coconut::CoconutTree;
use super::coffee::CoffeeTree;
use super::gem::GemTree;
use super::lime::LimeTree;
use super::mango::MangoTree;
use super::maple::MapleTree;
use super::orange::OrangeTree;
use super::pine::PineTree;

/// Noise function handle as the dynamic world stores it (`None` is a nil `NoiseFunction`, value `0.0`).
// Note: writes `Option<Rc<NoiseFunction>>`; dynamic_world_core stores `Rc<dyn DwNoise>`.
pub type NoiseRef = Option<Rc<dyn DwNoise>>;

/// Dynamic object type ids of the trees.
pub const OBJECT_TYPE_APPLE: i32 = 1;
pub const OBJECT_TYPE_MAPLE: i32 = 2;
pub const OBJECT_TYPE_MANGO: i32 = 3;
pub const OBJECT_TYPE_PINE: i32 = 4;
pub const OBJECT_TYPE_CACTUS: i32 = 5;
pub const OBJECT_TYPE_COCONUT: i32 = 6;
pub const OBJECT_TYPE_ORANGE: i32 = 7;
pub const OBJECT_TYPE_CHERRY: i32 = 8;
pub const OBJECT_TYPE_COFFEE: i32 = 9;
pub const OBJECT_TYPE_LIME: i32 = 37;
pub const OBJECT_TYPE_GEM: i32 = 57;

/// Tree types (`TreeType`).
pub const TREE_APPLE: i32 = 1;
pub const TREE_MANGO: i32 = 2;
pub const TREE_MAPLE: i32 = 3;
pub const TREE_PINE: i32 = 4;
pub const TREE_CACTUS: i32 = 5;
pub const TREE_COCONUT: i32 = 6;
pub const TREE_ORANGE: i32 = 7;
pub const TREE_CHERRY: i32 = 8;
pub const TREE_COFFEE: i32 = 9;
pub const TREE_LIME: i32 = 10;

/// `Tree: DynamicObject`.
pub static TREE_CLASS: ClassInfo = ClassInfo { name: "Tree", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &[] };

// ------------------------------------------------------------------------------------------------ RNG helpers

/// `rand()` with a trace tag.
#[inline]
pub(crate) fn rn(tag: &'static str) -> i32 {
    glibc_rand::rand_tagged(tag)
}

/// `R01 = (float)rand() / 2147483648.0f` with a trace tag.
#[inline]
pub(crate) fn r01t(tag: &'static str) -> f32 {
    glibc_rand::r01_tagged(tag)
}

/// `clampi(v, lo, hi)`.
#[inline]
pub(crate) fn clampi(v: i32, lo: i32, hi: i32) -> i32 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// `[nf getX:(double)x Y:(double)y octaves:n]`, `0.0` for a nil receiver.
#[inline]
pub(crate) fn noise(nf: &NoiseRef, x: f64, y: f64, octaves: i32) -> f64 {
    match nf {
        Some(n) => n.get_x_y_octaves(x, y, octaves),
        None => 0.0,
    }
}

// ------------------------------------------------------------------------------------------------ state

/// One `TreeFruit` record (12 bytes in the original; the 3 pad bytes are not observable).
#[derive(Clone, Copy, Default)]
pub struct TreeFruit {
    pub pos: IntPair,
    pub has_created_free_block_this_season: bool,
}

/// Tree ivars `0x4e..0xb0` (plus the world links the species code needs as concrete types).
pub struct TreeState {
    /// 0x4e
    pub max_height_gene: u16,
    /// 0x50
    pub growth_rate_gene: u16,
    /// 0x54
    pub height: i32,
    /// 0x58
    pub max_height_reached: i32,
    /// 0x5c
    pub growth_counter: f32,
    /// 0x60
    pub growth_rate: f32,
    /// 0x68
    pub tree_density_noise_function: NoiseRef,
    /// 0x70
    pub season_offset_noise_function: NoiseRef,
    /// 0x78
    pub tree_season_offset: i32,
    /// 0x7c
    pub max_height: i32,
    /// 0x80
    pub max_age: f32,
    /// 0x84
    pub age: f32,
    /// 0x88
    pub age_counter: f32,
    /// 0x8c
    pub dead: bool,
    /// 0x90
    pub time_died: f64,
    /// 0x98
    pub remove_check_count: f32,
    /// 0xa0 (capacity 64, never reallocated; stale slots survive a shrink, R7)
    pub tree_fruits: [TreeFruit; 64],
    /// 0xa8
    pub fruit_count: i32,
    /// 0xac
    pub no_light_die_timer: f32,
    /// `DynamicObject.world` as the concrete type (weak: the world owns the objects).
    pub world: Weak<World>,
    /// `DynamicObject.dynamicWorld` as the concrete type.
    pub dynamic_world: Weak<DynamicWorld>,
}

impl TreeState {
    /// All ivars zero / nil as after `alloc`.
    pub fn new() -> TreeState {
        TreeState {
            max_height_gene: 0,
            growth_rate_gene: 0,
            height: 0,
            max_height_reached: 0,
            growth_counter: 0.0,
            growth_rate: 0.0,
            tree_density_noise_function: None,
            season_offset_noise_function: None,
            tree_season_offset: 0,
            max_height: 0,
            max_age: 0.0,
            age: 0.0,
            age_counter: 0.0,
            dead: false,
            time_died: 0.0,
            remove_check_count: 0.0,
            tree_fruits: [TreeFruit::default(); 64],
            fruit_count: 0,
            no_light_die_timer: 0.0,
            world: Weak::new(),
            dynamic_world: Weak::new(),
        }
    }
}

impl Default for TreeState {
    fn default() -> Self {
        TreeState::new()
    }
}

/// Every overridable `Tree` selector with the Tree base behaviour as default.
pub trait TreeLike: DynamicObject {
    /// The Tree ivars.
    fn tree(&self) -> &RefCell<TreeState>;
    /// `-treeType`: TREE_NOTHING in the base (every species overrides it).
    fn tree_type(&self) -> i32;
    /// `-fruitItemType`: ITEM_NONE in the base (every species overrides it).
    fn fruit_item_type(&self) -> i32;
    /// `-shouldAddFallenFruits`: YES.
    fn should_add_fallen_fruits(&self) -> bool {
        true
    }
    /// `-fruitShouldFallInSeason:`: NO.
    fn fruit_should_fall_in_season(&self, _season: i32) -> bool {
        false
    }
    /// `-isRequiredSoilType:` (dirt family).
    fn is_required_soil_type(&self, tile_type: i32) -> bool {
        tree_is_required_soil_type(tile_type)
    }
    /// `-tileIsKindOfSelf:`: NO in the base (every species overrides it).
    fn tile_is_kind_of_self(&self, t: &Tile) -> bool;
    /// `-makeTileDead:`: empty in the base (every species overrides it).
    fn make_tile_dead(&self, t: &mut Tile);
    /// `-updateGrowth:`: empty in the base.
    fn update_growth(&self, _me: &ObjRef, _add_new_branch_blocks: bool) {}
    /// `-isStaticTree`: NO (GemTree: YES).
    fn is_static_tree(&self) -> bool {
        false
    }
    /// `-incrementHeight`.
    fn increment_height(&self) {
        let mut s = self.tree().borrow_mut();
        s.height = s.height.wrapping_add(1);
    }
    /// `-loadSaveDictValues:` (virtual; GemTree/CactusTree extend).
    fn load_save_dict_values(&self, me: &ObjRef, d: &PlistDict) {
        tree_load_save_dict_values(me, d)
    }
    /// Species `update:accurateDT:isSimulation:`; species overrides run [`tree_update`] first (`[super update:]`).
    fn species_update(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool);
    /// Species `worldChanged:`: the default is [`tree_world_changed`]; GemTree does NOT chain.
    fn species_world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        tree_world_changed(me, positions)
    }
    /// Species keys added to `getSaveDict` after the Tree keys (`availableFood`, `splitHeightA`,...).
    fn extend_save_dict(&self, _d: &mut PlistDict) {}
}

impl dyn TreeLike + '_ {
    /// `-maxHeightGeneVariation` (1 `rand()`).
    pub fn max_height_gene_variation(&self) -> u16 {
        let g = self.tree().borrow().max_height_gene;
        gene_variation(g)
    }
    /// `-growthRateGeneVariation` (1 `rand()`).
    pub fn growth_rate_gene_variation(&self) -> u16 {
        let g = self.tree().borrow().growth_rate_gene;
        gene_variation(g)
    }
    /// `-height`.
    pub fn height(&self) -> i32 {
        self.tree().borrow().height
    }
}

/// `clampi((int)((float)gene + (R01 - 0.5f) * 8.0f), 1, 255)`.
fn gene_variation(gene: u16) -> u16 {
    let r = r01t("Tree.geneVariation");
    clampi(f32_to_i32(gene as f32 + (r - 0.5f32) * 8.0f32), 1, 255) as u16
}

/// Tries every species type (the 11 tree classes are the only `TreeLike` implementors).
pub fn as_tree(me: &ObjRef) -> Option<&dyn TreeLike> {
    let a: &dyn Any = me.as_any();
    macro_rules! try_ty {
        ($($t:ty),*) => { $( if let Some(x) = a.downcast_ref::<$t>() { return Some(x as &dyn TreeLike); } )* };
    }
    try_ty!(
        AppleTree,
        MapleTree,
        MangoTree,
        PineTree,
        CactusTree,
        CoconutTree,
        OrangeTree,
        CherryTree,
        CoffeeTree,
        LimeTree,
        GemTree
    );
    None
}

#[inline]
fn tl(me: &ObjRef) -> &dyn TreeLike {
    as_tree(me).expect("not a Tree")
}

/// The Tree ivars of `me` (panics when `me` is not a tree).
pub fn tree_state(me: &ObjRef) -> &RefCell<TreeState> {
    tl(me).tree()
}

/// The concrete `World` of a tree. The world outlives its objects; a dangling link is a programming error.
pub fn tree_world(me: &ObjRef) -> Rc<World> {
    tl(me).tree().borrow().world.upgrade().expect("Tree: owning World was dropped")
}

/// The concrete `DynamicWorld` of a tree.
pub fn tree_dynamic_world(me: &ObjRef) -> Rc<DynamicWorld> {
    tl(me).tree().borrow().dynamic_world.upgrade().expect("Tree: owning DynamicWorld was dropped")
}

/// `tileAt(x, y)` = `tileAtWorldPositionLoaded(x, y, world)`.
#[inline]
pub fn tile_at(w: &World, x: i32, y: i32) -> Option<TileRef> {
    tile_at_world_position_loaded(x, y, w)
}

/// `tileAtAny(x, y)` = `tileAtWorldPosition(x, y, [world macroTiles], world)`.
#[inline]
pub fn tile_at_any(w: &World, x: i32, y: i32) -> Option<TileRef> {
    let mt = w.macro_tiles();
    tile_at_world_position(x, y, &mt, w)
}

/// `[dynamicWorld dynamicWorldChangedAtPos:pos objectType:[self objectType]]`.
pub fn dw_changed(me: &ObjRef) {
    tree_dynamic_world(me).dynamic_world_changed_at_pos_object_type(me.pos(), me.object_type());
}

/// `[world removeTileAtWorldX:x worldY:y createContentsFreeblockCount:cc createForegroundContentsFreeblockCount:0
/// removeBlockhead:nil]` (the 5-argument wrapper on `World`).
pub fn remove_tile(w: &World, x: i32, y: i32, cc: i32) {
    w.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead(x, y, cc, 0, None);
}

/// The Tree "mark dead" idiom: `dead = 1; timeDied = worldTime(); [self killAllOwnedTiles]; dwChanged`.
fn mark_dead(me: &ObjRef) {
    let wt = tree_world(me).world_time();
    {
        let mut s = tl(me).tree().borrow_mut();
        s.dead = true;
        s.time_died = wt;
    }
    tree_kill_all_owned_tiles(me);
    dw_changed(me);
}

fn is_compost_type(t: u8) -> bool {
    matches!(t, 48..=50)
}

// ------------------------------------------------------------------------------------------------ class table

/// `DynamicWorld.loadDynamicObjectsForMacroTile` path: object types 1..9, 37, 57 construct the species
/// through the 6-argument saveDict init (`loadSaveDictValues` virtual, then `growInTimeSinceSaved:`).
pub fn new_tree_from_save_dict(
    object_type: i32,
    world: &Rc<World>,
    dw: &Rc<DynamicWorld>,
    d: &PlistDict,
    cache: CacheRef,
    tdnf: NoiseRef,
    sonf: NoiseRef,
) -> Option<ObjRef> {
    match object_type {
        OBJECT_TYPE_APPLE => AppleTree::from_save_dict(world, dw, d, cache, tdnf, sonf),
        OBJECT_TYPE_MAPLE => MapleTree::from_save_dict(world, dw, d, cache, tdnf, sonf),
        OBJECT_TYPE_MANGO => MangoTree::from_save_dict(world, dw, d, cache, tdnf, sonf),
        OBJECT_TYPE_PINE => PineTree::from_save_dict(world, dw, d, cache, tdnf, sonf),
        OBJECT_TYPE_CACTUS => CactusTree::from_save_dict(world, dw, d, cache, tdnf, sonf),
        OBJECT_TYPE_COCONUT => CoconutTree::from_save_dict(world, dw, d, cache, tdnf, sonf),
        OBJECT_TYPE_ORANGE => OrangeTree::from_save_dict(world, dw, d, cache, tdnf, sonf),
        OBJECT_TYPE_CHERRY => CherryTree::from_save_dict(world, dw, d, cache, tdnf, sonf),
        OBJECT_TYPE_COFFEE => CoffeeTree::from_save_dict(world, dw, d, cache, tdnf, sonf),
        OBJECT_TYPE_LIME => LimeTree::from_save_dict(world, dw, d, cache, tdnf, sonf),
        OBJECT_TYPE_GEM => GemTree::from_save_dict(world, dw, d, cache, tdnf, sonf),
        _ => None,
    }
}

/// The single dispatcher used by `DynamicWorld.loadTreeAtPosition`. Runs the whole init
/// including its `rand()` draws; `None` when the Tree base rejects the tile (the draws stay consumed).
/// The caller registers the object.
#[allow(clippy::too_many_arguments)]
pub fn new_tree_for_type(
    tree_type: i32,
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
    match tree_type {
        TREE_APPLE => {
            AppleTree::new(world, dw, pos, cache, max_height, growth_rate, tdnf, sonf, adult_tree, adult_max_age)
        }
        TREE_MANGO => {
            MangoTree::new(world, dw, pos, cache, max_height, growth_rate, tdnf, sonf, adult_tree, adult_max_age)
        }
        TREE_MAPLE => {
            MapleTree::new(world, dw, pos, cache, max_height, growth_rate, tdnf, sonf, adult_tree, adult_max_age)
        }
        TREE_PINE => {
            PineTree::new(world, dw, pos, cache, max_height, growth_rate, tdnf, sonf, adult_tree, adult_max_age)
        }
        TREE_CACTUS => {
            CactusTree::new(world, dw, pos, cache, max_height, growth_rate, tdnf, sonf, adult_tree, adult_max_age)
        }
        TREE_COCONUT => {
            CoconutTree::new(world, dw, pos, cache, max_height, growth_rate, tdnf, sonf, adult_tree, adult_max_age)
        }
        TREE_ORANGE => {
            OrangeTree::new(world, dw, pos, cache, max_height, growth_rate, tdnf, sonf, adult_tree, adult_max_age)
        }
        TREE_CHERRY => {
            CherryTree::new(world, dw, pos, cache, max_height, growth_rate, tdnf, sonf, adult_tree, adult_max_age)
        }
        TREE_COFFEE => {
            CoffeeTree::new(world, dw, pos, cache, max_height, growth_rate, tdnf, sonf, adult_tree, adult_max_age)
        }
        TREE_LIME => {
            LimeTree::new(world, dw, pos, cache, max_height, growth_rate, tdnf, sonf, adult_tree, adult_max_age)
        }
        11..=15 => GemTree::new(world, dw, pos, cache, tdnf, sonf, tree_type),
        _ => None,
    }
}

// ------------------------------------------------------------------------------------------------ init

fn link_world(me: &ObjRef, world: &Rc<World>, dw: &Rc<DynamicWorld>, tdnf: NoiseRef, sonf: NoiseRef) {
    let mut s = tl(me).tree().borrow_mut();
    s.world = Rc::downgrade(world);
    s.dynamic_world = Rc::downgrade(dw);
    s.tree_density_noise_function = tdnf;
    s.season_offset_noise_function = sonf;
}

/// `-[Tree initStaticTreeWithWorld:dynamicWorld:atPosition:cache:treeDensityNoiseFunction:seasonOffsetNoiseFunction:]`
///  (GemTree). No `rand()`. `false` = the original returned nil (after `removeFromMacroBlock`).
pub fn tree_init_static_tree(
    me: &ObjRef,
    world: &Rc<World>,
    dw: &Rc<DynamicWorld>,
    pos: IntPair,
    cache: CacheRef,
    tdnf: NoiseRef,
    sonf: NoiseRef,
) -> bool {
    link_world(me, world, dw, tdnf, sonf);
    let wr: WorldRef = world.clone();
    let dr: DynamicWorldRef = dw.clone();
    // `base_init_with_world_dynamic_world_at_position_cache` answers true on every path
    let _ = base_init_with_world_dynamic_world_at_position_cache(me, &wr, &dr, pos, cache);
    let p = me.pos();
    match tile_at(world, p.x, p.y) {
        Some(t) if t.get().contents == 0 => true,
        _ => {
            me.remove_from_macro_block(me);
            false
        }
    }
}

/// `-[Tree initWithWorld:dynamicWorld:atPosition:cache:maxHeight:growthRate:treeDensityNoiseFunction:
/// seasonOffsetNoiseFunction:adultTree:adultMaxAge:]`. One `rand()` iff `adultTree`, drawn BEFORE the
/// tile validity check. `false` = nil.
#[allow(clippy::too_many_arguments)]
pub fn tree_init_with_world_dynamic_world_at_position_cache_max_height_growth_rate_tdnf_sonf_adult_tree_adult_max_age(
    me: &ObjRef,
    world: &Rc<World>,
    dw: &Rc<DynamicWorld>,
    pos: IntPair,
    cache: CacheRef,
    max_height: i16,
    growth_rate: i16,
    tdnf: NoiseRef,
    sonf: NoiseRef,
    adult_tree: bool,
    _adult_max_age: f32,
) -> bool {
    link_world(me, world, dw, tdnf, sonf);
    let wr: WorldRef = world.clone();
    let dr: DynamicWorldRef = dw.clone();
    // `base_init_with_world_dynamic_world_at_position_cache` answers true on every path
    let _ = base_init_with_world_dynamic_world_at_position_cache(me, &wr, &dr, pos, cache);
    let t = tl(me);
    let p = me.pos();
    let (mhg, grg) = (clampi(max_height as i32, 1, 255) as u16, clampi(growth_rate as i32, 1, 255) as u16);
    let mut gc = 0.0f32;
    if adult_tree {
        gc = r01t("Tree.init.growthCounter");
    }
    let f = grg as f32 / 255.0f32 + 0.2f32;
    let vigor = growth_vigor_for_tree_type_at_pos(TreeType(t.tree_type() as u32), p, Some(world));
    let base = crate::items::recipes::base_growth_rate_for_tree_type(TreeType(t.tree_type() as u32));
    let mut rate = (f * vigor) * base;
    {
        let mut s = t.tree().borrow_mut();
        s.max_height_gene = mhg;
        s.growth_rate_gene = grg;
        s.growth_counter = gc;
    }
    match tile_at(world, p.x, p.y) {
        Some(tile) if tile.get().contents == 0 => {}
        _ => {
            me.remove_from_macro_block(me);
            return false;
        }
    }
    if let Some(b) = tile_at(world, p.x, p.y - 1) {
        if is_compost_type(b.get().type_index) {
            rate *= 2.0f32;
        }
    }
    t.tree().borrow_mut().growth_rate = rate;
    true
}

/// `-[Tree initWithWorld:dynamicWorld:saveDict:cache:treeDensityNoiseFunction:seasonOffsetNoiseFunction:]`
/// : base load, virtual `loadSaveDictValues:`, then `growInTimeSinceSaved:`.
pub fn tree_init_with_world_dynamic_world_save_dict_cache_tdnf_sonf(
    me: &ObjRef,
    world: &Rc<World>,
    dw: &Rc<DynamicWorld>,
    d: &PlistDict,
    cache: CacheRef,
    tdnf: NoiseRef,
    sonf: NoiseRef,
) -> bool {
    link_world(me, world, dw, tdnf, sonf);
    let wr: WorldRef = world.clone();
    let dr: DynamicWorldRef = dw.clone();
    // `base_init_with_world_dynamic_world_save_dict_cache` answers true on every path
    let _ = base_init_with_world_dynamic_world_save_dict_cache(me, &wr, &dr, d, cache);
    tl(me).load_save_dict_values(me, d);
    let save_time = d.map.get(&"saveTime".to_string()).map_or(0.0, |v| v.double_value());
    tree_grow_in_time_since_saved(me, save_time);
    true
}

// ------------------------------------------------------------------------------------------------ simple queries

/// `-[Tree removeFromMacroBlock]`: `removeAllOwnedTiles:NO`, then the base.
pub fn tree_remove_from_macro_block(me: &ObjRef) {
    tree_remove_all_owned_tiles(me, false);
    base_remove_from_macro_block(me);
}

/// `-[Tree isGrowingInCompost]`.
pub fn tree_is_growing_in_compost(me: &ObjRef) -> bool {
    let w = tree_world(me);
    let p = me.pos();
    matches!(tile_at(&w, p.x, p.y - 1), Some(b) if is_compost_type(b.get().type_index))
}

/// `-[Tree isRequiredSoilType:]`: TILE_DIRT(6), DIRT_GRASS(27), DIRT_GRASS_FROZEN(28), COMPOST(48),
/// COMPOST_GRASS(49), COMPOST_GRASS_FROZEN(50).
pub fn tree_is_required_soil_type(tile_type: i32) -> bool {
    matches!(tile_type, 6 | 27 | 28 | 48 | 49 | 50)
}

/// `-[Tree maxHeightGeneVariation]` (1 `rand()`).
pub fn tree_max_height_gene_variation(me: &ObjRef) -> u16 {
    let g = tl(me).tree().borrow().max_height_gene;
    gene_variation(g)
}

/// `-[Tree growthRateGeneVariation]` (1 `rand()`).
pub fn tree_growth_rate_gene_variation(me: &ObjRef) -> u16 {
    let g = tl(me).tree().borrow().growth_rate_gene;
    gene_variation(g)
}

/// `-[Tree height]`.
pub fn tree_height(me: &ObjRef) -> i32 {
    tl(me).tree().borrow().height
}

// ------------------------------------------------------------------------------------------------ fallen fruits

/// `-[Tree addFallenFruits]` (generic; the species `update:` bodies do not call it,).
pub fn tree_add_fallen_fruits(me: &ObjRef) {
    let t = tl(me);
    let w = tree_world(me);
    let dw = tree_dynamic_world(me);
    let p = me.pos();
    let uid = me.unique_id();
    let season = season_for_world_x(p.x, w.world_time(), Some(&w));
    let count = t.tree().borrow().fruit_count;
    for i in 0..count {
        let fruit = t.tree().borrow().tree_fruits[i as usize];
        let Some(tr) = tile_at(&w, fruit.pos.x, fruit.pos.y) else { continue };
        let tile = tr.get();
        if t.tile_is_kind_of_self(&tile) && tile.dynamic_object_owner == uid {
            let v = 8.0f32 * (season + 0.003f32 * ((tile.season_offset as i32 - 127) as f32)) + 4.0f32;
            let season_idx = f32_to_i32(v).wrapping_rem(8);
            let height = t.tree().borrow().height;
            if t.fruit_should_fall_in_season(season_idx) && height > 2 {
                for offset in 0..16 {
                    let bp = IntPair { x: fruit.pos.x, y: fruit.pos.y - offset - 1 };
                    let bt = tile_at(&w, bp.x, bp.y);
                    if bt.is_some() && tile_is_solid(bt) {
                        let item = t.fruit_item_type();
                        let a = (t as &dyn TreeLike).max_height_gene_variation();
                        let b = (t as &dyn TreeLike).growth_rate_gene_variation();
                        dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                            IntPair { x: bp.x, y: bp.y + 1 },
                            ItemType(item as u32),
                            a,
                            b,
                            None,
                            None,
                            false,
                            0,
                            None,
                        );
                        break;
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------ offline catch-up

/// `-[Tree growInTimeSinceSaved:]`.
pub fn tree_grow_in_time_since_saved(me: &ObjRef, save_time: f64) {
    let t = tl(me);
    if t.is_static_tree() {
        return;
    }
    let w = tree_world(me);
    let dw = tree_dynamic_world(me);
    let p = me.pos();
    let mut did_grow = false;
    if !t.tree().borrow().dead {
        let mut td = w.world_time();
        td -= save_time;
        let (max_age, age) = {
            let s = t.tree().borrow();
            (s.max_age, s.age)
        };
        if (max_age as f64) <= (age as f64) + td {
            if !tree_is_growing_in_compost(me) {
                dw.sow_tree_near_parent_adult_adult_max_age(me, true, (td - (max_age - age) as f64) as f32);
                {
                    let mut s = t.tree().borrow_mut();
                    s.dead = true;
                    s.time_died = (save_time + s.max_age as f64) - s.age as f64;
                }
                tree_remove_all_owned_tiles(me, false);
            } else {
                let mut s = t.tree().borrow_mut();
                s.age = s.max_age;
            }
        } else {
            let mut remainder = 1.0f32 - t.tree().borrow().growth_counter;
            loop {
                let (height, max_height) = {
                    let s = t.tree().borrow();
                    (s.height, s.max_height)
                };
                if !(height < max_height && 0.0 < td) {
                    break;
                }
                let top = tile_at(&w, p.x, p.y.wrapping_add(height));
                let mut light = 0.5f32;
                if let Some(tt) = top {
                    let tt = tt.get();
                    light = ((tt.sun_light as f32) * 0.5f32) / 255.0f32
                        + (((tt.artificial_light_r as i32) / 4
                            + (tt.artificial_light_g as i32) / 4
                            + (tt.artificial_light_b as i32) / 2)
                            / 1024) as f32;
                }
                let (growth_rate, gc) = {
                    let s = t.tree().borrow();
                    (s.growth_rate, s.growth_counter)
                };
                let tn = (remainder as f64
                    / (0.005f64
                        * ((((1.0f32 - height as f32 / max_height as f32) + 0.2f32) * 0.5f32) as f64)
                        * growth_rate as f64
                        * light as f64)) as f32;
                if td <= tn as f64 {
                    t.tree().borrow_mut().growth_counter = gc + (1.0f32 - gc) * ((td / tn as f64) as f32);
                    td = -1.0;
                } else {
                    td -= tn as f64;
                    {
                        let mut s = t.tree().borrow_mut();
                        s.age = tn + s.age;
                        s.growth_counter = 0.0;
                    }
                    t.increment_height();
                    {
                        let mut s = t.tree().borrow_mut();
                        if s.max_height_reached < s.height {
                            s.max_height_reached = s.height;
                        }
                    }
                    t.update_growth(me, true);
                    remainder = 1.0f32;
                    did_grow = true;
                }
            }
        }
    }
    if !did_grow && !t.tree().borrow().dead {
        t.update_growth(me, false);
    }
}

// ------------------------------------------------------------------------------------------------ save / load

/// `-[Tree loadSaveDictValues:]`. Missing keys read as 0 / 0.0 / NO (nil messaging).
pub fn tree_load_save_dict_values(me: &ObjRef, d: &PlistDict) {
    let t = tl(me);
    let w = tree_world(me);
    let uid = me.unique_id();
    let get = |k: &str| d.map.get(&k.to_string());
    let int = |k: &str| get(k).map_or(0, |v| v.int_value());
    let flt = |k: &str| get(k).map_or(0.0, |v| v.float_value());
    {
        let mut s = t.tree().borrow_mut();
        s.tree_season_offset = int("treeSeasonOffset");
        s.dead = get("dead").map_or(false, |v| v.bool_value());
        s.time_died = get("timeDied").map_or(0.0, |v| v.double_value());
        s.remove_check_count = flt("removeCheckCount");
        s.fruit_count = 0;
    }
    if let Some(PlistValue::Array(a)) = get("treeFruit") {
        for fd in a.items.iter() {
            let fx = fd.object_for_key("pos.x").map_or(0, |v| v.int_value());
            let fy = fd.object_for_key("pos.y").map_or(0, |v| v.int_value());
            let Some(tr) = tile_at(&w, fx, fy) else { continue };
            let tile = tr.get();
            if tile.dynamic_object_owner == uid && t.tile_is_kind_of_self(&tile) {
                let mut s = t.tree().borrow_mut();
                let n = s.fruit_count;
                if (0..64).contains(&n) {
                    s.tree_fruits[n as usize] = TreeFruit {
                        pos: IntPair { x: fx, y: fy },
                        has_created_free_block_this_season: fd
                            .object_for_key("hasCreatedFreeBlockThisSeason")
                            .map_or(false, |v| v.bool_value()),
                    };
                    s.fruit_count = n + 1;
                }
            }
        }
    }
    let is_static = t.is_static_tree();
    let mut s = t.tree().borrow_mut();
    s.height = int("height");
    s.age = flt("age");
    if !is_static {
        s.growth_counter = flt("growthCounter");
        s.growth_rate = flt("growthRate");
        s.max_height = int("maxHeight");
        s.max_height_gene = int("maxHeightGene") as u16;
        s.growth_rate_gene = int("growthRateGene") as u16;
        s.max_height_reached = int("maxHeightReached");
        s.max_height_gene = clampi(s.max_height_gene as i32, 1, 255) as u16;
        s.growth_rate_gene = clampi(s.growth_rate_gene as i32, 1, 255) as u16;
        s.max_age = flt("maxAge");
    }
}

/// `-[Tree getSaveDict]`. NSNumber kinds: int / float / double / bool exactly as the spec lists.
pub fn tree_get_save_dict(me: &ObjRef) -> PlistDict {
    let t = tl(me);
    let wt = tree_world(me).world_time();
    let mut d = base_get_save_dict(me);
    let s = t.tree().borrow();
    let mut put = |k: &str, v: PlistValue| {
        d.map.insert(k.to_string(), v);
    };
    put("height", PlistValue::int(s.height as i64));
    put("saveTime", PlistValue::real_f64(wt));
    put("treeSeasonOffset", PlistValue::int(s.tree_season_offset as i64));
    put("age", PlistValue::real_f32(s.age));
    put("dead", PlistValue::boolean(s.dead));
    put("timeDied", PlistValue::real_f64(s.time_died));
    put("removeCheckCount", PlistValue::real_f32(s.remove_check_count));
    let mut arr = PlistValue::mutable_array();
    if let PlistValue::Array(a) = &mut arr {
        for i in 0..s.fruit_count.clamp(0, 64) as usize {
            let f = &s.tree_fruits[i];
            let mut fd = new_mutable_dict();
            fd.map.insert("pos.x".to_string(), PlistValue::int(f.pos.x as i64));
            fd.map.insert("pos.y".to_string(), PlistValue::int(f.pos.y as i64));
            fd.map.insert(
                "hasCreatedFreeBlockThisSeason".to_string(),
                PlistValue::boolean(f.has_created_free_block_this_season),
            );
            a.items.push(PlistValue::Dict(fd));
        }
    }
    put("treeFruit", arr);
    if !t.is_static_tree() {
        put("maxHeightGene", PlistValue::int(s.max_height_gene as i64));
        put("growthRateGene", PlistValue::int(s.growth_rate_gene as i64));
        put("maxHeightReached", PlistValue::int(s.max_height_reached as i64));
        put("growthCounter", PlistValue::real_f32(s.growth_counter));
        put("growthRate", PlistValue::real_f32(s.growth_rate));
        put("maxHeight", PlistValue::int(s.max_height as i64));
        put("maxAge", PlistValue::real_f32(s.max_age));
    }
    d
}

// ------------------------------------------------------------------------------------------------ worldChanged

/// `-[Tree worldChanged:]`.
pub fn tree_world_changed(me: &ObjRef, positions: &mut Vec<IntPair>) {
    let t = tl(me);
    let w = tree_world(me);
    let pos = me.pos();
    let uid = me.unique_id();
    let obj_type = me.object_type();
    for &p in positions.clone().iter() {
        let height = t.tree().borrow().height;
        if !(p.y <= pos.y.wrapping_add(height) && pos.y - 1 <= p.y) {
            continue;
        }
        if p.x == pos.x {
            // soil check
            if !t.tree().borrow().dead && p.y == pos.y - 1 {
                let below = tile_at(&w, pos.x, pos.y - 1);
                if let Some(b) = below {
                    if !t.is_required_soil_type(b.get().type_index as i32) && !t.tree().borrow().dead {
                        mark_dead(me);
                    }
                }
            }
            // trunk integrity: first trunk tile that is no longer ours
            let mut y = 0;
            loop {
                let height = t.tree().borrow().height;
                if y >= height {
                    break;
                }
                if let Some(tr) = tile_at(&w, pos.x, pos.y.wrapping_add(y)) {
                    if !t.tile_is_kind_of_self(&tr.get()) {
                        if y == 0 {
                            if !t.tree().borrow().dead {
                                {
                                    let mut s = t.tree().borrow_mut();
                                    s.dead = true;
                                    s.time_died = w.world_time();
                                }
                                tree_dynamic_world(me).dynamic_world_changed_at_pos_object_type(pos, obj_type);
                            }
                            tree_kill_all_owned_tiles_above_y(me, pos.y - 1);
                            me.base().borrow_mut().needs_removed = true;
                        } else {
                            {
                                let mut s = t.tree().borrow_mut();
                                s.height = y;
                                s.growth_counter = 0.0;
                            }
                            tree_dynamic_world(me).dynamic_world_changed_at_pos_object_type(pos, obj_type);
                            tree_kill_all_owned_tiles_above_y(me, pos.y.wrapping_add(y) - 1);
                        }
                        break;
                    }
                }
                y += 1;
            }
        } else {
            // branch / leaf cleanup
            let height = t.tree().borrow().height;
            if (p.x.wrapping_sub(pos.x)).wrapping_abs() < height {
                let ct = tile_at(&w, p.x, p.y);
                if let Some(c) = ct {
                    if !t.tile_is_kind_of_self(&c.get()) {
                        let expert = w.expert_mode();
                        let scan = |x: i32| {
                            if let Some(tr) = tile_at(&w, x, p.y) {
                                let tt = tr.get();
                                if tt.dynamic_object_owner == uid && tt.on_fire == 0 && t.tile_is_kind_of_self(&tt) {
                                    let mut flag = false;
                                    if !expert {
                                        flag = 0.8f32 < r01t("Tree.worldChanged");
                                    }
                                    remove_tile(&w, x, p.y, flag as i32);
                                }
                            }
                        };
                        if pos.x < p.x {
                            let mut x = p.x + 1;
                            while x < pos.x.wrapping_add(height) {
                                scan(x);
                                x += 1;
                            }
                        } else {
                            let mut x = p.x - 1;
                            while pos.x.wrapping_sub(height) < x {
                                scan(x);
                                x -= 1;
                            }
                        }
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------ update

/// `LIGHT(t)` of `Tree.update`: `(float)(((double)sun * m * 2.0) / 255.0 + (double)(int-division term / 255))`.
fn update_light(t: &Tile, m: f64) -> f32 {
    let art =
        ((t.artificial_light_r as i32) / 4 + (t.artificial_light_g as i32) / 4 + (t.artificial_light_b as i32) / 2)
            / 255;
    (((t.sun_light as f64) * m * 2.0) / 255.0 + art as f64) as f32
}

/// `-[Tree update:accurateDT:isSimulation:]`. No `rand()` in this method itself.
pub fn tree_update(me: &ObjRef, dt: f32, _accurate_dt: f32, _is_simulation: bool) {
    let t = tl(me);
    let w = tree_world(me);
    let dw = tree_dynamic_world(me);
    let pos = me.pos();
    let obj_type = me.object_type();
    let dead = t.tree().borrow().dead;
    if !dead {
        // ageCounter flush
        let flush = {
            let mut s = t.tree().borrow_mut();
            s.age_counter += dt;
            if 1000.0f32 * dt <= s.age_counter {
                s.age += s.age_counter;
                s.age_counter = 0.0;
                true
            } else {
                false
            }
        };
        if flush {
            dw.dynamic_world_changed_at_pos_object_type(pos, obj_type);
            let over = {
                let s = t.tree().borrow();
                s.max_age < s.age
            };
            if over && tree_is_growing_in_compost(me) {
                let mut s = t.tree().borrow_mut();
                s.age = s.max_age;
            }
        }
        if !t.is_static_tree() {
            let height = t.tree().borrow().height;
            let tod = w.time_of_day_fraction();
            let m: f64 = if (tod as f64) <= 0.2f64 { 0.2f64 } else { tod as f64 };
            let mut light = 1.0f32;
            if let Some(top) = tile_at(&w, pos.x, pos.y.wrapping_add(height)) {
                light = update_light(&top.get(), m);
            }
            if let Some(up1) = tile_at(&w, pos.x, pos.y + 1) {
                let l = update_light(&up1.get(), m);
                if l > light {
                    light = l;
                }
            }
            let over_age = {
                let s = t.tree().borrow();
                s.max_age < s.age
            } && !tree_is_growing_in_compost(me);
            let too_high = pos.y > 0x3df;
            if over_age || (light as f64) < 0.05f64 || too_high {
                let mut die = over_age || too_high;
                if (light as f64) < 0.05f64 {
                    let mut s = t.tree().borrow_mut();
                    s.no_light_die_timer += dt;
                    if 20.0f32 < s.no_light_die_timer {
                        die = true;
                    }
                }
                if die {
                    mark_dead(me);
                }
            } else {
                let ready = {
                    let mut s = t.tree().borrow_mut();
                    s.no_light_die_timer = 0.0;
                    let inner = ((1.0f32 - s.height as f32 / s.max_height as f32) + 0.2f32) * s.growth_rate;
                    s.growth_counter =
                        (s.growth_counter as f64 + (dt as f64) * 0.005f64 * (inner as f64) * (light as f64)) as f32;
                    s.growth_counter >= 1.0f32
                };
                if ready {
                    let flooded = matches!(tile_at(&w, pos.x, pos.y), Some(tr) if tr.get().type_index == 3);
                    let (max_height, height) = {
                        let s = t.tree().borrow();
                        (s.max_height, s.height)
                    };
                    if max_height <= height || flooded {
                        if !tree_is_growing_in_compost(me) {
                            mark_dead(me);
                        }
                    } else {
                        {
                            let mut s = t.tree().borrow_mut();
                            s.growth_counter = 0.0;
                        }
                        t.increment_height();
                        {
                            let mut s = t.tree().borrow_mut();
                            if s.max_height_reached < s.height {
                                s.max_height_reached = s.height;
                            }
                        }
                        t.update_growth(me, true);
                        let at_max = {
                            let s = t.tree().borrow();
                            s.height == s.max_height
                        };
                        if at_max && !tree_is_growing_in_compost(me) {
                            dw.sow_tree_near_parent_adult_adult_max_age(me, false, 99999.0f32);
                        }
                        dw.dynamic_world_changed_at_pos_object_type(pos, obj_type);
                    }
                }
            }
        }
    } else if !me.needs_removed() {
        let time_died = t.tree().borrow().time_died;
        if w.world_time() - time_died <= 60.0f64 {
            let check = {
                let mut s = t.tree().borrow_mut();
                s.remove_check_count += dt;
                if s.remove_check_count > 1.0f32 {
                    s.remove_check_count -= 1.0f32;
                    true
                } else {
                    false
                }
            };
            if check {
                tree_check_if_dead_tiles_need_removed(me);
            }
        } else {
            me.base().borrow_mut().needs_removed = true;
            tree_remove_all_owned_tiles(me, true);
        }
    }
}

// ------------------------------------------------------------------------------------------------ owned tiles

/// `(pos, maxHeightReached)` of the owned-tile scans: rows `pos.y..< pos.y + mhr`, columns
/// `pos.x - mhr - 1..< pos.x + mhr + 1`.
fn scan_bounds(me: &ObjRef) -> (IntPair, i32) {
    (me.pos(), tl(me).tree().borrow().max_height_reached)
}

/// `-[Tree killAllOwnedTiles]`.
pub fn tree_kill_all_owned_tiles(me: &ObjRef) {
    let t = tl(me);
    let w = tree_world(me);
    let dw = tree_dynamic_world(me);
    let uid = me.unique_id();
    let (pos, mhr) = scan_bounds(me);
    let mut y = pos.y;
    while y < pos.y.wrapping_add(mhr) {
        let mut x = pos.x.wrapping_sub(mhr).wrapping_sub(1);
        while x < pos.x.wrapping_add(mhr).wrapping_add(1) {
            if let Some(tr) = tile_at(&w, x, y) {
                let tt = tr.get();
                if tt.dynamic_object_owner == uid && tt.on_fire == 0 {
                    tr.update(|tile| t.make_tile_dead(tile));
                    dw.world_contents_changed_at_pos(IntPair { x, y });
                }
            }
            x += 1;
        }
        y += 1;
    }
}

/// `-[Tree checkIfDeadTilesNeedRemoved]`.
pub fn tree_check_if_dead_tiles_need_removed(me: &ObjRef) {
    let w = tree_world(me);
    let uid = me.unique_id();
    let (pos, mhr) = scan_bounds(me);
    let time_died = tl(me).tree().borrow().time_died;
    let elapsed = w.world_time() - time_died;
    let iy = f64_to_i32(((pos.y as f64) + (mhr as f64) * (1.0f64 - elapsed / 60.0f64)) - 1.0f64);
    let expert = w.expert_mode();
    let mut y = iy;
    loop {
        y = y.wrapping_add(1);
        if !(y < pos.y.wrapping_add(mhr)) {
            break;
        }
        let mut x = pos.x.wrapping_sub(mhr).wrapping_sub(1);
        while x < pos.x.wrapping_add(mhr).wrapping_add(1) {
            if let Some(tr) = tile_at(&w, x, y) {
                let tt = tr.get();
                if tt.dynamic_object_owner == uid && tt.on_fire == 0 {
                    let sf = tt.season_offset as f32 / 255.0f32;
                    let d = ((y as f32) - (iy as f32)) as f64 * 0.7f64;
                    let trunk_bias = if tile_is_tree_trunk(Some(tr)) { 0.7f32 } else { 0.0f32 };
                    if 2.5999999046325684f64 < ((sf as f64) + d) - (trunk_bias as f64) {
                        let mut flag = false;
                        if !expert {
                            flag = 0.95f32 < r01t("Tree.checkDead");
                        }
                        remove_tile(&w, x, y, flag as i32);
                    }
                }
            }
            x += 1;
        }
    }
}

/// `-[Tree killAllOwnedTilesAboveY:]`: y outer ascending, x inner ascending, `0.8f < R01` per
/// qualifying tile in non-expert mode.
pub fn tree_kill_all_owned_tiles_above_y(me: &ObjRef, y_min: i32) {
    let t = tl(me);
    let w = tree_world(me);
    let uid = me.unique_id();
    let (pos, mhr) = scan_bounds(me);
    let expert = w.expert_mode();
    let mut y = y_min.wrapping_add(1);
    while y < pos.y.wrapping_add(mhr) {
        let mut x = pos.x.wrapping_sub(mhr).wrapping_sub(1);
        while x < pos.x.wrapping_add(mhr).wrapping_add(1) {
            if let Some(tr) = tile_at(&w, x, y) {
                let tt = tr.get();
                if tt.dynamic_object_owner == uid && tt.on_fire == 0 && t.tile_is_kind_of_self(&tt) {
                    let mut flag = false;
                    if !expert {
                        flag = 0.8f32 < r01t("Tree.killAboveY");
                    }
                    remove_tile(&w, x, y, flag as i32);
                }
            }
            x += 1;
        }
        y += 1;
    }
}

/// `-[Tree removeAllOwnedTiles:]`: `rand()` only when `createFreeblocks && !expert`.
pub fn tree_remove_all_owned_tiles(me: &ObjRef, create_freeblocks: bool) {
    let t = tl(me);
    let w = tree_world(me);
    let uid = me.unique_id();
    let (pos, mhr) = scan_bounds(me);
    let expert = w.expert_mode();
    let mut y = pos.y;
    while y < pos.y.wrapping_add(mhr) {
        let mut x = pos.x.wrapping_sub(mhr).wrapping_sub(1);
        while x < pos.x.wrapping_add(mhr).wrapping_add(1) {
            if let Some(tr) = tile_at(&w, x, y) {
                let tt = tr.get();
                if tt.dynamic_object_owner == uid && tt.on_fire == 0 && t.tile_is_kind_of_self(&tt) {
                    let mut flag = false;
                    if create_freeblocks && !expert {
                        flag = 0.8f32 < r01t("Tree.removeAll");
                    }
                    remove_tile(&w, x, y, flag as i32);
                }
            }
            x += 1;
        }
        y += 1;
    }
}

/// `-[Tree updateAllOwnedTilesToNewIDSize]`: save migration from 32-bit to 64-bit owner ids.
pub fn tree_update_all_owned_tiles_to_new_id_size(me: &ObjRef) {
    let w = tree_world(me);
    let uid = me.unique_id();
    let (pos, mhr) = scan_bounds(me);
    let mut y = pos.y;
    while y < pos.y.wrapping_add(mhr) {
        let mut x = pos.x.wrapping_sub(mhr).wrapping_sub(1);
        while x < pos.x.wrapping_add(mhr).wrapping_add(1) {
            if let Some(tr) = tile_at(&w, x, y) {
                if tr.get().dynamic_object_owner_old as u64 == uid {
                    tr.update(|tile| tile.dynamic_object_owner = uid);
                }
            }
            x += 1;
        }
        y += 1;
    }
}

// ------------------------------------------------------------------------------------------------ DynamicObject glue

/// Generates the `DynamicObject` impl every tree species shares: class info, `objectType`,
/// `occupiesNormalContents` (YES), `removeFromMacroBlock`, `update:`, `worldChanged:`, `getSaveDict`.
#[macro_export]
macro_rules! impl_tree_dynamic_object {
    ($ty:ty, $class:expr, $otype:expr) => {
        impl $crate::objects::base::DynamicObject for $ty {
            fn base(&self) -> &::std::cell::RefCell<$crate::objects::base::DynamicObjectBase> {
                &self.base
            }
            fn class_info(&self) -> &'static $crate::objects::class_info::ClassInfo {
                &$class
            }
            fn as_any(&self) -> &dyn ::std::any::Any {
                self
            }
            fn object_type(&self) -> i32 {
                $otype
            }
            fn occupies_normal_contents(&self) -> bool {
                true
            }
            fn remove_from_macro_block(&self, me: &$crate::objects::handle::ObjRef) {
                $crate::trees::tree::tree_remove_from_macro_block(me)
            }
            fn update_accurate_dt_is_simulation(
                &self,
                me: &$crate::objects::handle::ObjRef,
                dt: f32,
                accurate_dt: f32,
                is_simulation: bool,
            ) {
                $crate::trees::tree::TreeLike::species_update(self, me, dt, accurate_dt, is_simulation)
            }
            fn world_changed(
                &self,
                me: &$crate::objects::handle::ObjRef,
                positions: &mut Vec<$crate::types::structs::IntPair>,
            ) {
                $crate::trees::tree::TreeLike::species_world_changed(self, me, positions)
            }
            fn get_save_dict(&self, me: &$crate::objects::handle::ObjRef) -> $crate::plist::value::PlistDict {
                let mut d = $crate::trees::tree::tree_get_save_dict(me);
                $crate::trees::tree::TreeLike::extend_save_dict(self, &mut d);
                d
            }
        }
    };
}

// ------------------------------------------------------------------------------------------------ shared init pieces

/// Shared constructor skeleton steps 1-3: compost `maxHeight * 1.5f`, `heightToGrowTo`
/// (`rand() % maxHeight` iff adult, minimum 1), `height = 0`. Stores `maxHeight`. Returns
/// `(heightToGrowTo, adultMaxAge)` where `adultMaxAge` is `99999.0f` for a non-adult.
pub fn init_height_target(me: &ObjRef, mut max_height: i32, adult_tree: bool, adult_max_age: f32) -> (i32, f32) {
    let t = tl(me);
    let w = tree_world(me);
    let p = me.pos();
    if let Some(b) = tile_at(&w, p.x, p.y - 1) {
        if is_compost_type(b.get().type_index) {
            max_height = f32_to_i32(max_height as f32 * 1.5f32);
        }
    }
    let mut htg = 1;
    let mut ama = adult_max_age;
    if !adult_tree {
        ama = 99999.0f32;
    } else {
        let r = rn("Tree.init.heightToGrowTo");
        // maxHeight >= 3 for every species (a zero divisor would be SIGFPE in the original)
        htg = if max_height == 0 { 0 } else { r.wrapping_rem(max_height) };
        if htg < 1 {
            htg = 1;
        }
    }
    {
        let mut s = t.tree().borrow_mut();
        s.max_height = max_height;
        s.height = 0;
    }
    (htg, ama)
}

/// `(float)[sonf getX:(double)xf Y:(double)yf octaves:6]` (0.0 when nil): the constructor season-offset noise.
pub fn season_noise_fraction(me: &ObjRef, xf: f32, yf: f32) -> f32 {
    let sonf = tl(me).tree().borrow().season_offset_noise_function.clone();
    noise(&sonf, xf as f64, yf as f64, 6) as f32
}

/// `fabsf(fmodf(((float)px / 32.0f) / (float)ww, 0.5f) - 0.25f) / 0.25f` (Mango, Orange, Lime, Coffee).
pub fn distance_from_equator_fraction(px: i32, ww: i32) -> f32 {
    let fm = crate::fmath::libm::fmodf((px as f32 / 32.0f32) / ww as f32, 0.5f32);
    (fm - 0.25f32).abs() / 0.25f32
}

/// Constructor pre-growth loop (shared step 5): grows `height` while `heightIndex < heightToGrowTo` and
/// `age < adultMaxAge`, calling the virtual `updateGrowth:YES` each step.
pub fn pre_grow(me: &ObjRef, height_to_grow_to: i32, adult_max_age: f32) {
    let t = tl(me);
    let mut hi = 0;
    while hi < height_to_grow_to && t.tree().borrow().age < adult_max_age {
        {
            let mut s = t.tree().borrow_mut();
            let gr =
                ((((1.0f32 - s.height as f32 / s.max_height as f32) + 0.2f32) * s.growth_rate) as f64 * 0.5f64) as f32;
            s.age = (s.age as f64 + 1.0f64 / (0.005f64 * gr as f64)) as f32;
        }
        t.increment_height();
        {
            let mut s = t.tree().borrow_mut();
            if s.max_height_reached < s.height {
                s.max_height_reached = s.height;
            }
        }
        t.update_growth(me, true);
        hi += 1;
    }
}

/// Constructor lifetime loop (shared step 6): `for testHeight in 0... maxHeight` (inclusive).
pub fn compute_max_age(me: &ObjRef) {
    let t = tl(me);
    let mut s = t.tree().borrow_mut();
    let mh = s.max_height;
    let mut th = 0;
    while th <= mh {
        let g = ((((1.0f32 - th as f32 / mh as f32) + 0.2f32) * s.growth_rate) as f64 * 0.5f64) as f32;
        s.max_age = (s.max_age as f64 + 1.0f64 / (0.005f64 * g as f64)) as f32;
        th += 1;
    }
}

// ------------------------------------------------------------------------------------------------ shared updateGrowth

/// How `tile.seasonOffset` is derived from the float jitter value.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SeasonByte {
    /// `(uint8)(((int)v + 255) % 255)` (Apple, Cherry, Coffee, Mango, Maple, Orange, Lime).
    Mod255,
    /// `v < 0 ? 0: (v > 255 ? 255: v)` (Pine, Cactus, GemTree).
    Clamp,
}

/// Fruit-slot noise threshold.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FruitThreshold {
    /// `(belowCompost ? -0.2: 0.2) + (expert ? 0.2: 0.0)` (f64).
    CompostExpert,
    /// Constant `0.4` (f64) (Maple, Pine).
    Const04,
}

/// Per-species differences of the foliage `updateGrowth:` shape (Apple, Cherry, Coffee, Mango, Maple, Orange,
/// Lime, Pine).
///  Pine.
pub struct BushyParams {
    pub trunk: u8,
    pub bush: u8,
    pub trunk_bush: u8,
    /// Season-base multiplier `K` (`((y - h/2) * K) / h + 127 + treeSeasonOffset`).
    pub k: i32,
    /// Jitter amplitude `A`.
    pub amp: f32,
    pub byte: SeasonByte,
    /// Pine widths: `((h+2) - y) * (0.4 + f * 0.2)` instead of `((h+4) - y) * (0.2 + f * 0.5)`.
    pub pine_widths: bool,
    /// Maple/Coffee width clamp `min(y, h/2) + f * 2.0`.
    pub clamp_widths: bool,
    pub fruit: FruitThreshold,
    /// AppleTree: `dn <= thr` rescue logic and the 3-high guarantee instead of `thr < dn`.
    pub apple_rescue: bool,
    /// `dynamicWorldChangedAtPos` after adding a fruit slot (all but Orange and Lime).
    pub dw_on_fruit: bool,
    /// Orange and Lime: cold death check at the end of `updateGrowth:`.
    pub cold_death: bool,
}

fn season_byte(mode: SeasonByte, base: i32, amp: f32, tag: &'static str) -> u8 {
    let r = r01t(tag);
    let v = f32_to_i32(base as f32 + amp * (r - 0.5f32) * 2.0f32);
    match mode {
        SeasonByte::Mod255 => (v.wrapping_add(255).wrapping_rem(255)) as u8,
        SeasonByte::Clamp => {
            if v < 0 {
                0
            } else if v > 255 {
                255
            } else {
                v as u8
            }
        }
    }
}

/// `(wL, wR)` of the foliage rows (the same noise calls in the upper and the lower part).
fn widths(me: &ObjRef, p: &BushyParams, w: &World, y: i32, h: i32, upper: bool) -> (i32, i32) {
    let tdnf = tl(me).tree().borrow().tree_density_noise_function.clone();
    let pos = me.pos();
    let ww = w.world_width_macro();
    let ww32 = ww.wrapping_shl(5) as f32;
    // Answered: Pine's upper part has NO `* 8.0f`; Pine lower part and all other species multiply by 8.
    let k = if p.pine_widths && upper { 1.0f32 } else { 8.0f32 };
    let nlx = (pos.x as f32 / ww32) * k;
    let nly = (((pos.y as f32) + (y as f32)) / 1024.0f32) * k;
    let frac_l = 0.5f32 * (noise(&tdnf, nlx as f64, nly as f64, 6) as f32) + 0.5f32;
    let nrx = (((pos.x as f32) + 67.0f32) / ww32) * k;
    let nry = ((((pos.y as f32) + (y as f32)) + 5438.0f32) / 1024.0f32) * k;
    let frac_r = 0.5f32 * (noise(&tdnf, nrx as f64, nry as f64, 6) as f32) + 0.5f32;
    let span = ((if p.pine_widths { h + 2 } else { h + 4 }) - y) as f64;
    let (a, b) = if p.pine_widths { (0.4f64, 0.2f64) } else { (0.2f64, 0.5f64) };
    let mut wl = f64_to_i32(span * (a + (frac_l as f64) * b));
    let mut wr = f64_to_i32(span * (a + (frac_r as f64) * b));
    if p.clamp_widths {
        let c = y.min(h / 2);
        let tl_ = (c as f64) + (frac_l as f64) * 2.0f64;
        wl = if tl_ <= wl as f64 { f64_to_i32(tl_) } else { wl };
        let tr_ = (c as f64) + (frac_r as f64) * 2.0f64;
        wr = if tr_ <= wr as f64 { f64_to_i32(tr_) } else { wr };
    }
    (wl, wr)
}

/// The shared foliage `updateGrowth:`: trunk column with side bushes in the upper part, bare trunk and cleared
/// side leaves in the lower eighth, fruit-slot rolls, and (Orange, Lime) the cold-death check.
/// `rand()` order per row: trunk jitter, then one jitter per claimed side tile (bushX ascending, left first).
/// The `reloadDrawBlockGeometryForTile` calls and the `minX/maxX` bookkeeping are client-only no-ops and omitted.
pub fn bushy_update_growth(me: &ObjRef, add_new_branch_blocks: bool, p: &BushyParams) {
    let t = tl(me);
    if t.tree().borrow().dead {
        return;
    }
    let w = tree_world(me);
    let dw = tree_dynamic_world(me);
    let pos = me.pos();
    let uid = me.unique_id();
    let ww = w.world_width_macro();
    let h = t.tree().borrow().height;
    let tso = t.tree().borrow().tree_season_offset;
    let obj_type = me.object_type();
    let mut y = 0;
    while y < h {
        let season_base = (y.wrapping_sub(h / 2).wrapping_mul(p.k) / h).wrapping_add(127).wrapping_add(tso);
        let t0 = tile_at(&w, pos.x, pos.y.wrapping_add(y));
        if let Some(tr) = t0 {
            let tt = tr.get();
            if tt.on_fire != 0 || tile_is_solid(t0) || (tt.contents != 0 && !tile_is_tree(t0)) {
                break;
            }
        }
        if (h / 8 < y) || h < 3 {
            // ---- upper part: trunk + bushes spreading sideways
            let mut rescue_x = pos.x;
            if let Some(tr) = t0 {
                let cur = tr.get().contents;
                let nc = if y == h - 1 && cur != p.trunk_bush { p.bush } else { p.trunk_bush };
                let sb = season_byte(p.byte, season_base, p.amp, "Tree.updateGrowth.trunk");
                tr.update(|tile| {
                    tile.dynamic_object_owner = uid;
                    tile.contents = nc;
                    tile.season_offset = sb;
                });
                dw.world_contents_changed_at_pos(IntPair { x: pos.x, y: pos.y + y });
            }
            let (wl, wr) = widths(me, p, &w, y, h, true);
            let mut can_l = true;
            let mut can_r = true;
            let mut bush_x = 1;
            while bush_x < wl || bush_x < wr {
                for s in [-1i32, 1] {
                    if !((s == -1 && can_l && bush_x < wl) || (s == 1 && can_r && bush_x < wr)) {
                        continue;
                    }
                    let bp = IntPair { x: pos.x.wrapping_add(bush_x * s), y: pos.y.wrapping_add(y) };
                    let bt = tile_at(&w, bp.x, bp.y);
                    let Some(btr) = bt else { continue };
                    let mut blocked = false;
                    let mut claim = false;
                    let bc = btr.get();
                    if add_new_branch_blocks && bc.dynamic_object_owner != uid {
                        if tile_is_solid(bt) || bc.contents != 0 {
                            claim = tile_is_dead_tree(bt);
                        } else {
                            claim = true;
                        }
                        if !claim && tile_is_bush(bt) {
                            if bc.dynamic_object_owner == 0 {
                                claim = true;
                            } else if bush_x < 2 {
                                claim = true;
                            }
                        }
                    }
                    if claim {
                        btr.update(|tile| {
                            tile.dynamic_object_owner = uid;
                            tile.contents = p.bush;
                        });
                        let fruit_count = t.tree().borrow().fruit_count;
                        if fruit_count < 64 {
                            let comp =
                                matches!(tile_at(&w, pos.x, pos.y - 1), Some(b) if is_compost_type(b.get().type_index));
                            let nx = ((((pos.x as f32) + 5000.0f32 + ((bush_x * s) as f32)) / 32.0f32) / (ww as f32))
                                * 99.0f32;
                            let ny = ((((pos.y as f32) + 5000.0f32 + (y as f32)) / 32.0f32) / (ww as f32)) * 99.0f32;
                            let sonf = t.tree().borrow().season_offset_noise_function.clone();
                            let n = noise(&sonf, nx as f64, ny as f64, 7) as f32;
                            let thr: f64 = match p.fruit {
                                FruitThreshold::CompostExpert => {
                                    (if comp { -0.2f64 } else { 0.2f64 })
                                        + (if w.expert_mode() { 0.2f64 } else { 0.0f64 })
                                }
                                FruitThreshold::Const04 => 0.4f64,
                            };
                            if p.apple_rescue {
                                if (n as f64) <= thr {
                                    if h == 3 && fruit_count == 0 && rn("AppleTree.updateGrowth.rescue") > 0x3fffffff {
                                        rescue_x = bp.x;
                                    }
                                } else {
                                    add_fruit(me, bp, true);
                                }
                            } else if thr < n as f64 {
                                add_fruit(me, bp, p.dw_on_fruit);
                            }
                        }
                        let sb = season_byte(p.byte, season_base, p.amp, "Tree.updateGrowth.bush");
                        btr.update(|tile| tile.season_offset = sb);
                        dw.world_contents_changed_at_pos(bp);
                        blocked = true;
                        if s == -1 {
                            can_l = false;
                        } else {
                            can_r = false;
                        }
                    } else if !t.tile_is_kind_of_self(&btr.get()) || tile_is_tree_trunk(bt) {
                        blocked = true;
                    }
                    let after = btr.get();
                    if after.on_fire != 0 {
                        blocked = true;
                    }
                    if blocked && after.dynamic_object_owner != uid {
                        if s == -1 {
                            can_l = false;
                        } else {
                            can_r = false;
                        }
                    }
                }
                bush_x += 1;
            }
            if p.apple_rescue && h == 3 && t.tree().borrow().fruit_count == 0 {
                {
                    let mut st = t.tree().borrow_mut();
                    let n = st.fruit_count as usize;
                    if n < 64 {
                        st.tree_fruits[n] = TreeFruit {
                            pos: IntPair { x: rescue_x, y: pos.y + y },
                            has_created_free_block_this_season: false,
                        };
                    }
                    st.fruit_count += 1;
                }
                dw.dynamic_world_changed_at_pos_object_type(pos, obj_type);
            }
        } else {
            // ---- lower trunk: bare trunk, clear side leaves
            if let Some(tr) = t0 {
                let sb = season_byte(p.byte, season_base, p.amp, "Tree.updateGrowth.trunk");
                tr.update(|tile| {
                    tile.dynamic_object_owner = uid;
                    tile.contents = p.trunk;
                    tile.season_offset = sb;
                });
                dw.world_contents_changed_at_pos(IntPair { x: pos.x, y: pos.y + y });
            }
            let (wl, wr) = widths(me, p, &w, y, h, false);
            for bx in 1..wl {
                let x = pos.x.wrapping_sub(bx);
                if let Some(btr) = tile_at(&w, x, pos.y + y) {
                    let bc = btr.get();
                    if (bc.dynamic_object_owner == uid || bc.dynamic_object_owner == 0) && t.tile_is_kind_of_self(&bc) {
                        remove_tile(&w, x, pos.y + y, 0);
                    }
                }
            }
            for bx in 1..wr {
                let x = pos.x.wrapping_add(bx);
                if let Some(btr) = tile_at(&w, x, pos.y + y) {
                    let bc = btr.get();
                    if (bc.dynamic_object_owner == uid || bc.dynamic_object_owner == 0) && t.tile_is_kind_of_self(&bc) {
                        remove_tile(&w, x, pos.y + y, 0);
                    }
                }
            }
        }
        y += 1;
    }
    if p.cold_death {
        cold_death_check(me);
    }
}

/// Appends a fruit slot `{pos, hasCreated = NO}` (capacity 64 guaranteed by the caller) and optionally marks the
/// object dirty.
fn add_fruit(me: &ObjRef, at: IntPair, dw_changed_after: bool) {
    {
        let mut st = tl(me).tree().borrow_mut();
        let n = st.fruit_count as usize;
        if n < 64 {
            st.tree_fruits[n] = TreeFruit { pos: at, has_created_free_block_this_season: false };
        }
        st.fruit_count += 1;
    }
    if dw_changed_after {
        dw_changed(me);
    }
}

/// OrangeTree / LimeTree cold death at the end of `updateGrowth:`: `temp < -10.0f` kills the tree.
/// Evaluation order: weather, day/night, season, temperature.
pub fn cold_death_check(me: &ObjRef) {
    let t = tl(me);
    let w = tree_world(me);
    let pos = me.pos();
    let tile = tile_at(&w, pos.x, pos.y);
    let weather = w.get_weather_fraction_for_pos_at_world_time(pos, w.world_time());
    let day_night = w.get_day_night_fraction_for_x_at_world_time(pos.x as f32, w.world_time());
    let season = season_for_world_x(pos.x, w.world_time(), Some(&w));
    // Answered: the original dereferences a NULL tile (crash under faithful-crashes).
    if tile.is_none() {
        crate::objects::base::ub_null_tile_deref();
    }
    let tv = tile.map_or_else(Tile::default, |tr| tr.get());
    let temp = current_temperature_for_tile_at_world_pos(&tv, pos, season, day_night, weather, Some(&w));
    if temp < -10.0f32 {
        {
            let mut s = t.tree().borrow_mut();
            s.dead = true;
            s.time_died = w.world_time();
        }
        tree_kill_all_owned_tiles(me);
        dw_changed(me);
    }
}

// ------------------------------------------------------------------------------------------------ shared update (fruit)

/// Per-species differences of the seasonal fruit-drop loop in `update:`.
///  PineTree; GemTree.
pub struct FruitCfg {
    /// Item type of the free block.
    pub item: u32,
    /// `tileAtWorldPosition` (macro tiles, no load) instead of `tileAtWorldPositionLoaded`.
    pub lookup_any: bool,
    /// Season index `((int)(...)) % 8` of a fruit tile (C remainder, may be negative).
    pub idx: fn(season_for_x: f32, tile: &Tile, tree_season_offset: i32) -> i32,
    /// Window test on the index.
    pub window: fn(i32) -> bool,
    /// `height > min_height` is part of the creation condition (`i32::MIN`: no gate).
    pub min_height: i32,
    /// Number of free blocks per creation (`n`); evaluated at creation time.
    pub count: fn(me: &ObjRef, height: i32) -> i32,
    /// `dataA/dataB = maxHeightGeneVariation/growthRateGeneVariation` (false: `0/0`, GemTree).
    pub gene_data: bool,
}

/// `n = 1`.
pub fn count_one(_me: &ObjRef, _height: i32) -> i32 {
    1
}

/// The seasonal fruit loop shared by every species: stale slots are removed by shifting down with `i` NOT adjusted
/// (the shifted-in slot is skipped this tick), live slots create their free block(s) inside the season window and
/// reset the flag outside it. A slot whose tile is NULL is ignored.
pub fn fruit_season_update(me: &ObjRef, cfg: &FruitCfg) {
    let t = tl(me);
    let w = tree_world(me);
    let dw = tree_dynamic_world(me);
    let pos = me.pos();
    let uid = me.unique_id();
    let season_for_x = season_for_world_x(pos.x, w.world_time(), Some(&w));
    let mut i: i32 = 0;
    loop {
        let count = t.tree().borrow().fruit_count;
        if i >= count {
            break;
        }
        let fruit = t.tree().borrow().tree_fruits[i as usize];
        let tr = if cfg.lookup_any {
            tile_at_any(&w, fruit.pos.x, fruit.pos.y)
        } else {
            tile_at(&w, fruit.pos.x, fruit.pos.y)
        };
        if let Some(tr) = tr {
            let tile = tr.get();
            if !t.tile_is_kind_of_self(&tile) || tile.dynamic_object_owner != uid {
                {
                    let mut s = t.tree().borrow_mut();
                    let n = s.fruit_count;
                    let mut j = i;
                    while j < n - 1 {
                        s.tree_fruits[j as usize] = s.tree_fruits[(j + 1) as usize];
                        j += 1;
                    }
                    s.fruit_count -= 1;
                }
                dw_changed(me);
            } else {
                let (tso, height) = {
                    let s = t.tree().borrow();
                    (s.tree_season_offset, s.height)
                };
                let idx = (cfg.idx)(season_for_x, &tile, tso);
                if (cfg.window)(idx) && height > cfg.min_height {
                    if !fruit.has_created_free_block_this_season {
                        t.tree().borrow_mut().tree_fruits[i as usize].has_created_free_block_this_season = true;
                        let n = (cfg.count)(me, height);
                        let mut k = 0;
                        while k < n {
                            let (a, b) = if cfg.gene_data {
                                let a = (t as &dyn TreeLike).max_height_gene_variation();
                                let b = (t as &dyn TreeLike).growth_rate_gene_variation();
                                (a, b)
                            } else {
                                (0, 0)
                            };
                            dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                                fruit.pos,
                                ItemType(cfg.item),
                                a,
                                b,
                                None,
                                None,
                                true,
                                0,
                                None,
                            );
                            k += 1;
                        }
                        dw_changed(me);
                    }
                } else if fruit.has_created_free_block_this_season {
                    t.tree().borrow_mut().tree_fruits[i as usize].has_created_free_block_this_season = false;
                    dw_changed(me);
                }
            }
        }
        i += 1;
    }
}

/// `((int)(8.0f * s + 4.0f)) % 8` (C truncation and remainder; negative remainders match no window).
pub fn idx_basic(season: f32) -> i32 {
    f32_to_i32(8.0f32 * season + 4.0f32).wrapping_rem(8)
}

/// Integration-test fixtures: a real `World` + `DynamicWorld` with loaded dirt/air blocks.
#[cfg(test)]
pub(crate) mod testenv {
    use super::*;
    use crate::dynworld::state::testutil::dw;
    use crate::world::state::PhysicalBlock;

    /// Ground top row (inclusive): rows `0..=GROUND` are dirt, above is air.
    pub const GROUND: i32 = 9;

    /// A 2-macro-wide world (64 tiles) whose every block is loaded: dirt (type 6) up to `GROUND`, air (type 2,
    /// back wall 2, full sun light) above.
    pub fn env(name: &str) -> (Rc<World>, Rc<DynamicWorld>) {
        let d = dw(name, 2);
        let w = d.world();
        let mt = w.macro_tiles();
        for i in 0..mt.len() {
            let m = mt.get(i).unwrap();
            let pb: &'static PhysicalBlock = Box::leak(Box::new(PhysicalBlock::new_fresh()));
            let (mx, my) = ((i % 2) as i32, (i / 2) as i32);
            pb.x_pos.set(mx);
            pb.y_pos.set(my);
            pb.valid.set(true);
            for ly in 0..32usize {
                for lx in 0..32usize {
                    let wy = my * 32 + ly as i32;
                    let mut t = Tile::default();
                    if wy <= GROUND {
                        t.type_index = 6;
                    } else {
                        t.type_index = 2;
                        t.back_wall_type_index = 2;
                        t.sun_light = 255;
                    }
                    pb.tiles[ly * 32 + lx].set(t);
                }
            }
            *m.physical_block.borrow_mut() = Some(pb);
        }
        (w, d)
    }

    /// `n` rand draws after `srand(seed)`.
    pub fn draws(seed: u32, n: usize) -> Vec<i32> {
        crate::rng::glibc_rand::srand(seed);
        (0..n).map(|_| crate::rng::glibc_rand::rand()).collect()
    }

    pub fn tile(w: &World, x: i32, y: i32) -> Tile {
        tile_at(w, x, y).unwrap().get()
    }

    pub fn ip(x: i32, y: i32) -> IntPair {
        IntPair { x, y }
    }

    /// Count tiles with `contents` in `set` over the whole column span `x0..x1`, rows `y0..y1`.
    pub fn count_contents(w: &World, x0: i32, x1: i32, y0: i32, y1: i32, set: &[u8]) -> usize {
        let mut n = 0;
        for y in y0..y1 {
            for x in x0..x1 {
                if set.contains(&tile(w, x, y).contents) {
                    n += 1;
                }
            }
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::testenv::*;
    use super::*;

    /// : {6, 27, 28, 48, 49, 50}.
    #[test]
    fn soil_set() {
        for t in 0..=255 {
            assert_eq!(tree_is_required_soil_type(t), matches!(t, 6 | 27 | 28 | 48 | 49 | 50));
        }
    }

    ///  / T15: `(int)` of a negative float truncates toward zero and `%` keeps the dividend's sign.
    #[test]
    fn season_index_negative_remainder() {
        assert_eq!(idx_basic(-1.0), (-4) % 8);
        assert!(idx_basic(-1.0) < 0);
        assert_eq!(idx_basic(0.0), 4);
        assert_eq!(idx_basic(0.5), 0);
    }

    /// : `clampi((int)(gene + (R01 - 0.5f) * 8.0f), 1, 255)`.
    #[test]
    fn gene_variation_bounds() {
        crate::rng::glibc_rand::srand(1);
        for _ in 0..1000 {
            let v = gene_variation(1);
            assert!((1..=255).contains(&(v as i32)));
            let v = gene_variation(255);
            assert!((1..=255).contains(&(v as i32)));
        }
    }

    ///  R1 / R6: update-light constant `m = 0.2` and the integer artificial term.
    #[test]
    fn update_light_formula() {
        let mut t = Tile::default();
        t.sun_light = 255;
        let l = update_light(&t, 0.2);
        assert!((l - 0.4).abs() < 1e-6);
        t.artificial_light_r = 1020; // (1020/4)/255 = 1
        assert!((update_light(&t, 0.2) - 1.4).abs() < 1e-6);
    }

    #[test]
    fn clamp() {
        assert_eq!(clampi(-5, 1, 255), 1);
        assert_eq!(clampi(300, 1, 255), 255);
        assert_eq!(clampi(7, 1, 255), 7);
    }

    ///  jitter byte: `(int)(base + A*(R-0.5)*2)` then `% 255` truncated to u8 (negative remainders wrap).
    #[test]
    fn season_byte_modes() {
        crate::rng::glibc_rand::srand(1);
        let b = season_byte(SeasonByte::Clamp, -1000, 8.0, "t");
        assert_eq!(b, 0);
        let b = season_byte(SeasonByte::Clamp, 1000, 8.0, "t");
        assert_eq!(b, 255);
        // Mod255: (v + 255) % 255 with v = -300 -> (-45) % 255 = -45 -> as u8 = 211
        let v: i32 = -300;
        assert_eq!(((v + 255) % 255) as u8, 211);
    }

    // ------------------------------------------------------------------------------------------ integration tests

    use crate::dynworld::state::{DwHooks, FreeBlockInit};
    use crate::trees::apple::AppleTree;
    use crate::trees::cactus::CactusTree;
    use crate::trees::cherry::CherryTree;
    use crate::trees::coconut::CoconutTree;
    use crate::trees::coffee::CoffeeTree;
    use crate::trees::gem::GemTree;
    use crate::trees::lime::LimeTree;
    use crate::trees::mango::MangoTree;
    use crate::trees::maple::MapleTree;
    use crate::trees::orange::OrangeTree;
    use std::cell::RefCell as RC;

    type NewFn =
        fn(&Rc<World>, &Rc<DynamicWorld>, IntPair, CacheRef, i16, i16, NoiseRef, NoiseRef, bool, f32) -> Option<ObjRef>;

    /// Records the `createFreeBlockAtPosition:...` calls of a test.
    struct Rec {
        log: Rc<RC<Vec<(IntPair, u32, u16, u16, bool)>>>,
    }
    impl crate::dynworld::state::DwHooksStub for Rec {
        fn freeblock_from_position(&self, init: FreeBlockInit<'_>) -> Option<ObjRef> {
            self.log.borrow_mut().push((init.position, init.item_type, init.data_a, init.data_b, init.hovers));
            None
        }
    }

    fn record(d: &Rc<DynamicWorld>) -> Rc<RC<Vec<(IntPair, u32, u16, u16, bool)>>> {
        let log = Rc::new(RC::new(vec![]));
        d.set_hooks(Rc::new(Rec { log: log.clone() }));
        log
    }

    fn species() -> Vec<(i32, i32, NewFn)> {
        vec![
            (TREE_APPLE, OBJECT_TYPE_APPLE, AppleTree::new as NewFn),
            (TREE_MANGO, OBJECT_TYPE_MANGO, MangoTree::new as NewFn),
            (TREE_MAPLE, OBJECT_TYPE_MAPLE, MapleTree::new as NewFn),
            (TREE_PINE, OBJECT_TYPE_PINE, PineTree::new as NewFn),
            (TREE_CACTUS, OBJECT_TYPE_CACTUS, CactusTree::new as NewFn),
            (TREE_COCONUT, OBJECT_TYPE_COCONUT, CoconutTree::new as NewFn),
            (TREE_ORANGE, OBJECT_TYPE_ORANGE, OrangeTree::new as NewFn),
            (TREE_CHERRY, OBJECT_TYPE_CHERRY, CherryTree::new as NewFn),
            (TREE_COFFEE, OBJECT_TYPE_COFFEE, CoffeeTree::new as NewFn),
            (TREE_LIME, OBJECT_TYPE_LIME, LimeTree::new as NewFn),
        ]
    }

    ///  species table: object types, tree types, fruit items, class chain, `occupiesNormalContents`.
    #[test]
    fn species_identity_table() {
        let (w, d) = env("tr_ident");
        let expect_fruit = [21, 22, 23, 27, 24, 46, 60, 77, 78, 160];
        for (n, (tt, ot, f)) in species().into_iter().enumerate() {
            let me = f(&w, &d, ip(4 + 5 * n as i32, GROUND + 1), None, 100, 100, None, None, false, 0.0).expect("tree");
            let t = as_tree(&me).unwrap();
            assert_eq!(t.tree_type(), tt);
            assert_eq!(me.object_type(), ot);
            assert_eq!(t.fruit_item_type(), expect_fruit[n]);
            assert!(me.occupies_normal_contents());
            assert!(me.class_info().is_kind_of(&TREE_CLASS));
            assert!(!t.is_static_tree());
        }
        let g = GemTree::new(&w, &d, ip(60, GROUND + 1), None, None, None, 13).unwrap();
        let t = as_tree(&g).unwrap();
        assert_eq!((t.tree_type(), g.object_type(), t.fruit_item_type()), (13, OBJECT_TYPE_GEM, 76));
        assert!(t.is_static_tree());
        assert!(!t.should_add_fallen_fruits());
        assert!(!as_tree(
            &species().into_iter().find(|s| s.0 == TREE_PINE).unwrap().2(
                &w,
                &d,
                ip(30, GROUND + 1),
                None,
                1,
                1,
                None,
                None,
                false,
                0.0
            )
            .unwrap()
        )
        .unwrap()
        .should_add_fallen_fruits());
    }

    ///  T13 /: soil sets per species.
    #[test]
    fn soil_overrides() {
        let (w, d) = env("tr_soil");
        let mk = |tt: i32, x: i32| {
            new_tree_for_type(tt, &w, &d, ip(x, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap()
        };
        let cactus = mk(TREE_CACTUS, 4);
        let coconut = mk(TREE_COCONUT, 12);
        let apple = mk(TREE_APPLE, 20);
        for t in 0..=255 {
            assert_eq!(as_tree(&cactus).unwrap().is_required_soil_type(t), matches!(t, 7 | 8 | 58 | 48 | 49 | 50));
            assert_eq!(as_tree(&coconut).unwrap().is_required_soil_type(t), matches!(t, 6 | 27 | 8 | 58 | 48 | 49));
            assert_eq!(as_tree(&apple).unwrap().is_required_soil_type(t), matches!(t, 6 | 27 | 28 | 48 | 49 | 50));
        }
    }

    /// : the dispatcher returns None for unknown types and builds gems for 11..=15.
    #[test]
    fn dispatcher() {
        let (w, d) = env("tr_disp");
        assert!(new_tree_for_type(0, &w, &d, ip(4, GROUND + 1), None, 1, 1, None, None, false, 0.0).is_none());
        assert!(new_tree_for_type(16, &w, &d, ip(4, GROUND + 1), None, 1, 1, None, None, false, 0.0).is_none());
        for (i, tt) in (11..=15).enumerate() {
            let g = new_tree_for_type(tt, &w, &d, ip(4 + 11 * i as i32, GROUND + 1), None, 1, 1, None, None, true, 5.0)
                .unwrap();
            assert_eq!(g.object_type(), OBJECT_TYPE_GEM);
            assert_eq!(as_tree(&g).unwrap().tree_type(), tt);
        }
        assert!(new_tree_from_save_dict(10, &w, &d, &new_mutable_dict(), None, None, None).is_none());
    }

    ///  AppleTree init + `updateGrowth:`: a non-adult tree is pre-grown to height 1 (bush top, two side bushes)
    /// and consumes exactly 4 `rand()`: trunk jitter, two side jitters, `availableFood`.
    #[test]
    fn apple_nonadult_init_rng_and_tiles() {
        let (w, d) = env("tr_apple_init");
        crate::rng::glibc_rand::srand(5);
        let me = AppleTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        assert_eq!(crate::rng::glibc_rand::rand(), draws(5, 5)[4]);
        let uid = me.unique_id();
        for x in [9, 10, 11] {
            let t = tile(&w, x, GROUND + 1);
            assert_eq!(t.contents, 3, "x={x}");
            assert_eq!(t.dynamic_object_owner, uid);
        }
        assert_eq!(tile(&w, 8, GROUND + 1).contents, 0);
        let s = tree_state(&me).borrow();
        assert_eq!((s.height, s.max_height_reached), (1, 1));
        assert_eq!(s.fruit_count, 0);
        assert!(s.max_height >= 3 && s.max_age > 0.0 && s.age > 0.0);
        assert!(s.max_age.is_finite());
    }

    ///  CoconutTree: a height-1 palm is a single crown tile and consumes no `rand()` in `init`.
    #[test]
    fn coconut_nonadult_no_rand() {
        let (w, d) = env("tr_coco_init");
        crate::rng::glibc_rand::srand(9);
        let me = CoconutTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        assert_eq!(crate::rng::glibc_rand::rand(), draws(9, 1)[0]);
        assert_eq!(tile(&w, 10, GROUND + 1).contents, 15);
        assert_eq!(tile(&w, 10, GROUND + 1).dynamic_object_owner, me.unique_id());
        assert_eq!(tile(&w, 9, GROUND + 1).contents, 0);
        assert_eq!(tree_state(&me).borrow().fruit_count, 0);
    }

    ///  CactusTree init RNG order: maxHeight, splitA, splitB, splitDirection, trunk jitter (updateGrowth),
    /// availableFood = 6 draws for a non-adult tree. Splits are ordered and within range.
    #[test]
    fn cactus_nonadult_init_rng() {
        let (w, d) = env("tr_cactus_init");
        crate::rng::glibc_rand::srand(3);
        let me = CactusTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        assert_eq!(crate::rng::glibc_rand::rand(), draws(3, 7)[6]);
        let dict = as_tree(&me).unwrap();
        let sd = me.get_save_dict(&me);
        let a = sd.map.get(&"splitHeightA".to_string()).unwrap().int_value();
        let b = sd.map.get(&"splitHeightB".to_string()).unwrap().int_value();
        let mh = tree_state(&me).borrow().max_height;
        assert!(a <= b);
        assert!(a == 99 || (2..=mh - 1).contains(&a));
        assert!(b == 99 || (2..=mh - 1).contains(&b));
        assert!(mh >= 4);
        assert_eq!(tile(&w, 10, GROUND + 1).contents, 43);
        assert!(sd.map.get(&"availableFood".to_string()).unwrap().float_value() < 450.0);
        assert_eq!(dict.tree_type(), 5);
    }

    /// `rand() % maxHeight` is drawn only for adults (T4); the adult draw order is growthCounter first.
    #[test]
    fn adult_draws_growth_counter_first() {
        let (w, d) = env("tr_adult");
        crate::rng::glibc_rand::srand(11);
        let me = CherryTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, true, 123.0).unwrap();
        let r = draws(11, 1)[0];
        assert_eq!(tree_state(&me).borrow().growth_counter, r01_from_test(r));
        // an adult has pre-grown at least one step
        assert!(tree_state(&me).borrow().height >= 1);
        assert!(tree_state(&me).borrow().age <= 123.0 + 1e9);
    }

    fn r01_from_test(r: i32) -> f32 {
        crate::rng::glibc_rand::r01_from(r)
    }

    /// `Tree.update`: growthCounter advances by the documented formula with `m = 0.2` (T1/T5).
    #[test]
    fn update_growth_counter_formula() {
        let (w, d) = env("tr_upd");
        let me = CherryTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        let (gc0, h, mh, gr) = {
            let s = tree_state(&me).borrow();
            (s.growth_counter, s.height, s.max_height, s.growth_rate)
        };
        assert_eq!(gc0, 0.0);
        tree_update(&me, 0.2, 0.2, false);
        let light = ((255.0f64 * 0.2 * 2.0) / 255.0) as f32;
        let inner = ((1.0f32 - h as f32 / mh as f32) + 0.2f32) * gr;
        let expect = (0.0f64 + 0.2f32 as f64 * 0.005 * inner as f64 * light as f64) as f32;
        assert_eq!(tree_state(&me).borrow().growth_counter, expect);
        assert!((light - 0.4).abs() < 1e-6);
        // ageCounter accumulates and is flushed at 1000*dt
        assert_eq!(tree_state(&me).borrow().age_counter, 0.2);
        let age0 = tree_state(&me).borrow().age;
        for _ in 0..1000 {
            tree_update(&me, 0.2, 0.2, false);
        }
        let s = tree_state(&me).borrow();
        assert!(s.age > age0);
    }

    /// `Tree.update` growth step: at `growthCounter >= 1` the height increments, the counter resets and the
    /// virtual `updateGrowth:YES` writes the new trunk tile (T5).
    #[test]
    fn update_growth_step() {
        let (w, d) = env("tr_step");
        let me = CoconutTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        let (mh, h0) = {
            let s = tree_state(&me).borrow();
            (s.max_height, s.height)
        };
        assert!(mh > h0);
        tree_state(&me).borrow_mut().growth_counter = 1.0;
        tree_update(&me, 0.2, 0.2, false);
        let s = tree_state(&me).borrow();
        assert_eq!(s.height, h0 + 1);
        assert!(s.growth_counter < 0.01);
        assert_eq!(s.max_height_reached, 2);
        drop(s);
        // height 2: topSize 1 => bare trunk row 0 (16) + crown row 1 (15)
        assert_eq!(tile(&w, 10, GROUND + 1).contents, 16);
        assert_eq!(tile(&w, 10, GROUND + 2).contents, 15);
    }

    /// Dies without light for more than 20 s, and at `pos.y > 991` (T5).
    #[test]
    fn update_no_light_and_too_high_death() {
        let (w, d) = env("tr_nolight");
        let me = CherryTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        // black out the light on the tree's own column
        for y in GROUND + 1..GROUND + 5 {
            tile_at(&w, 10, y).unwrap().update(|t| t.sun_light = 0);
        }
        for _ in 0..99 {
            tree_update(&me, 0.2, 0.2, false);
        }
        assert!(!tree_state(&me).borrow().dead, "19.8 s is not > 20 s");
        // f32 accumulation of 0.2 passes 20.0f at tick 100 or 101
        tree_update(&me, 0.2, 0.2, false);
        tree_update(&me, 0.2, 0.2, false);
        assert!(tree_state(&me).borrow().dead);
        assert_eq!(tree_state(&me).borrow().time_died, w.world_time());
        // dead tiles flip immediately
        assert_eq!(tile(&w, 10, GROUND + 1).contents, 39);
    }

    /// worldChanged soil check (T6): removing the soil kills the tree, tiles turn to their `*_DEAD` contents and
    /// the removal wave only starts after the 1 s checks.
    #[test]
    fn soil_removal_kills_tree() {
        let (w, d) = env("tr_soilkill");
        let me = AppleTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        w.set_world_time(1000.0);
        tile_at(&w, 10, GROUND).unwrap().update(|t| t.type_index = 2);
        tree_world_changed(&me, &mut vec![ip(10, GROUND)]);
        let s = tree_state(&me).borrow();
        assert!(s.dead);
        assert_eq!(s.time_died, 1000.0);
        drop(s);
        // the tiles of the 1-high apple: bush -> BUSH_DEAD (0x1b)
        assert_eq!(tile(&w, 10, GROUND + 1).contents, 0x1b);
        assert_eq!(tile(&w, 9, GROUND + 1).contents, 0x1b);
    }

    /// Dead trees with `now - timeDied > 60 s` set `needsRemoved` and call `removeAllOwnedTiles(YES)`; in expert
    /// mode that consumes no `rand()` (T6).
    #[test]
    fn dead_tree_forced_removal_expert_no_rand() {
        let (w, d) = env("tr_expert");
        w.set_expert_mode(true);
        let me = AppleTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        {
            let mut s = tree_state(&me).borrow_mut();
            s.dead = true;
            s.time_died = 0.0;
        }
        w.set_world_time(100.0);
        crate::rng::glibc_rand::srand(2);
        tree_update(&me, 0.2, 0.2, false);
        assert!(me.needs_removed());
        assert_eq!(crate::rng::glibc_rand::rand(), draws(2, 1)[0]);
        assert_eq!(tile(&w, 10, GROUND + 1).contents, 0);
    }

    /// Non-expert `removeAllOwnedTiles(YES)` draws one `0.8f < R01` per owned tile of the tree's kind (T6).
    #[test]
    fn remove_all_owned_tiles_draws_per_tile() {
        let (w, d) = env("tr_removeall");
        let me = AppleTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        crate::rng::glibc_rand::srand(4);
        tree_remove_all_owned_tiles(&me, true);
        // three bush tiles => three draws (the removal itself draws nothing in this world)
        assert_eq!(crate::rng::glibc_rand::rand(), draws(4, 4)[3]);
        assert_eq!(count_contents(&w, 0, 64, GROUND + 1, GROUND + 3, &[3, 4, 5, 0x1b, 0x1c]), 0);
        // without free blocks: no draw
        let me2 = AppleTree::new(&w, &d, ip(30, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        crate::rng::glibc_rand::srand(4);
        tree_remove_all_owned_tiles(&me2, false);
        assert_eq!(crate::rng::glibc_rand::rand(), draws(4, 1)[0]);
    }

    /// : chopping the trunk at the base marks the tree for removal (T7).
    #[test]
    fn chop_base_sets_needs_removed() {
        let (w, d) = env("tr_chop");
        let me = CoconutTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, true, 99999.0).unwrap();
        assert!(tree_state(&me).borrow().height >= 1);
        let base = ip(10, GROUND + 1);
        tile_at(&w, base.x, base.y).unwrap().update(|t| t.contents = 0);
        w.set_world_time(50.0);
        tree_world_changed(&me, &mut vec![base]);
        assert!(me.needs_removed());
        assert!(tree_state(&me).borrow().dead);
        assert_eq!(tree_state(&me).borrow().time_died, 50.0);
    }

    /// : chopping a trunk tile above the base shrinks the tree to that height (T7).
    #[test]
    fn chop_trunk_shrinks_height() {
        let (w, d) = env("tr_chop2");
        let me = CoconutTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        for _ in 0..3 {
            tree_state(&me).borrow_mut().growth_counter = 1.0;
            tree_update(&me, 0.2, 0.2, false);
        }
        let h = tree_state(&me).borrow().height;
        assert!(h >= 4);
        let cut = ip(10, GROUND + 3);
        tile_at(&w, cut.x, cut.y).unwrap().update(|t| t.contents = 0);
        tree_world_changed(&me, &mut vec![cut]);
        let s = tree_state(&me).borrow();
        assert_eq!(s.height, 2);
        assert_eq!(s.growth_counter, 0.0);
        assert!(!s.dead);
        assert!(!me.needs_removed());
    }

    ///  R7 /: stale fruit slots are removed by shifting down WITHOUT re-checking the shifted-in
    /// slot, so only every other stale slot goes per tick.
    #[test]
    fn stale_fruit_removal_skips_shifted_entry() {
        let (w, d) = env("tr_stale");
        let me = CherryTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        {
            let mut s = tree_state(&me).borrow_mut();
            for i in 0..4 {
                s.tree_fruits[i] =
                    TreeFruit { pos: ip(40 + i as i32, GROUND + 1), has_created_free_block_this_season: false };
            }
            s.fruit_count = 4;
        }
        // none of these tiles belong to the tree: slots 0 and 2 are removed this tick (1 and 3 skipped)
        tree_state(&me).borrow_mut().height = 1; // keep creation conditions off
        crate::trees::tree::TreeLike::species_update(as_tree(&me).unwrap(), &me, 0.2, 0.2, false);
        let s = tree_state(&me).borrow();
        assert_eq!(s.fruit_count, 2);
        assert_eq!(s.tree_fruits[0].pos, ip(41, GROUND + 1));
        assert_eq!(s.tree_fruits[1].pos, ip(43, GROUND + 1));
    }

    /// Seasonal drop (T8): inside the window the flag flips and the free block carries the child genes
    /// A then B and hovers; outside it the flag resets.
    #[test]
    fn apple_fruit_drop_in_season() {
        let (w, d) = env("tr_fruit");
        let log = record(&d);
        let me = AppleTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        // register a fruit slot on the owned bush tile at x = 9
        let uid = me.unique_id();
        {
            let mut s = tree_state(&me).borrow_mut();
            s.tree_fruits[0] = TreeFruit { pos: ip(9, GROUND + 1), has_created_free_block_this_season: false };
            s.fruit_count = 1;
            s.height = 3;
        }
        assert_eq!(tile(&w, 9, GROUND + 1).dynamic_object_owner, uid);
        let so = tile(&w, 9, GROUND + 1).season_offset;
        // find a world time whose season index is 1 or 2
        let mut found = None;
        let mut t = 0.0f64;
        while t < 3600.0 {
            let s = season_for_world_x(10, t, Some(&w));
            let i = idx_basic(s + 0.003f32 * ((so as i32 - 127) as f32));
            if i == 1 || i == 2 {
                found = Some(t);
                break;
            }
            t += 5.0;
        }
        w.set_world_time(found.expect("a season window exists"));
        crate::rng::glibc_rand::srand(21);
        crate::trees::tree::TreeLike::species_update(as_tree(&me).unwrap(), &me, 0.2, 0.2, false);
        assert!(tree_state(&me).borrow().tree_fruits[0].has_created_free_block_this_season);
        let l = log.borrow();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].0, ip(9, GROUND + 1));
        assert_eq!(l[0].1, 21);
        assert!(l[0].4);
        // dataA/dataB come from two consecutive draws, A first
        let r = draws(21, 3);
        let va = clampi(
            f32_to_i32(tree_state(&me).borrow().max_height_gene as f32 + (r01_from_test(r[0]) - 0.5) * 8.0),
            1,
            255,
        ) as u16;
        let vb = clampi(
            f32_to_i32(tree_state(&me).borrow().growth_rate_gene as f32 + (r01_from_test(r[1]) - 0.5) * 8.0),
            1,
            255,
        ) as u16;
        assert_eq!((l[0].2, l[0].3), (va, vb));
        drop(l);
        // a second tick in the window does not create another block
        crate::trees::tree::TreeLike::species_update(as_tree(&me).unwrap(), &me, 0.2, 0.2, false);
        assert_eq!(log.borrow().len(), 1);
        // outside the window the flag resets: move half a year away
        w.set_world_time(found.unwrap() + 1800.0);
        crate::trees::tree::TreeLike::species_update(as_tree(&me).unwrap(), &me, 0.2, 0.2, false);
        assert!(!tree_state(&me).borrow().tree_fruits[0].has_created_free_block_this_season);
    }

    /// T11: `getSaveDict` -> `from_save_dict` -> `getSaveDict` round trip keeps every key (kinds included).
    #[test]
    fn save_round_trip_all_species() {
        let (w, d) = env("tr_save");
        for (n, (tt, ot, f)) in species().into_iter().enumerate() {
            let me =
                f(&w, &d, ip(4 + 6 * n as i32, GROUND + 1), None, 120, 80, None, None, true, 5000.0).expect("tree");
            let d1 = me.get_save_dict(&me);
            let me2 = new_tree_from_save_dict(ot, &w, &d, &d1, None, None, None).expect("load");
            let d2 = me2.get_save_dict(&me2);
            assert_eq!(
                format!("{:?}", d1.map.keys_in_order().len()),
                format!("{:?}", d2.map.keys_in_order().len()),
                "tree {tt}"
            );
            for k in d1.map.keys_in_order() {
                let a = d1.map.get(&k).unwrap();
                let b = d2.map.get(&k).unwrap_or_else(|| panic!("missing {k} in reloaded {tt}"));
                if k == "treeFruit" || k == "uniqueID" {
                    continue;
                }
                assert_eq!(format!("{:?}", a), format!("{:?}", b), "tree {tt} key {k}");
            }
            assert_eq!(me2.unique_id(), me.unique_id());
        }
    }

    /// Save kinds: ints, floats, doubles and bools exactly as lists (T11).
    #[test]
    fn save_dict_number_kinds() {
        let (w, d) = env("tr_kinds");
        let me = AppleTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        let sd = me.get_save_dict(&me);
        let kind = |k: &str| match sd.map.get(&k.to_string()).unwrap() {
            PlistValue::Int(_) => "int",
            PlistValue::Real { is_float: true, .. } => "float",
            PlistValue::Real { is_float: false, .. } => "double",
            PlistValue::Bool(_) => "bool",
            PlistValue::Array(_) => "array",
            _ => "other",
        };
        for (k, e) in [
            ("height", "int"),
            ("saveTime", "double"),
            ("treeSeasonOffset", "int"),
            ("age", "float"),
            ("dead", "bool"),
            ("timeDied", "double"),
            ("removeCheckCount", "float"),
            ("treeFruit", "array"),
            ("maxHeightGene", "int"),
            ("growthRateGene", "int"),
            ("maxHeightReached", "int"),
            ("growthCounter", "float"),
            ("growthRate", "float"),
            ("maxHeight", "int"),
            ("maxAge", "float"),
            ("availableFood", "float"),
        ] {
            assert_eq!(kind(k), e, "{k}");
        }
    }

    /// Fruit entries whose tile is not owned are dropped on load (T11).
    #[test]
    fn load_drops_unowned_fruit() {
        let (w, d) = env("tr_dropfruit");
        let me = CherryTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        {
            let mut s = tree_state(&me).borrow_mut();
            s.tree_fruits[0] = TreeFruit { pos: ip(9, GROUND + 1), has_created_free_block_this_season: true };
            s.tree_fruits[1] = TreeFruit { pos: ip(50, GROUND + 1), has_created_free_block_this_season: true };
            s.fruit_count = 2;
        }
        let sd = me.get_save_dict(&me);
        let me2 = new_tree_from_save_dict(OBJECT_TYPE_CHERRY, &w, &d, &sd, None, None, None).unwrap();
        let s = tree_state(&me2).borrow();
        assert_eq!(s.fruit_count, 1);
        assert_eq!(s.tree_fruits[0].pos, ip(9, GROUND + 1));
        assert!(s.tree_fruits[0].has_created_free_block_this_season);
    }

    /// Offline catch-up (T10): growth steps while alive, age by completed steps only, `timeDied` formula when
    /// the lifetime ends during the gap.
    #[test]
    fn grow_in_time_since_saved() {
        let (w, d) = env("tr_catchup");
        let me = CoconutTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        let (age0, mh) = {
            let s = tree_state(&me).borrow();
            (s.age, s.max_height)
        };
        let mut sd = me.get_save_dict(&me);
        w.set_world_time(100_000.0);
        sd.map.insert("saveTime".to_string(), PlistValue::real_f64(100_000.0 - 3000.0));
        let me2 = new_tree_from_save_dict(OBJECT_TYPE_COCONUT, &w, &d, &sd, None, None, None).unwrap();
        let s = tree_state(&me2).borrow();
        assert!(s.height > 1 || s.growth_counter > 0.0);
        assert!(s.age >= age0);
        assert!(s.height <= mh);
        drop(s);

        // lifetime ends during the gap
        let max_age = tree_state(&me).borrow().max_age;
        let age = tree_state(&me).borrow().age;
        let mut sd = me.get_save_dict(&me);
        let save_time = 1000.0f64;
        sd.map.insert("saveTime".to_string(), PlistValue::real_f64(save_time));
        w.set_world_time(save_time + max_age as f64 * 2.0);
        let me3 = new_tree_from_save_dict(OBJECT_TYPE_COCONUT, &w, &d, &sd, None, None, None).unwrap();
        let s = tree_state(&me3).borrow();
        assert!(s.dead);
        assert_eq!(s.time_died, (save_time + max_age as f64) - age as f64);
        // `removeAllOwnedTiles(NO)` ran: the owned tiles are gone (the loaded tree is "me3", uid shared with "me")
        drop(s);
        assert_eq!(tile(&w, 10, GROUND + 1).contents, 0);
    }

    /// A tree standing in compost does not die of old age on load: `age = maxAge` (T10).
    #[test]
    fn catchup_in_compost_clamps_age() {
        let (w, d) = env("tr_compost");
        tile_at(&w, 10, GROUND).unwrap().update(|t| t.type_index = 48);
        let me = CoconutTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        let max_age = tree_state(&me).borrow().max_age;
        let mut sd = me.get_save_dict(&me);
        sd.map.insert("saveTime".to_string(), PlistValue::real_f64(0.0));
        w.set_world_time(max_age as f64 * 3.0);
        let me2 = new_tree_from_save_dict(OBJECT_TYPE_COCONUT, &w, &d, &sd, None, None, None).unwrap();
        let s = tree_state(&me2).borrow();
        assert!(!s.dead);
        assert_eq!(s.age, s.max_age);
    }

    /// Compost: `maxHeight * 1.5f` and the growth rate doubles (T4).
    #[test]
    fn compost_boosts_height_and_rate() {
        let (w, d) = env("tr_compost2");
        let a = CoconutTree::new(&w, &d, ip(4, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        tile_at(&w, 20, GROUND).unwrap().update(|t| t.type_index = 49);
        let b = CoconutTree::new(&w, &d, ip(20, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        let (ma, ga) = {
            let s = tree_state(&a).borrow();
            (s.max_height, s.growth_rate)
        };
        let (mb, gb) = {
            let s = tree_state(&b).borrow();
            (s.max_height, s.growth_rate)
        };
        assert_eq!(mb, f32_to_i32(ma as f32 * 1.5));
        assert_eq!(gb, ga * 2.0);
        assert!(tree_is_growing_in_compost(&b));
        assert!(!tree_is_growing_in_compost(&a));
    }

    /// GemTree (T12): static, `fruitYear = rand() % 8` is the first draw, owned tiles of its kind exist, the
    /// height is the highest canopy row, and `Tree.update` never kills it.
    #[test]
    fn gem_tree_builds_and_is_static() {
        let (w, d) = env("tr_gem");
        crate::rng::glibc_rand::srand(77);
        let g = GemTree::new(&w, &d, ip(30, GROUND + 1), None, None, None, 11).unwrap();
        let first = draws(77, 1)[0];
        let gt = g.as_any().downcast_ref::<GemTree>().unwrap();
        assert_eq!(gt.fruit_year(), first % 8);
        let uid = g.unique_id();
        let owned = (0..64)
            .flat_map(|x| (GROUND + 1..GROUND + 40).map(move |y| (x, y)))
            .filter(|&(x, y)| tile(&w, x, y).dynamic_object_owner == uid)
            .count();
        assert!(owned >= 8);
        let h = tree_state(&g).borrow().height;
        assert!(h >= 3, "the canopy tops the trunk");
        assert_eq!(tree_state(&g).borrow().max_height_reached, 0, "R5: maxHeightReached stays 0");
        let tops = tile(&w, 30, GROUND + 1).contents;
        assert!(matches!(tops, 109 | 110 | 111), "{tops}");
        // R5: nothing dies, nothing is removed on unload
        for _ in 0..50 {
            tree_update(&g, 0.2, 0.2, false);
        }
        assert!(!tree_state(&g).borrow().dead);
        tree_remove_from_macro_block(&g);
        assert_eq!(tile(&w, 30, GROUND + 1).contents, tops);
        // the save dict has no growth keys but the gem keys
        let sd = g.get_save_dict(&g);
        assert!(sd.map.get(&"maxHeight".to_string()).is_none());
        assert_eq!(sd.map.get(&"gemTreeType".to_string()).unwrap().int_value(), 11);
        assert_eq!(sd.map.get(&"fruitYear".to_string()).unwrap().int_value(), first % 8);
    }

    /// GemTree `makeTileDead:` maps to the pine dead contents; `tileIsKindOfSelf:` per type.
    #[test]
    fn gem_kind_and_dead_tables() {
        let (w, d) = env("tr_gemkind");
        let g = GemTree::new(&w, &d, ip(30, GROUND + 1), None, None, None, 12).unwrap();
        let t = as_tree(&g).unwrap();
        let mut tl_ = Tile::default();
        for c in 0..=255u8 {
            tl_.contents = c;
            assert_eq!(t.tile_is_kind_of_self(&tl_), matches!(c, 0x22 | 0x1d | 112 | 113 | 114), "{c}");
        }
        tl_.contents = 112;
        t.make_tile_dead(&mut tl_);
        assert_eq!(tl_.contents, 0x1d);
        tl_.contents = 114;
        t.make_tile_dead(&mut tl_);
        assert_eq!(tl_.contents, 0x1d);
        tl_.contents = 113;
        t.make_tile_dead(&mut tl_);
        assert_eq!(tl_.contents, 0x22);
    }

    /// GemTree fruit year (T8/T12): the gem drops only in its fruit year, in band {2,3}, with data 0/0.
    #[test]
    fn gem_drops_in_fruit_year() {
        let (w, d) = env("tr_gemfruit");
        let log = record(&d);
        let g = GemTree::new(&w, &d, ip(30, GROUND + 1), None, None, None, 13).unwrap();
        let gt = g.as_any().downcast_ref::<GemTree>().unwrap();
        {
            let mut s = tree_state(&g).borrow_mut();
            s.fruit_count = 0;
        }
        // use the first owned tile as the fruit slot
        let uid = g.unique_id();
        let mut slot = None;
        'o: for y in GROUND + 1..GROUND + 40 {
            for x in 0..64 {
                if tile(&w, x, y).dynamic_object_owner == uid {
                    slot = Some(ip(x, y));
                    break 'o;
                }
            }
        }
        let slot = slot.unwrap();
        {
            let mut s = tree_state(&g).borrow_mut();
            s.tree_fruits[0] = TreeFruit { pos: slot, has_created_free_block_this_season: false };
            s.fruit_count = 1;
        }
        let so = tile(&w, slot.x, slot.y).season_offset;
        let fy = gt.fruit_year();
        // a world time inside the fruit year whose season index is 2 or 3, and one in another year
        let mut good = None;
        let mut t = fy as f64 * 3600.0;
        while t < (fy as f64 + 1.0) * 3600.0 {
            let s = season_for_world_x(30, t, Some(&w));
            let i =
                f32_to_i32(((8.0f32 * (s + 0.003f32 * ((so as i32 - 127) as f32))) + 4.0f32) + 8.0f32).wrapping_rem(8);
            if i == 2 || i == 3 {
                good = Some(t);
                break;
            }
            t += 5.0;
        }
        let good = good.expect("window");
        // wrong year first: nothing happens
        w.set_world_time(good + 3600.0);
        crate::trees::tree::TreeLike::species_update(as_tree(&g).unwrap(), &g, 0.2, 0.2, false);
        assert!(log.borrow().is_empty());
        w.set_world_time(good);
        crate::rng::glibc_rand::srand(1);
        crate::trees::tree::TreeLike::species_update(as_tree(&g).unwrap(), &g, 0.2, 0.2, false);
        let l = log.borrow();
        assert_eq!(l.len(), 1);
        assert_eq!((l[0].0, l[0].1, l[0].2, l[0].3, l[0].4), (slot, 76, 0, 0, true));
        drop(l);
        assert_eq!(crate::rng::glibc_rand::rand(), draws(1, 1)[0], "no rand in the gem drop");
    }

    /// GemTree `worldChanged:` (T12): a disconnected tile is removed; rolls are `0.8f` then `0.95f`; tiles of
    /// the soil-anchored owned set survive.
    #[test]
    fn gem_world_changed_removes_orphans() {
        let (w, d) = env("tr_gemwc");
        let g = GemTree::new(&w, &d, ip(30, GROUND + 1), None, None, None, 11).unwrap();
        let uid = g.unique_id();
        // find a connected tree tile and an artificial orphan: a lone trunk tile far from the tree, owned by it
        let orphan = ip(50, GROUND + 3);
        tile_at(&w, orphan.x, orphan.y).unwrap().update(|t| {
            t.contents = 109;
            t.dynamic_object_owner = uid;
        });
        let broken = ip(51, GROUND + 3);
        // the broken tile is in range of height*2 only if close: widen the tree's height for the test
        tree_state(&g).borrow_mut().height = 40;
        crate::rng::glibc_rand::srand(8);
        as_tree(&g).unwrap().species_world_changed(&g, &mut vec![broken]);
        // the orphan is removed: 0.8f roll then 0.95f roll (2 draws), the dropped contents path draws nothing
        assert_eq!(tile(&w, orphan.x, orphan.y).contents, 0);
        assert_eq!(crate::rng::glibc_rand::rand(), draws(8, 3)[2]);
        // tiles connected to the soil survive
        assert!(tile(&w, 30, GROUND + 1).dynamic_object_owner == uid);
    }

    /// Orange cold death (T14): Mango has no cold check; Orange kills at `temp < -10`.
    #[test]
    fn orange_cold_death_check_runs() {
        let (w, d) = env("tr_cold");
        let o = OrangeTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        // mild default world: alive
        assert!(!tree_state(&o).borrow().dead);
        // force a very cold tile: artificial heat is signed 16-bit and added after the climate terms
        tile_at(&w, 10, GROUND + 1).unwrap().update(|t| t.artificial_heat = -20000);
        cold_death_check(&o);
        assert!(tree_state(&o).borrow().dead);
        let m = MangoTree::new(&w, &d, ip(30, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        tile_at(&w, 30, GROUND + 1).unwrap().update(|t| t.artificial_heat = -20000);
        as_tree(&m).unwrap().update_growth(&m, false);
        assert!(!tree_state(&m).borrow().dead);
    }

    /// `Tree.update` height == maxHeight seeds exactly once and then dies at the next full counter (T5),
    /// unless in compost. Needs the hooks to observe the sowing: 2 draws with no tile loaded to place.
    #[test]
    fn mature_tree_seeds_then_dies() {
        let (w, d) = env("tr_mature");
        let me = CoconutTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        {
            let mut s = tree_state(&me).borrow_mut();
            s.height = s.max_height - 1;
            s.growth_counter = 1.0;
        }
        crate::rng::glibc_rand::srand(6);
        tree_update(&me, 0.2, 0.2, false);
        {
            let s = tree_state(&me).borrow();
            assert_eq!(s.height, s.max_height);
            assert!(!s.dead);
        }
        // sowTreeNearParent: two attempts, one rand each (+ whatever updateGrowth drew: coconut draws none)
        assert_eq!(crate::rng::glibc_rand::rand(), draws(6, 3)[2]);
        tree_state(&me).borrow_mut().growth_counter = 1.0;
        tree_update(&me, 0.2, 0.2, false);
        assert!(tree_state(&me).borrow().dead);
    }

    /// Flooded base: TILE_WATER at the base kills a full-counter tree (T5).
    #[test]
    fn flooded_base_kills() {
        let (w, d) = env("tr_flood");
        let me = CoconutTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        tile_at(&w, 10, GROUND + 1).unwrap().update(|t| t.type_index = 3);
        tree_state(&me).borrow_mut().growth_counter = 1.0;
        tree_update(&me, 0.2, 0.2, false);
        assert!(tree_state(&me).borrow().dead);
    }

    /// The tree is not created on an occupied tile; the adult growth-counter draw is consumed anyway.
    #[test]
    fn init_rejects_occupied_tile_after_draw() {
        let (w, d) = env("tr_occupied");
        tile_at(&w, 10, GROUND + 1).unwrap().update(|t| t.contents = 7);
        crate::rng::glibc_rand::srand(13);
        let r = AppleTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, true, 1.0);
        assert!(r.is_none());
        assert_eq!(crate::rng::glibc_rand::rand(), draws(13, 2)[1]);
        // static trees: no rand at all
        crate::rng::glibc_rand::srand(13);
        let g = GemTree::new(&w, &d, ip(10, GROUND + 1), None, None, None, 11);
        assert!(g.is_none());
        assert_eq!(crate::rng::glibc_rand::rand(), draws(13, 1)[0]);
    }

    /// Pine offline food catch-up: `availableFood += 0.5 * (now - (float)saveTime)`.
    #[test]
    fn pine_food_catchup() {
        let (w, d) = env("tr_pine");
        let me = PineTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        let mut sd = me.get_save_dict(&me);
        let food = sd.map.get(&"availableFood".to_string()).unwrap().float_value();
        sd.map.insert("saveTime".to_string(), PlistValue::real_f64(100.0));
        w.set_world_time(300.0);
        let me2 = new_tree_from_save_dict(OBJECT_TYPE_PINE, &w, &d, &sd, None, None, None).unwrap();
        let sd2 = me2.get_save_dict(&me2);
        let f2 = sd2.map.get(&"availableFood".to_string()).unwrap().float_value();
        assert_eq!(f2, (food as f64 + 0.5 * (300.0 - 100.0f32 as f64)) as f32);
    }

    /// `getTreeLifeFractionForPos` etc. reach trees through `tree_height` / gene variations (1 rand each).
    #[test]
    fn gene_variation_hooks_one_rand_each() {
        let (w, d) = env("tr_genes");
        let me = AppleTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        crate::rng::glibc_rand::srand(5);
        let _ = tree_max_height_gene_variation(&me);
        let _ = tree_growth_rate_gene_variation(&me);
        assert_eq!(crate::rng::glibc_rand::rand(), draws(5, 3)[2]);
        assert_eq!(tree_height(&me), tree_state(&me).borrow().height);
    }

    /// Cactus compost: two free blocks per spawn (T8).
    #[test]
    fn cactus_two_blocks_in_compost() {
        let (w, d) = env("tr_cactusfruit");
        let log = record(&d);
        tile_at(&w, 10, GROUND).unwrap().update(|t| t.type_index = 48);
        let me = CactusTree::new(&w, &d, ip(10, GROUND + 1), None, 100, 100, None, None, false, 0.0).unwrap();
        let uid = me.unique_id();
        {
            let mut s = tree_state(&me).borrow_mut();
            s.tree_fruits[0] = TreeFruit { pos: ip(10, GROUND + 1), has_created_free_block_this_season: false };
            s.fruit_count = 1;
        }
        assert_eq!(tile(&w, 10, GROUND + 1).dynamic_object_owner, uid);
        let so = tile(&w, 10, GROUND + 1).season_offset;
        let mut t = 0.0f64;
        let mut found = None;
        while t < 3600.0 {
            let s = season_for_world_x(10, t, Some(&w));
            let i = idx_basic((256.0f32 + s) + 0.05f32 * ((so as i32 - 127) as f32));
            if i == 2 || i == 3 {
                found = Some(t);
                break;
            }
            t += 5.0;
        }
        w.set_world_time(found.unwrap());
        crate::trees::tree::TreeLike::species_update(as_tree(&me).unwrap(), &me, 0.2, 0.2, false);
        let l = log.borrow();
        assert_eq!(l.len(), 2);
        assert!(l.iter().all(|e| e.1 == 24 && e.4));
    }

    /// Smoke/invariant test over all species with adult pre-growth and many ticks: every owned tile is of the
    /// tree's kind, heights stay within `maxHeight`, fruit slots reference owned tiles, no panic (T3/T4).
    #[test]
    fn adult_trees_invariants() {
        for seed in 1..6u32 {
            let (w, d) = env(&format!("tr_inv{seed}"));
            crate::rng::glibc_rand::srand(seed);
            let mut trees = vec![];
            for (n, (_tt, _ot, f)) in species().into_iter().enumerate() {
                let x = 3 + 6 * n as i32;
                if let Some(me) =
                    f(&w, &d, ip(x, GROUND + 1), None, (50 + 40 * seed) as i16, 120, None, None, true, 99999.0)
                {
                    trees.push(me);
                }
            }
            assert_eq!(trees.len(), 10);
            for me in &trees {
                let t = as_tree(me).unwrap();
                let s = tree_state(me).borrow();
                assert!(
                    s.height >= 1 && s.height <= s.max_height,
                    "{} h={} mh={}",
                    me.object_type(),
                    s.height,
                    s.max_height
                );
                assert!(s.max_height_reached >= s.height);
                drop(s);
                let uid = me.unique_id();
                let pos = me.pos();
                for y in GROUND + 1..GROUND + 60 {
                    for x in pos.x - 40..pos.x + 40 {
                        let tt = tile_at(&w, x, y).unwrap().get();
                        if tt.dynamic_object_owner == uid {
                            assert!(
                                t.tile_is_kind_of_self(&tt),
                                "obj {} owns foreign contents {} at ({x},{y})",
                                me.object_type(),
                                tt.contents
                            );
                        }
                    }
                }
                let s = tree_state(me).borrow();
                for i in 0..s.fruit_count as usize {
                    let ft = tile(&w, s.tree_fruits[i].pos.x, s.tree_fruits[i].pos.y);
                    assert_eq!(ft.dynamic_object_owner, uid);
                }
            }
            for k in 0..3000 {
                w.set_world_time(k as f64 * 0.2);
                for me in &trees {
                    crate::trees::tree::TreeLike::species_update(as_tree(me).unwrap(), me, 0.2, 0.2, false);
                }
            }
        }
    }
}

/// Whole-life scenarios of every species: growth over the seasons, fruit, saving and loading, neighbours changing
/// and death.
#[cfg(test)]
mod scenario_tests {
    use super::testenv::{env, GROUND};
    use super::*;
    use crate::objects::handle::downcast;
    use crate::rng::glibc_rand::srand;
    use crate::trees::cactus::CactusTree;

    const SPECIES: [(i32, i32); 15] = [
        (TREE_APPLE, OBJECT_TYPE_APPLE),
        (TREE_MANGO, OBJECT_TYPE_MANGO),
        (TREE_MAPLE, OBJECT_TYPE_MAPLE),
        (TREE_PINE, OBJECT_TYPE_PINE),
        (TREE_CACTUS, OBJECT_TYPE_CACTUS),
        (TREE_COCONUT, OBJECT_TYPE_COCONUT),
        (TREE_ORANGE, OBJECT_TYPE_ORANGE),
        (TREE_CHERRY, OBJECT_TYPE_CHERRY),
        (TREE_COFFEE, OBJECT_TYPE_COFFEE),
        (TREE_LIME, OBJECT_TYPE_LIME),
        (11, OBJECT_TYPE_GEM),
        (12, OBJECT_TYPE_GEM),
        (13, OBJECT_TYPE_GEM),
        (14, OBJECT_TYPE_GEM),
        (15, OBJECT_TYPE_GEM),
    ];

    fn plant(w: &Rc<World>, d: &Rc<DynamicWorld>, tree_type: i32, x: i32, adult: bool) -> Option<ObjRef> {
        new_tree_for_type(tree_type, w, d, IntPair { x, y: GROUND + 1 }, None, 9, 5, None, None, adult, 99_999.0)
    }

    #[test]
    fn every_species_lives_through_the_seasons() {
        for (k, (tree_type, object_type)) in SPECIES.iter().enumerate() {
            let (w, d) = env(&format!("tr_life_{k}"));
            let x = 10 + 3 * (k as i32 % 3);
            for adult in [false, true] {
                for seed in 1..4u32 {
                    srand(seed);
                    let Some(t) = plant(&w, &d, *tree_type, x, adult) else { continue };
                    assert_eq!(t.object_type(), *object_type);
                    for step in 0..150 {
                        w.set_world_time(f64::from(step) * 37.5);
                        t.update_accurate_dt_is_simulation(&t, 500.0, 500.0, false);
                    }
                    // the animals living in pines and cacti come when the food is there
                    if let Some(cactus) = downcast::<CactusTree>(&t) {
                        for _ in 0..4 {
                            cactus.set_available_food(5000.0);
                            t.update_accurate_dt_is_simulation(&t, 5.0, 5.0, false);
                        }
                    }
                    // old age: the tree dies, its tiles die with it and are removed a minute later
                    if seed == 2 {
                        let max_age = tl(&t).tree().borrow().max_age;
                        tl(&t).tree().borrow_mut().age = max_age + 10.0;
                        for step in 0..40 {
                            w.set_world_time(10_000.0 + f64::from(step) * 3.0);
                            t.update_accurate_dt_is_simulation(&t, 1.5, 1.5, false);
                        }
                        w.set_world_time(10_200.0);
                        t.update_accurate_dt_is_simulation(&t, 1.5, 1.5, false);
                        assert!(t.needs_removed() || *tree_type == TREE_CACTUS || *object_type == OBJECT_TYPE_GEM);
                    }
                    // a save and a reload long after
                    let dict = t.get_save_dict(&t);
                    w.set_world_time(50_000.0);
                    let back = new_tree_from_save_dict(*object_type, &w, &d, &dict, None, None, None);
                    if let Some(back) = back {
                        back.update_accurate_dt_is_simulation(&back, 25.0, 25.0, false);
                    }
                    // a change next to it, then the end
                    t.world_changed(&t, &mut vec![IntPair { x, y: GROUND }, IntPair { x: x + 1, y: GROUND + 3 }]);
                    t.set_needs_removed(&t, true);
                    t.remove_from_macro_block(&t);
                    // clean the tiles for the next round
                    for y in GROUND + 1..GROUND + 30 {
                        for dx in -6..=6 {
                            if let Some(tr) = crate::world::tiles::tile_at_world_position_loaded(x + dx, y, &w) {
                                tr.update(|tile| {
                                    tile.type_index = 2;
                                    tile.contents = 0;
                                    tile.foreground_contents = 0;
                                    tile.back_wall_type_index = 2;
                                    tile.sun_light = 255;
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn fruit_falls_and_the_base_selectors_answer() {
        struct Fixed(f64);
        impl crate::dynworld::state::DwNoise for Fixed {
            fn get_x_y_octaves(&self, _x: f64, _y: f64, _o: i32) -> f64 {
                self.0
            }
        }
        let noise: Rc<dyn crate::dynworld::state::DwNoise> = Rc::new(Fixed(0.3));
        for (k, (tree_type, object_type)) in SPECIES.iter().enumerate() {
            let (w, d) = env(&format!("tr_fruit_{k}"));
            srand(3);
            let x = 12;
            let Some(t) = new_tree_for_type(
                *tree_type,
                &w,
                &d,
                IntPair { x, y: GROUND + 1 },
                None,
                9,
                5,
                Some(noise.clone()),
                Some(noise.clone()),
                true,
                99_999.0,
            ) else {
                continue;
            };
            let like = tl(&t);
            let _ = (like.tree_type(), like.fruit_item_type(), like.should_add_fallen_fruits());
            let _ = (like.fruit_should_fall_in_season(1), like.is_static_tree(), like.height());
            for step in 0..160 {
                w.set_world_time(f64::from(step) * 22.5);
                t.update_accurate_dt_is_simulation(&t, 500.0, 500.0, false);
                tree_add_fallen_fruits(&t);
            }
            like.update_growth(&t, true);
            like.update_growth(&t, false);
            assert_eq!(t.object_type(), *object_type);
        }
        let _ = TreeState::default();
    }

    #[test]
    fn trees_need_loaded_ground() {
        let (w, d) = env("tr_unloaded");
        let dict = {
            let t = plant(&w, &d, TREE_APPLE, 10, false).unwrap();
            t.get_save_dict(&t)
        };
        for mx in 0..2 {
            *w.macro_tiles().get(mx).unwrap().physical_block.borrow_mut() = None;
        }
        for (tree_type, object_type) in SPECIES {
            assert!(plant(&w, &d, tree_type, 12, false).is_none() || tree_type == 11);
            let _ = new_tree_from_save_dict(object_type, &w, &d, &dict, None, None, None);
        }
        assert!(new_tree_from_save_dict(99, &w, &d, &dict, None, None, None).is_none());
        assert!(plant(&w, &d, 99, 12, false).is_none());
    }
}

/// Random disturbances around every species: neighbours appear and vanish, fire, water, saved and reloaded.
#[cfg(test)]
mod fuzz_tests {
    use super::testenv::{env, GROUND};
    use super::*;
    use crate::rng::glibc_rand::srand;

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.0 >> 33
        }
        fn pick<T: Copy>(&mut self, v: &[T]) -> T {
            v[(self.next() % v.len() as u64) as usize]
        }
    }

    #[test]
    fn owner_ids_are_migrated_to_the_wide_field() {
        let (w, d) = env("tr_fuzz_ids");
        let t = new_tree_for_type(
            TREE_APPLE,
            &w,
            &d,
            IntPair { x: 20, y: GROUND + 1 },
            None,
            9,
            5,
            None,
            None,
            true,
            99_999.0,
        )
        .unwrap();
        let uid = t.unique_id();
        let tile = crate::world::tiles::tile_at_world_position_loaded(21, GROUND + 3, &w).unwrap();
        tile.update(|x| {
            x.dynamic_object_owner_old = uid as u32;
            x.dynamic_object_owner = 0;
        });
        tree_update_all_owned_tiles_to_new_id_size(&t);
        assert_eq!(tile.get().dynamic_object_owner, uid);
    }

    #[test]
    fn disturbed_trees_of_every_species() {
        let species = [
            TREE_APPLE,
            TREE_MANGO,
            TREE_MAPLE,
            TREE_PINE,
            TREE_CACTUS,
            TREE_COCONUT,
            TREE_ORANGE,
            TREE_CHERRY,
            TREE_COFFEE,
            TREE_LIME,
            11,
            12,
            13,
            14,
            15,
        ];
        let mut r = Lcg(5);
        for (k, tree_type) in species.into_iter().enumerate() {
            let (w, d) = env(&format!("tr_fuzz_{k}"));
            let x = 20;
            for round in 0..6u32 {
                srand(round + 1);
                let Some(t) = new_tree_for_type(
                    tree_type,
                    &w,
                    &d,
                    IntPair { x, y: GROUND + 1 },
                    None,
                    9,
                    5,
                    None,
                    None,
                    round % 2 == 0,
                    99_999.0,
                ) else {
                    continue;
                };
                for step in 0..60 {
                    w.set_world_time(f64::from(round) * 4000.0 + f64::from(step) * 61.0);
                    t.update_accurate_dt_is_simulation(&t, 12.0, 12.0, step % 7 == 0);
                    if r.next() % 3 == 0 {
                        let mut changed = vec![];
                        for _ in 0..(1 + r.next() % 6) {
                            let p = IntPair { x: x + (r.next() % 13) as i32 - 6, y: GROUND + (r.next() % 14) as i32 };
                            if let Some(tr) = crate::world::tiles::tile_at_world_position_loaded(p.x, p.y, &w) {
                                let (ty, fire, contents) = (
                                    r.pick(&[2u8, 2, 6, 3, 1]),
                                    r.pick(&[0u8, 0, 0, 1]),
                                    r.pick(&[0u8, 0, 0, 0x30, 0x03]),
                                );
                                if p.y > GROUND && r.next() % 2 == 0 {
                                    tr.update(|tile| {
                                        tile.type_index = ty;
                                        tile.on_fire = fire;
                                        if tile.dynamic_object_owner == 0 {
                                            tile.contents = contents;
                                        }
                                    });
                                }
                                changed.push(p);
                            }
                        }
                        t.world_changed(&t, &mut changed);
                    }
                    if step == 30 {
                        let dict = t.get_save_dict(&t);
                        w.set_world_time(f64::from(round) * 4000.0 + 20_000.0);
                        if let Some(back) = new_tree_from_save_dict(t.object_type(), &w, &d, &dict, None, None, None) {
                            back.update_accurate_dt_is_simulation(&back, 5.0, 5.0, false);
                            back.world_changed(&back, &mut vec![IntPair { x, y: GROUND + 2 }]);
                            back.set_needs_removed(&back, true);
                            back.remove_from_macro_block(&back);
                        }
                    }
                }
                t.set_needs_removed(&t, true);
                t.remove_from_macro_block(&t);
                for y in GROUND + 1..GROUND + 30 {
                    for dx in -8..=8 {
                        if let Some(tr) = crate::world::tiles::tile_at_world_position_loaded(x + dx, y, &w) {
                            tr.update(|tile| {
                                tile.type_index = 2;
                                tile.contents = 0;
                                tile.foreground_contents = 0;
                                tile.on_fire = 0;
                                tile.dynamic_object_owner = 0;
                                tile.back_wall_type_index = 2;
                                tile.sun_light = 255;
                            });
                        }
                    }
                }
            }
        }
    }
}
