//! `ArtificialLight` (DynamicObject type 21, instance size 0x90): the BFS light grid that adds R/G/B/heat to tiles.
//! Also hosts [`LightingServices`], the single seam through which the whole `lighting_env` module reaches the
//! `World` / `DynamicWorld` methods that belong to world_ops / dynamic_world_ops (not yet implemented as inherent
//! methods when this module was written
//!
//! requirements),.2, 4, 7 (items 1, 2).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

use crate::fmath::cconv::f32_to_i32;
use crate::objects::base::{
    base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_save_dict_cache, base_remove_from_macro_block, DynamicObject, DynamicObjectBase,
    DynamicWorldRef, WorldRef,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{self, CacheRef, ObjRef, ObjWeak};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::structs::{ElectrictyParticlePathIndex, IntPair};
use crate::world::helpers::{
    tile_is_air_water_or_snow, tile_is_semi_transparent_solid_block, tile_is_water, tile_requires_glow_block,
};
use crate::world::state::{MacroTileRef, PhysicalBlockRef, TileRef, World};
use crate::world::tiles::{get_world_pos_for_world_index, tile_at_world_position, world_index_at_world_position};

// =====================================================================================================
// Services seam
// =====================================================================================================

/// Every call `lighting_env` makes into `World` (world_ops / world_core) and `DynamicWorld`
/// (dynamic_world_ops) that is not (yet) an inherent method of the concrete type. All methods have inert defaults
/// so a partial integration or a unit-test double overrides only what it needs. The integrator installs the real
/// implementation once with [`install_services`] (a thin adapter calling the real `World` / `DynamicWorld`).
///
#[allow(unused_variables)]
pub trait LightingServices {
    // ---- World ----
    /// `-[World saveSunlightChangedAtPos:]` = `dw.lightChangedAtMacroPos:(x/32, y/32) sendReliably:NO`.
    fn save_sunlight_changed_at_pos(&self, world: &World, pos: IntPair) {}
    /// `-[World fillTile:atPos:withType:]`.
    fn fill_tile_at_pos_with_type(&self, world: &World, tile: TileRef, pos: IntPair, item_type: i32) {}
    /// `-[World getWeatherFractionForPos:]`
    fn get_weather_fraction_for_pos(&self, world: &World, pos: IntPair) -> f32 {
        0.0
    }
    /// `-[World getWeatherFractionForPos:atWorldTime:]`
    fn get_weather_fraction_for_pos_at_world_time(&self, world: &World, pos: IntPair, world_time: f64) -> f32 {
        0.0
    }
    /// `-[World getDayNightFractionForX:atWorldTime:]`
    fn get_day_night_fraction_for_x_at_world_time(&self, world: &World, x: f32, world_time: f64) -> f32 {
        0.0
    }
    /// `-[World loadLightBlockForClientLightBlockIndex:intoPhysicalBlock:]`
    fn load_light_block_for_client_light_block_index_into_physical_block(
        &self,
        world: &World,
        index: i32,
        pb: PhysicalBlockRef,
    ) {
    }
    /// `-[World decommisionBlock:blockToSavePhyscialBlock:]`
    fn decommision_block_block_to_save_physcial_block(
        &self,
        world: &World,
        macro_tile: &MacroTileRef,
        block_to_save_physcial_block: bool,
    ) {
    }
    /// `[[world peersInterestedInMacroIndex:idx] count]`
    fn peers_interested_in_macro_index_count(&self, world: &World, macro_index: i32) -> usize {
        0
    }
    /// `-[World removeWaterTileAtPos:]`
    fn remove_water_tile_at_pos(&self, world: &World, pos: IntPair) {}
    /// `-[World waterMovedFrom:fromTile:to:toTile:amount:]`
    fn water_moved_from_from_tile_to_to_tile_amount(
        &self,
        world: &World,
        from_pos: IntPair,
        from_tile: TileRef,
        to_pos: IntPair,
        to_tile: TileRef,
        amount: i32,
    ) {
    }
    /// `-[World sendNetDataForElectricityParticlePathIfRequired:size:ignoreClient:]`
    fn send_net_data_for_electricity_particle_path_if_required_size_ignore_client(
        &self,
        world: &World,
        path: Vec<ElectrictyParticlePathIndex>,
        size: f32,
        ignore_client: Option<&str>,
    ) {
    }

    // ---- DynamicWorld ----
    /// `-[DynamicWorld lightChangedAtMacroPos:sendReliably:]` (forwards with sendAtAll = YES,)
    fn light_changed_at_macro_pos_send_reliably(&self, macro_pos: IntPair, send_reliably: bool) {}
    /// `-[DynamicWorld lightChangedAtMacroPos:sendReliably:sendAtAll:]`
    fn light_changed_at_macro_pos_send_reliably_send_at_all(
        &self,
        macro_pos: IntPair,
        send_reliably: bool,
        send_at_all: bool,
    ) {
    }
    /// `-[DynamicWorld exploreLightChangedAtMacroPos:clientLightBlockIndex:]`
    fn explore_light_changed_at_macro_pos_client_light_block_index(
        &self,
        macro_pos: IntPair,
        client_light_block_index: i32,
    ) {
    }
    /// `-[DynamicWorld snowChangedAtMacroPos:]`
    fn snow_changed_at_macro_pos(&self, macro_pos: IntPair) {}
    /// `-[DynamicWorld waterChangedAtPos:fullBlock:]`
    fn water_changed_at_pos_full_block(&self, pos: IntPair, full_block: bool) {}
    /// `-[DynamicWorld hasDynamicObjectsToSaveInMacroPos:]`
    fn has_dynamic_objects_to_save_in_macro_pos(&self, macro_pos: IntPair) -> bool {
        false
    }
    /// `[dynamicWorld blockheads]` in index order.
    fn blockheads(&self) -> Vec<ObjRef> {
        Vec::new()
    }
    /// `-[DynamicWorld addTorchAtPos:ofType:dataA:dataB:saveDict:placedByClient:]`
    fn add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(
        &self,
        pos: IntPair,
        item_type: i32,
        data_a: u16,
        data_b: u16,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
    ) {
    }
    /// `-[DynamicWorld createTreasureChestOrTrollAtTile:atPos:loadTroll:loadTreasure:]`
    fn create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(
        &self,
        tile: TileRef,
        pos: IntPair,
        load_troll: bool,
        load_treasure: bool,
    ) {
    }
    /// `-[DynamicWorld loadGlowBlockIfNeededAtPos:tile:]`
    fn load_glow_block_if_needed_at_pos_tile(&self, pos: IntPair, tile: TileRef) {}
    /// `-[DynamicWorld loadSurfaceBlockAtPos:]`
    fn load_surface_block_at_pos(&self, pos: IntPair) {}
    /// `-[DynamicWorld createFreeBlockAtPosition:ofType:dataA:dataB:subItems:nil dynamicObjectSaveDict:hovers:playSound:priorityBlockhead:nil]`
    fn create_free_block_at_position_of_type_data_a_data_b_dynamic_object_save_dict_hovers_play_sound(
        &self,
        pos: IntPair,
        item_type: i32,
        data_a: u16,
        data_b: u16,
        save_dict: PlistDict,
        hovers: bool,
        play_sound: bool,
    ) {
    }
    /// `-[DynamicWorld removeStandardObject:]`
    fn remove_standard_object(&self, obj: &ObjRef) {}

    // ---- Blockhead ----
    /// `[blockhead viewRadius]`
    fn blockhead_view_radius(&self, blockhead: &ObjRef) -> i32 {
        0
    }
}

/// The default services: every inert default.
pub struct NullServices;
impl LightingServices for NullServices {}

thread_local! {
    static SERVICES: RefCell<Rc<dyn LightingServices>> = RefCell::new(Rc::new(NullServices));
}

/// Install the real adapter (main thread). Replaces the previous one.
pub fn install_services(s: Rc<dyn LightingServices>) {
    SERVICES.with(|c| *c.borrow_mut() = s);
}

/// The installed services (cheap `Rc` clone; never hold across a re-entrant call that may reinstall).
pub fn services() -> Rc<dyn LightingServices> {
    SERVICES.with(|c| c.borrow().clone())
}

// =====================================================================================================
// ArtificialLight
// =====================================================================================================

/// `ArtificialLight` class descriptor.
pub static ARTIFICIAL_LIGHT_CLASS: ClassInfo =
    ClassInfo { name: "ArtificialLight", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &[] };

/// `LightDirectionType::ALL`.
pub const LIGHT_DIRECTION_ALL: i32 = 0;
/// `LightDirectionType::DOWN`
pub const LIGHT_DIRECTION_DOWN: i32 = 1;
/// `LightDirectionType::UP`
pub const LIGHT_DIRECTION_UP: i32 = 2;

/// `DYNAMIC_OBJECT_TYPE_ARTIFICIAL_LIGHT`
pub const DYNAMIC_OBJECT_TYPE_ARTIFICIAL_LIGHT: i32 = 21;

/// ArtificialLight ivars 0x50..0x90.
#[derive(Default)]
pub struct ArtificialLightState {
    /// `int16_t* contributionGrid` (0x50): `diameter*diameter*5`, cell base `(gy*diameter+gx)*5`:
    /// `[contribution, R, G, B, heat]`.
    pub contribution_grid: Vec<i16>,
    /// `uint8_t* addedGrid` (0x58): `diameter*diameter`.
    pub added_grid: Vec<u8>,
    /// `maxRed` (0x60)
    pub max_red: i32,
    /// `maxGreen` (0x64)
    pub max_green: i32,
    /// `maxBlue` (0x68)
    pub max_blue: i32,
    /// `maxHeat` (0x6c)
    pub max_heat: i32,
    /// `radius` (0x70)
    pub radius: i32,
    /// `contributionGridOrigin` (0x74/0x78)
    pub contribution_grid_origin: IntPair,
    /// `diameter` (0x7c) = `radius << 1`
    pub diameter: i32,
    /// `lightDirection` (0x80)
    pub light_direction: i32,
    /// `parentObject` (0x88): assigned, not retained.
    pub parent_object: Option<ObjWeak>,
}

/// `ArtificialLight: DynamicObject` (type 21).
pub struct ArtificialLight {
    base: RefCell<DynamicObjectBase>,
    /// The ArtificialLight ivars.
    pub st: RefCell<ArtificialLightState>,
    /// Unretained `World*` (the base only keeps the abstract `WorldApi`).
    world: Weak<World>,
}

/// Immutable per-call parameters of the BFS, copied out of the state so no borrow is held.
#[derive(Clone, Copy)]
struct Params {
    pos: IntPair,
    origin: IntPair,
    radius: i32,
    diameter: i32,
    direction: i32,
    wt: i32,
}

#[inline]
fn width_tiles(world: &World) -> i32 {
    world.world_width_macro().wrapping_mul(32)
}

/// `max(w, (r*k)/6)` implemented as in the binary: `if (r*k)/6 < w+1 then w else (r*k)/6`.
#[inline]
fn pen(r: i32, k: i32, w: i32) -> i32 {
    let v = r.wrapping_mul(k) / 6;
    if v < w + 1 {
        w
    } else {
        v
    }
}

/// `wrapdiff(a, b)` of: shortest signed horizontal distance.
#[inline]
fn wrapdiff(a: i32, b: i32, wt: i32) -> i32 {
    let d = a.wrapping_sub(b);
    if d < wt / 2 {
        if d < wt.wrapping_neg() / 2 {
            d.wrapping_add(wt)
        } else {
            d
        }
    } else {
        d.wrapping_sub(wt)
    }
}

/// Allocate zeroed grids for `diameter` (negative or zero diameters give empty grids).
fn alloc_grids(diameter: i32) -> (Vec<i16>, Vec<u8>) {
    if diameter <= 0 {
        return (Vec::new(), Vec::new());
    }
    let n = (diameter as usize) * (diameter as usize);
    (vec![0i16; n * 5], vec![0u8; n])
}

/// One BFS step. `grid` is the local copy of the contribution grid.
fn bfs_step(p: &Params, grid: &mut [i16], world: &World, open_list: &mut VecDeque<u32>) {
    let Some(front) = open_list.pop_front() else { return };
    let idx = front as i32;
    let wp = get_world_pos_for_world_index(idx, world);
    let (mut wx, wy) = (wp.x, wp.y);
    let macro_tiles = world.macro_tiles();
    if tile_at_world_position(wx, wy, &macro_tiles, world).is_none() {
        return;
    }
    // wrap wx to the shortest horizontal image relative to the light
    if wx.wrapping_sub(p.pos.x) > p.wt / 2 {
        wx = wx.wrapping_sub(p.wt);
    } else if wx.wrapping_sub(p.pos.x) < p.wt.wrapping_neg() / 2 {
        wx = wx.wrapping_add(p.wt);
    }
    let cx = wx.wrapping_sub(p.origin.x);
    let cy = wy.wrapping_sub(p.origin.y);
    if !(0 <= cx && cx < p.diameter && 0 <= cy && cy < p.diameter) {
        return;
    }
    let ci = (cy * p.diameter + cx) as usize;
    let mut contribution = grid[ci * 5] as i32;
    let r = p.radius;
    let dir = p.direction;
    // nVal for [above, below, right, left]; the "above" entry is never written (shadowing quirk,).
    let mut nval = [0i32; 4];
    if wx == p.pos.x && wy == p.pos.y {
        contribution = r.wrapping_mul(10);
    } else {
        let transparent = |t: Option<TileRef>| tile_is_air_water_or_snow(t) || tile_is_semi_transparent_solid_block(t);
        // Evaluate one neighbour; returns Some((raw, candidate)) when a candidate was computed from a real tile.
        let mut cand = |dx: i32, dy: i32, f: &dyn Fn(i32, Option<TileRef>) -> i32| -> Option<(i32, bool)> {
            let nx = wx.wrapping_add(dx);
            let ny = wy.wrapping_add(dy);
            let gx = nx.wrapping_sub(p.origin.x);
            let gy = ny.wrapping_sub(p.origin.y);
            if !(0 <= gx && gx < p.diameter && 0 <= gy && gy < p.diameter) {
                return None;
            }
            let raw = grid[((gy * p.diameter + gx) as usize) * 5] as i32;
            let mut v = raw;
            let mut used = false;
            if raw > 1 {
                let nt = tile_at_world_position(nx, ny, &macro_tiles, world);
                if nt.is_some() {
                    v = f(raw, nt);
                    used = true;
                    if contribution < v {
                        contribution = v;
                    }
                }
            }
            Some((v, used))
        };
        // above (0,+1): value used only for contribution (push test sees 0)
        let _ = cand(0, 1, &|raw, nt| {
            if dir == LIGHT_DIRECTION_UP || !transparent(nt) {
                if dir == LIGHT_DIRECTION_UP {
                    0
                } else {
                    raw.wrapping_sub(pen(r, 10, 10))
                }
            } else if tile_is_water(nt) {
                raw.wrapping_sub(pen(r, 5, 5))
            } else {
                raw.wrapping_sub(10)
            }
        });
        // below (0,-1)
        if let Some((v, _)) = cand(0, -1, &|raw, nt| {
            if dir == LIGHT_DIRECTION_DOWN || !transparent(nt) {
                if dir == LIGHT_DIRECTION_DOWN {
                    0
                } else {
                    raw.wrapping_sub(pen(r, 10, 10))
                }
            } else if tile_is_water(nt) {
                raw.wrapping_sub(pen(r, 5, 5))
            } else {
                raw.wrapping_sub(10)
            }
        }) {
            nval[1] = v;
        }
        let side = |raw: i32, nt: Option<TileRef>| {
            let both = dir == LIGHT_DIRECTION_DOWN || dir == LIGHT_DIRECTION_UP;
            if both || !transparent(nt) {
                if both {
                    raw.wrapping_sub(pen(r, 20, 20))
                } else {
                    raw.wrapping_sub(pen(r, 10, 10))
                }
            } else if tile_is_water(nt) {
                raw.wrapping_sub(pen(r, 5, 5))
            } else {
                raw.wrapping_sub(10)
            }
        };
        // right (+1,0), left (-1,0)
        if let Some((v, _)) = cand(1, 0, &side) {
            nval[2] = v;
        }
        if let Some((v, _)) = cand(-1, 0, &side) {
            nval[3] = v;
        }
        // aboveRight / belowRight / aboveLeft / belowLeft: candidates only feed `contribution`
        let diag_up = |raw: i32, nt: Option<TileRef>| {
            let both = dir == LIGHT_DIRECTION_DOWN || dir == LIGHT_DIRECTION_UP;
            if both || !transparent(nt) {
                if dir == LIGHT_DIRECTION_UP {
                    0
                } else {
                    raw.wrapping_sub(pen(r, 14, 14))
                }
            } else if tile_is_water(nt) {
                raw.wrapping_sub(pen(r, 7, 7))
            } else {
                raw.wrapping_sub(14)
            }
        };
        let diag_down = |raw: i32, nt: Option<TileRef>| {
            let both = dir == LIGHT_DIRECTION_DOWN || dir == LIGHT_DIRECTION_UP;
            if both || !transparent(nt) {
                if dir == LIGHT_DIRECTION_DOWN {
                    0
                } else {
                    raw.wrapping_sub(pen(r, 14, 14))
                }
            } else if tile_is_water(nt) {
                raw.wrapping_sub(pen(r, 7, 7))
            } else {
                raw.wrapping_sub(14)
            }
        };
        let _ = cand(1, 1, &diag_up);
        let _ = cand(1, -1, &diag_down);
        let _ = cand(-1, 1, &diag_up);
        let _ = cand(-1, -1, &diag_down);
    }
    if (grid[ci * 5] as i32) < contribution {
        grid[ci * 5] = contribution as i16;
        const DIRS: [(i32, i32); 4] = [(0, 1), (0, -1), (1, 0), (-1, 0)];
        for (n, (dx, dy)) in DIRS.iter().enumerate() {
            if nval[n] < contribution {
                let n_idx = world_index_at_world_position(wx.wrapping_add(*dx), wy.wrapping_add(*dy), world) as u32;
                if !open_list.contains(&n_idx) {
                    open_list.push_back(n_idx);
                }
            }
        }
    }
}

impl ArtificialLight {
    fn alloc(world: &Rc<World>) -> Rc<ArtificialLight> {
        let w = Rc::downgrade(world);
        handle::alloc_obj(|sw| ArtificialLight {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            st: RefCell::new(ArtificialLightState::default()),
            world: w,
        })
    }

    fn world(&self) -> Rc<World> {
        self.world.upgrade().expect("ArtificialLight: world dropped")
    }

    fn params(&self, world: &World) -> Params {
        let st = self.st.borrow();
        let pos = self.base.borrow().pos;
        Params {
            pos,
            origin: st.contribution_grid_origin,
            radius: st.radius,
            diameter: st.diameter,
            direction: st.light_direction,
            wt: width_tiles(world),
        }
    }

    /// `-[ArtificialLight initWithWorld:dynamicWorld:atPosition:cache:parentObject:colorR:colorG:colorB:heat:radius:lightDirection:]`
    /// Runs [`add_to_tiles`](Self::add_to_tiles) before returning. The `isClient` guard never
    /// fires (it is sent to the still-nil ivar; and the server is never a client).
    #[allow(clippy::too_many_arguments)]
    pub fn init_with_world_dynamic_world_at_position_cache_parent_object_color_r_color_g_color_b_heat_radius_light_direction(
        world: &Rc<World>,
        dynamic_world: &DynamicWorldRef,
        pos: IntPair,
        cache: CacheRef,
        parent_object: Option<&ObjRef>,
        color_r: i32,
        color_g: i32,
        color_b: i32,
        heat: i32,
        radius: i32,
        light_direction: i32,
    ) -> Option<Rc<ArtificialLight>> {
        let me = Self::alloc(world);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        base_init_with_world_dynamic_world_at_position_cache(&me_obj, &wref, dynamic_world, pos, cache);
        {
            let mut st = me.st.borrow_mut();
            st.light_direction = light_direction;
            st.max_red = color_r;
            st.max_green = color_g;
            st.max_blue = color_b;
            st.max_heat = heat;
            st.radius = radius;
            st.diameter = radius.wrapping_shl(1);
            st.parent_object = parent_object.map(Rc::downgrade);
            let (g, a) = alloc_grids(st.diameter);
            st.contribution_grid = g;
            st.added_grid = a;
            let p = me.base.borrow().pos;
            st.contribution_grid_origin = IntPair { x: p.x.wrapping_sub(radius), y: p.y.wrapping_sub(radius) };
        }
        me.add_to_tiles();
        Some(me)
    }

    /// `-[ArtificialLight initWithWorld:dynamicWorld:saveDict:cache:parentObject:]`.
    /// Missing keys read as 0; the legacy `downlight` key overrides `lightDirection` with DOWN.
    pub fn init_with_world_dynamic_world_save_dict_cache_parent_object(
        world: &Rc<World>,
        dynamic_world: &DynamicWorldRef,
        save_dict: &PlistDict,
        cache: CacheRef,
        parent_object: Option<&ObjRef>,
    ) -> Option<Rc<ArtificialLight>> {
        let me = Self::alloc(world);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        // `base_init_with_world_dynamic_world_save_dict_cache` answers true on every path
        let _ = base_init_with_world_dynamic_world_save_dict_cache(&me_obj, &wref, dynamic_world, save_dict, cache);
        let geti = |k: &str| save_dict.get(&k.to_string()).map_or(0, |v| v.int_value());
        {
            let mut st = me.st.borrow_mut();
            st.parent_object = parent_object.map(Rc::downgrade);
            st.max_red = geti("maxRed");
            st.max_green = geti("maxGreen");
            st.max_blue = geti("maxBlue");
            st.max_heat = geti("maxHeat");
            st.radius = geti("radius");
            st.contribution_grid_origin =
                IntPair { x: geti("contributionGridOrigin.x"), y: geti("contributionGridOrigin.y") };
            st.light_direction = geti("lightDirection");
            if save_dict.get(&"downlight".to_string()).map_or(false, |v| v.bool_value()) {
                st.light_direction = LIGHT_DIRECTION_DOWN;
            }
            st.diameter = st.radius.wrapping_shl(1);
            let (g, a) = alloc_grids(st.diameter);
            st.contribution_grid = g;
            st.added_grid = a;
        }
        me.add_to_tiles();
        Some(me)
    }

    /// `-[ArtificialLight lightColor]`: `(maxRed, maxGreen, maxBlue, 1.0f)`.
    pub fn light_color(&self) -> [f32; 4] {
        let st = self.st.borrow();
        [st.max_red as f32, st.max_green as f32, st.max_blue as f32, 1.0f32]
    }

    /// `-[ArtificialLight recursivelyUpdateLightWithList:]`: one BFS step.
    pub fn recursively_update_light_with_list(&self, open_list: &mut VecDeque<u32>) {
        let world = self.world();
        let p = self.params(&world);
        let mut grid = std::mem::take(&mut self.st.borrow_mut().contribution_grid);
        bfs_step(&p, &mut grid, &world, open_list);
        self.st.borrow_mut().contribution_grid = grid;
    }

    /// `-[ArtificialLight addToTiles]`.
    pub fn add_to_tiles(&self) {
        let world = self.world();
        let p = self.params(&world);
        let (max_red, max_green, max_blue, max_heat) = {
            let st = self.st.borrow();
            (st.max_red, st.max_green, st.max_blue, st.max_heat)
        };
        // Local copies of the grids: no RefCell borrow is held across the re-entrant DynamicWorld calls below.
        let mut grid = std::mem::take(&mut self.st.borrow_mut().contribution_grid);
        let mut added = std::mem::take(&mut self.st.borrow_mut().added_grid);
        let mut open_list: VecDeque<u32> = VecDeque::new();
        let start = world_index_at_world_position(p.pos.x, p.pos.y, &world);
        open_list.push_back(start as u32);
        while !open_list.is_empty() {
            bfs_step(&p, &mut grid, &world, &mut open_list);
        }
        let macro_tiles = world.macro_tiles();
        let sv = services();
        let wt = p.wt;
        for gy in 0..p.diameter.max(0) {
            for gx in 0..p.diameter.max(0) {
                let i = (gy * p.diameter + gx) as usize;
                if added[i] == 0 && grid[i * 5] > 0 {
                    let x = p.origin.x.wrapping_add(gx);
                    let y = p.origin.y.wrapping_add(gy);
                    let Some(tile) = tile_at_world_position(x, y, &macro_tiles, &world) else { continue };
                    let fraction = (grid[i * 5] as f32) / ((p.radius.wrapping_mul(10)) as f32);
                    grid[i * 5 + 1] = f32_to_i32((max_red as f32) * fraction) as i16;
                    grid[i * 5 + 2] = f32_to_i32((max_green as f32) * fraction) as i16;
                    grid[i * 5 + 3] = f32_to_i32((max_blue as f32) * fraction) as i16;
                    grid[i * 5 + 4] = f32_to_i32((max_heat as f32) * fraction) as i16;
                    let (g1, g2, g3, g4) = (grid[i * 5 + 1], grid[i * 5 + 2], grid[i * 5 + 3], grid[i * 5 + 4]);
                    tile.update(|t| {
                        t.artificial_light_r = t.artificial_light_r.wrapping_add(g1 as u16);
                        t.artificial_light_g = t.artificial_light_g.wrapping_add(g2 as u16);
                        t.artificial_light_b = t.artificial_light_b.wrapping_add(g3 as u16);
                        t.artificial_heat = t.artificial_heat.wrapping_add(g4);
                    });
                    added[i] = 1;
                    let pos = IntPair { x, y };
                    if !tile_requires_glow_block(tile) {
                        let fg = tile.foreground_contents();
                        let gem = match fg {
                            0x34 => Some((0x33u8, 0x4b)),
                            0x36 => Some((0x35, 0x4c)),
                            0x38 => Some((0x37, 0x56)),
                            0x3a => Some((0x39, 0x57)),
                            0x3c => Some((0x3b, 0x58)),
                            _ => None,
                        };
                        if let Some((real, item)) = gem {
                            tile.set_foreground_contents(real);
                            sv.add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(
                                pos, item, 0, 0, None, None,
                            );
                        } else {
                            let c = tile.contents();
                            if matches!(c, 0x5e | 0x91 | 0x90) {
                                tile.set_contents(0);
                                if tile.type_index() == 2 {
                                    sv.create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(
                                        tile,
                                        pos,
                                        c != 0x91,
                                        c != 0x90,
                                    );
                                }
                            }
                        }
                    } else {
                        sv.load_glow_block_if_needed_at_pos_tile(pos, tile);
                    }
                    // recalculateDrawBlockLightingForTile: empty in the server binary.
                    let mut safe_x = x;
                    if safe_x < wt {
                        if safe_x < 0 {
                            safe_x = safe_x.wrapping_add(wt);
                        }
                    } else {
                        safe_x = safe_x.wrapping_sub(wt);
                    }
                    sv.light_changed_at_macro_pos_send_reliably(IntPair { x: safe_x / 32, y: y / 32 }, true);
                }
            }
        }
        let mut st = self.st.borrow_mut();
        st.contribution_grid = grid;
        st.added_grid = added;
    }

    /// `-[ArtificialLight removeFromTiles]`.
    pub fn remove_from_tiles(&self) {
        let world = self.world();
        let p = self.params(&world);
        let mut grid = std::mem::take(&mut self.st.borrow_mut().contribution_grid);
        let mut added = std::mem::take(&mut self.st.borrow_mut().added_grid);
        let macro_tiles = world.macro_tiles();
        let sv = services();
        for gy in 0..p.diameter.max(0) {
            for gx in 0..p.diameter.max(0) {
                let i = (gy * p.diameter + gx) as usize;
                if added[i] == 1 && grid[i * 5] > 0 {
                    added[i] = 0;
                    let x = p.origin.x.wrapping_add(gx);
                    let y = p.origin.y.wrapping_add(gy);
                    if let Some(tile) = tile_at_world_position(x, y, &macro_tiles, &world) {
                        let (g1, g2, g3, g4) = (grid[i * 5 + 1], grid[i * 5 + 2], grid[i * 5 + 3], grid[i * 5 + 4]);
                        tile.update(|t| {
                            t.artificial_light_r = t.artificial_light_r.wrapping_sub(g1 as u16);
                            t.artificial_light_g = t.artificial_light_g.wrapping_sub(g2 as u16);
                            t.artificial_light_b = t.artificial_light_b.wrapping_sub(g3 as u16);
                            t.artificial_heat = t.artificial_heat.wrapping_sub(g4);
                            if t.artificial_light_r > 0xdfff
                                || t.artificial_light_g > 0xdfff
                                || t.artificial_light_b > 0xdfff
                            {
                                t.artificial_light_r = 0;
                                t.artificial_light_g = 0;
                                t.artificial_light_b = 0;
                                t.artificial_heat = 0;
                            }
                        });
                        let mut safe_x = x;
                        if safe_x >= p.wt {
                            safe_x = safe_x.wrapping_sub(p.wt);
                        }
                        sv.light_changed_at_macro_pos_send_reliably(IntPair { x: safe_x / 32, y: y / 32 }, true);
                    }
                }
            }
        }
        // the grid values are NOT cleared
        let _ = &mut grid;
        let mut st = self.st.borrow_mut();
        st.contribution_grid = grid;
        st.added_grid = added;
    }

    fn recompute(&self) {
        self.remove_from_tiles();
        {
            let mut st = self.st.borrow_mut();
            for v in st.contribution_grid.iter_mut() {
                *v = 0;
            }
        }
        self.add_to_tiles();
    }

    /// `-[ArtificialLight worldChanged:]` on a plain slice (owners forward their vector).
    pub fn world_changed_positions(&self, positions: &[IntPair]) {
        let world = self.world();
        let wt = width_tiles(&world);
        let pos = self.base.borrow().pos;
        let affects = {
            let st = self.st.borrow();
            let (radius, diameter, origin) = (st.radius, st.diameter, st.contribution_grid_origin);
            let mut affects = false;
            for p in positions {
                let dx = wrapdiff(pos.x, p.x, wt);
                if -radius <= dx
                    && dx < radius
                    && -radius <= pos.y.wrapping_sub(p.y)
                    && pos.y.wrapping_sub(p.y) < radius
                {
                    let gx = p.x.wrapping_sub(origin.x);
                    let gy = p.y.wrapping_sub(origin.y);
                    if 0 < gx && gx < diameter && 0 < gy && gy < diameter {
                        let cell = (gy * diameter + gx) as usize;
                        if st.added_grid.get(cell).copied() == Some(1)
                            && st.contribution_grid.get(cell * 5).map_or(false, |v| *v > 0)
                        {
                            affects = true;
                            break;
                        }
                    }
                }
            }
            affects
        };
        if affects {
            self.recompute();
        }
    }

    /// `-[ArtificialLight addContributionForPhysicalBlockLoadedAtXPos:yPos:]`.
    pub fn add_contribution_for_physical_block_loaded_at_x_pos_y_pos(&self, macro_x: i32, macro_y: i32) {
        let world = self.world();
        let wt = width_tiles(&world);
        let pos = self.base.borrow().pos;
        let radius = self.st.borrow().radius;
        let world_min = IntPair { x: macro_x.wrapping_mul(32), y: macro_y.wrapping_mul(32) };
        let world_max =
            IntPair { x: macro_x.wrapping_add(1).wrapping_mul(32), y: macro_y.wrapping_add(1).wrapping_mul(32) };
        if -radius <= wrapdiff(pos.x, world_min.x, wt)
            && wrapdiff(pos.x, world_max.x, wt) < radius
            && -radius <= pos.y.wrapping_sub(world_min.y)
            && pos.y.wrapping_sub(world_max.y) < radius
        {
            self.recompute();
        }
    }

    /// `-[ArtificialLight removeFromMacroBlock]`: `removeFromTiles` then the base.
    pub fn remove_from_macro_block_light(&self) {
        self.remove_from_tiles();
        let me: ObjRef = self.me();
        base_remove_from_macro_block(&me);
    }

    /// `-[ArtificialLight dealloc]`: `removeFromTiles`, free the grids.
    pub fn dealloc_light(&self) {
        self.remove_from_tiles();
        let mut st = self.st.borrow_mut();
        st.contribution_grid = Vec::new();
        st.added_grid = Vec::new();
    }
}

impl ArtificialLight {
    /// `-[DynamicObject initWithWorld:dynamicWorld:cache:netData:]` inherited unchanged (ArtificialLight defines no net init; object_registry A3).
    /// Zero-state species ivars + the DynamicObject net init. `None` when the base init yields nil.
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<Rc<ArtificialLight>> {
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

impl DynamicObject for ArtificialLight {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &ARTIFICIAL_LIGHT_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_ARTIFICIAL_LIGHT
    }
    /// : super dict plus eight `numberWithInt` entries in the given order.
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = base_get_save_dict(me);
        let st = self.st.borrow();
        let mut put = |k: &str, v: i32| {
            d.map.insert(k.to_string(), PlistValue::int(v as i64));
        };
        put("maxRed", st.max_red);
        put("maxGreen", st.max_green);
        put("maxBlue", st.max_blue);
        put("maxHeat", st.max_heat);
        put("radius", st.radius);
        put("contributionGridOrigin.x", st.contribution_grid_origin.x);
        put("contributionGridOrigin.y", st.contribution_grid_origin.y);
        put("lightDirection", st.light_direction);
        d
    }
    fn world_changed(&self, _me: &ObjRef, positions: &mut Vec<IntPair>) {
        self.world_changed_positions(positions);
    }
    fn remove_from_macro_block(&self, _me: &ObjRef) {
        self.remove_from_macro_block_light();
    }
    fn add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(
        &self,
        _me: &ObjRef,
        macro_x: i32,
        macro_y: i32,
    ) {
        ArtificialLight::add_contribution_for_physical_block_loaded_at_x_pos_y_pos(self, macro_x, macro_y);
    }
    fn dealloc(&self, _me: &ObjRef) {
        self.dealloc_light();
    }
}

// =====================================================================================================
// Test kit shared by the lighting_env unit tests
// =====================================================================================================

#[cfg(test)]
pub(crate) mod testkit {
    use super::*;
    use crate::objects::base::DynamicWorldApi;
    use std::cell::{Cell, RefCell};

    /// Records every services call as a string, with scriptable return values.
    #[derive(Default)]
    pub struct Rec {
        pub log: RefCell<Vec<String>>,
        pub weather: Cell<f32>,
        pub blockheads: RefCell<Vec<ObjRef>>,
        pub view_radius: Cell<i32>,
        pub peers: Cell<usize>,
        pub has_save: Cell<bool>,
    }
    impl Rec {
        pub fn take(&self) -> Vec<String> {
            std::mem::take(&mut *self.log.borrow_mut())
        }
        fn push(&self, s: String) {
            self.log.borrow_mut().push(s);
        }
    }
    impl LightingServices for Rec {
        fn save_sunlight_changed_at_pos(&self, _w: &World, p: IntPair) {
            self.push(format!("saveSun({},{})", p.x, p.y));
        }
        fn fill_tile_at_pos_with_type(&self, _w: &World, tile: TileRef, p: IntPair, it: i32) {
            self.push(format!("fill({},{},{:#x})", p.x, p.y, it));
            // minimal model of: freeze -> ice, melt -> water, basalt -> 0x33, water -> 3
            match it {
                0x422 => tile.set_type_index(4),
                0x423 => tile.set_type_index(3),
                0x428 => tile.set_type_index(0x33),
                0x414 => tile.set_type_index(3),
                _ => {}
            }
        }
        fn get_weather_fraction_for_pos(&self, _w: &World, _p: IntPair) -> f32 {
            self.weather.get()
        }
        fn get_weather_fraction_for_pos_at_world_time(&self, _w: &World, _p: IntPair, _t: f64) -> f32 {
            self.weather.get()
        }
        fn decommision_block_block_to_save_physcial_block(&self, _w: &World, mt: &MacroTileRef, b: bool) {
            let _ = mt;
            self.push(format!("decommission({b})"));
        }
        fn peers_interested_in_macro_index_count(&self, _w: &World, _i: i32) -> usize {
            self.peers.get()
        }
        fn remove_water_tile_at_pos(&self, _w: &World, p: IntPair) {
            self.push(format!("removeWater({},{})", p.x, p.y));
        }
        fn water_moved_from_from_tile_to_to_tile_amount(
            &self,
            _w: &World,
            f: IntPair,
            _ft: TileRef,
            t: IntPair,
            _tt: TileRef,
            a: i32,
        ) {
            self.push(format!("waterMoved({},{}->{},{} {a})", f.x, f.y, t.x, t.y));
        }
        fn light_changed_at_macro_pos_send_reliably(&self, p: IntPair, r: bool) {
            self.push(format!("lightChanged({},{},{r})", p.x, p.y));
        }
        fn light_changed_at_macro_pos_send_reliably_send_at_all(&self, p: IntPair, r: bool, a: bool) {
            self.push(format!("lightChangedAll({},{},{r},{a})", p.x, p.y));
        }
        fn explore_light_changed_at_macro_pos_client_light_block_index(&self, p: IntPair, i: i32) {
            self.push(format!("exploreLight({},{},{i})", p.x, p.y));
        }
        fn snow_changed_at_macro_pos(&self, p: IntPair) {
            self.push(format!("snow({},{})", p.x, p.y));
        }
        fn water_changed_at_pos_full_block(&self, p: IntPair, f: bool) {
            self.push(format!("waterChanged({},{},{f})", p.x, p.y));
        }
        fn has_dynamic_objects_to_save_in_macro_pos(&self, _p: IntPair) -> bool {
            self.has_save.get()
        }
        fn blockheads(&self) -> Vec<ObjRef> {
            self.blockheads.borrow().clone()
        }
        fn add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(
            &self,
            p: IntPair,
            it: i32,
            a: u16,
            b: u16,
            _d: Option<&PlistDict>,
            _c: Option<&str>,
        ) {
            self.push(format!("addTorch({},{},{:#x},{a},{b})", p.x, p.y, it));
        }
        fn create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(
            &self,
            _t: TileRef,
            p: IntPair,
            troll: bool,
            treasure: bool,
        ) {
            self.push(format!("chestOrTroll({},{},{troll},{treasure})", p.x, p.y));
        }
        fn load_glow_block_if_needed_at_pos_tile(&self, p: IntPair, _t: TileRef) {
            self.push(format!("glow({},{})", p.x, p.y));
        }
        fn load_surface_block_at_pos(&self, p: IntPair) {
            self.push(format!("loadSurface({},{})", p.x, p.y));
        }
        fn create_free_block_at_position_of_type_data_a_data_b_dynamic_object_save_dict_hovers_play_sound(
            &self,
            p: IntPair,
            it: i32,
            a: u16,
            b: u16,
            _d: PlistDict,
            h: bool,
            s: bool,
        ) {
            self.push(format!("freeBlock({},{},{:#x},{a},{b},{h},{s})", p.x, p.y, it));
        }
        fn remove_standard_object(&self, _o: &ObjRef) {
            self.push("removeStandard".to_string());
        }
        fn blockhead_view_radius(&self, _b: &ObjRef) -> i32 {
            self.view_radius.get()
        }
    }

    /// `DynamicWorldApi` double.
    pub struct TestDw {
        pub next_id: Cell<u64>,
        pub changed: RefCell<Vec<(IntPair, i32)>>,
    }
    impl DynamicWorldApi for TestDw {
        fn get_next_dynamic_object_id(&self) -> u64 {
            self.next_id.set(self.next_id.get() + 1);
            self.next_id.get()
        }
        fn dynamic_world_changed_at_pos_object_type(&self, pos: IntPair, t: i32) {
            self.changed.borrow_mut().push((pos, t));
        }
        fn is_server(&self) -> bool {
            true
        }
        fn blockheads(&self) -> Vec<ObjRef> {
            Vec::new()
        }
        fn get_owner_name_for_object_owner_id(&self, _id: &str) -> Option<String> {
            None
        }
    }

    pub fn test_dw() -> (Rc<TestDw>, DynamicWorldRef) {
        let d = Rc::new(TestDw { next_id: Cell::new(0), changed: RefCell::new(Vec::new()) });
        let r: DynamicWorldRef = d.clone();
        (d, r)
    }

    /// World of `w_macro` macro tiles width with the macro blocks `(mx, my)` loaded; all tiles AIR (type 2) with
    /// back wall 2 (open sky wall) and `explored_fraction = 0`, `sun_light = 0`.
    pub fn world_with_blocks(w_macro: i32, blocks: &[(i32, i32)]) -> Rc<World> {
        let world = World::alloc(w_macro);
        for &(mx, my) in blocks {
            let pb = world.take_physical_block();
            pb.valid.set(true);
            pb.x_pos.set(mx);
            pb.y_pos.set(my);
            for i in 0..1024 {
                let t = TileRef::new(pb, i);
                t.update(|t| {
                    t.type_index = 2;
                    t.back_wall_type_index = 2;
                });
            }
            let mt = world.macro_tiles().get((my * w_macro + mx) as usize).unwrap();
            *mt.physical_block.borrow_mut() = Some(pb);
            *mt.dynamic_objects.borrow_mut() = Some(Vec::new());
        }
        world
    }

    pub fn tile(world: &World, x: i32, y: i32) -> TileRef {
        crate::world::tiles::tile_at_world_position(x, y, &world.macro_tiles(), world).expect("tile loaded")
    }

    /// Install a fresh recorder and return it.
    pub fn install_rec() -> Rc<Rec> {
        let r = Rc::new(Rec::default());
        install_services(r.clone());
        r
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;

    fn mk(world: &Rc<World>, x: i32, y: i32, r: i32, dir: i32, col: (i32, i32, i32, i32)) -> Rc<ArtificialLight> {
        let (_d, dwr) = test_dw();
        ArtificialLight::init_with_world_dynamic_world_at_position_cache_parent_object_color_r_color_g_color_b_heat_radius_light_direction(
            world, &dwr, IntPair { x, y }, None, None, col.0, col.1, col.2, col.3, r, dir,
        )
        .unwrap()
    }

    fn all_zero(world: &World, x0: i32, y0: i32, n: i32) -> bool {
        for x in x0..x0 + n {
            for y in y0..y0 + n {
                let t = tile(world, x, y).get();
                if t.artificial_light_r != 0
                    || t.artificial_light_g != 0
                    || t.artificial_light_b != 0
                    || t.artificial_heat != 0
                {
                    return false;
                }
            }
        }
        true
    }

    #[test]
    fn open_air_falloff_and_colour_scaling() {
        let rec = install_rec();
        let world = world_with_blocks(4, &[(0, 0), (1, 0), (0, 1), (1, 1)]);
        // torch (253,150,55) r=15 heat 1 at (32,32): grid origin (17,17)
        let l = mk(&world, 32, 32, 15, LIGHT_DIRECTION_ALL, (253, 150, 55, 1));
        {
            let st = l.st.borrow();
            assert_eq!(st.diameter, 30);
            let at = |gx: usize, gy: usize| st.contribution_grid[(gy * 30 + gx) * 5];
            assert_eq!(at(15, 15), 150); // radius*10
            assert_eq!(at(15, 16), 140); // above: -10
            assert_eq!(at(16, 15), 140); // right
            assert_eq!(at(16, 16), 136); // diagonal: -14
        }
        let c = tile(&world, 32, 32).get();
        // fraction 1.0: colours truncated to int16
        assert_eq!(
            (c.artificial_light_r, c.artificial_light_g, c.artificial_light_b, c.artificial_heat),
            (253, 150, 55, 1)
        );
        let n = tile(&world, 33, 32).get();
        let f = 140.0f32 / 150.0f32;
        assert_eq!(n.artificial_light_r as i32, f32_to_i32(253.0 * f));
        assert_eq!(n.artificial_light_g as i32, f32_to_i32(150.0 * f));
        assert_eq!(n.artificial_light_b as i32, f32_to_i32(55.0 * f));
        // lightChanged calls were sent reliably
        assert!(rec.take().iter().any(|s| s == "lightChanged(1,1,true)"));
        // add then remove restores all tiles
        l.remove_from_tiles();
        assert!(all_zero(&world, 17, 17, 30));
        // second add_to_tiles re-adds (addedGrid cleared by remove)
        l.add_to_tiles();
        assert_eq!(tile(&world, 32, 32).get().artificial_light_r, 253);
        l.remove_from_tiles();
        assert!(all_zero(&world, 17, 17, 30));
    }

    #[test]
    fn ice_torch_negative_heat_survives() {
        install_rec();
        let world = world_with_blocks(4, &[(0, 0), (1, 0), (0, 1), (1, 1)]);
        let l = mk(&world, 40, 40, 10, LIGHT_DIRECTION_ALL, (12, 100, 220, -4));
        let c = tile(&world, 40, 40).get();
        assert_eq!(c.artificial_heat, -4);
        let n = tile(&world, 41, 40).get();
        // 90/100 * -4 = -3.6 -> truncates toward zero -> -3
        assert_eq!(n.artificial_heat, -3);
        l.remove_from_tiles();
        assert_eq!(tile(&world, 40, 40).get().artificial_heat, 0);
        assert!(all_zero(&world, 30, 30, 20));
    }

    #[test]
    fn solid_blocks_light_and_up_down_directions() {
        install_rec();
        let world = world_with_blocks(4, &[(0, 0), (1, 0), (0, 1), (1, 1)]);
        // wall of solid tiles above (40,41)
        tile(&world, 40, 41).update(|t| t.type_index = 1);
        let l = mk(&world, 40, 40, 10, LIGHT_DIRECTION_ALL, (100, 100, 100, 0));
        let st = l.st.borrow();
        let at = |x: i32, y: i32| {
            st.contribution_grid[(((y - st.contribution_grid_origin.y) * st.diameter
                + (x - st.contribution_grid_origin.x))
                * 5) as usize]
        };
        // candidates are evaluated against the NEIGHBOUR's tile: entering the solid tile costs 10...
        assert_eq!(at(40, 41), 90);
        // ... leaving it through the top costs raw - max(10, r*10/6 = 16) = 90 - 16 = 74
        assert_eq!(at(40, 42), 74);
        assert_eq!(at(41, 40), 90);
        drop(st);
        l.remove_from_tiles();

        // DOWN light: above neighbour is blocked entirely (value 0 but still pushed via the quirk)
        let world2 = world_with_blocks(4, &[(0, 0), (1, 0), (0, 1), (1, 1)]);
        let l2 = mk(&world2, 40, 40, 10, LIGHT_DIRECTION_DOWN, (100, 100, 100, 0));
        let st = l2.st.borrow();
        let at = |x: i32, y: i32| {
            st.contribution_grid[(((y - st.contribution_grid_origin.y) * st.diameter
                + (x - st.contribution_grid_origin.x))
                * 5) as usize]
        };
        assert_eq!(at(40, 39), 90); // below is open for DOWN
        assert_eq!(at(41, 40), 67); // sides for DOWN: raw - max(20, 10*20/6 = 33)
    }

    #[test]
    fn save_dict_roundtrip_and_downlight_legacy() {
        install_rec();
        let world = world_with_blocks(4, &[(0, 0), (1, 0), (0, 1), (1, 1)]);
        let l = mk(&world, 40, 40, 6, LIGHT_DIRECTION_UP, (76, 94, 127, 0));
        let me: ObjRef = l.clone();
        let d = l.get_save_dict(&me);
        let geti = |k: &str| d.get(&k.to_string()).unwrap().int_value();
        assert_eq!(geti("maxRed"), 76);
        assert_eq!(geti("radius"), 6);
        assert_eq!(geti("contributionGridOrigin.x"), 34);
        assert_eq!(geti("contributionGridOrigin.y"), 34);
        assert_eq!(geti("lightDirection"), 2);
        let mut d2 = d.clone();
        d2.map.insert("downlight".to_string(), PlistValue::boolean(true));
        let (_dw, dwr) = test_dw();
        let l2 =
            ArtificialLight::init_with_world_dynamic_world_save_dict_cache_parent_object(&world, &dwr, &d2, None, None)
                .unwrap();
        assert_eq!(l2.st.borrow().light_direction, LIGHT_DIRECTION_DOWN);
        assert_eq!(l2.st.borrow().diameter, 12);
        // missing keys -> 0: empty light, no grid
        let empty = PlistDict { map: crate::foundation::gsdict::GsDict::new(), tag: d.tag.clone() };
        let l3 = ArtificialLight::init_with_world_dynamic_world_save_dict_cache_parent_object(
            &world, &dwr, &empty, None, None,
        );
        assert!(l3.is_none() || l3.unwrap().st.borrow().diameter == 0);
    }

    #[test]
    fn promise_gems_and_glow_blocks() {
        let rec = install_rec();
        let world = world_with_blocks(4, &[(0, 0), (1, 0), (0, 1), (1, 1)]);
        tile(&world, 41, 40).update(|t| t.foreground_contents = 0x34);
        tile(&world, 40, 41).update(|t| t.contents = 0x91);
        tile(&world, 39, 40).update(|t| t.contents = 0x5e);
        tile(&world, 40, 39).update(|t| t.type_index = 0x1a); // gold block requires glow block
        let _l = mk(&world, 40, 40, 6, LIGHT_DIRECTION_ALL, (76, 94, 127, 0));
        let log = rec.take();
        assert!(log.contains(&"addTorch(41,40,0x4b,0,0)".to_string()));
        assert_eq!(tile(&world, 41, 40).get().foreground_contents, 0x33);
        assert!(log.contains(&"chestOrTroll(40,41,false,true)".to_string())); // 0x91: no troll
        assert!(log.contains(&"chestOrTroll(39,40,true,true)".to_string())); // 0x5e
        assert_eq!(tile(&world, 40, 41).get().contents, 0);
        assert!(log.contains(&"glow(40,39)".to_string()));
    }

    #[test]
    fn world_changed_recomputes_only_for_lit_cells() {
        let rec = install_rec();
        let world = world_with_blocks(4, &[(0, 0), (1, 0), (0, 1), (1, 1)]);
        let l = mk(&world, 40, 40, 6, LIGHT_DIRECTION_ALL, (100, 100, 100, 0));
        rec.take();
        // gx == 0 cell (x = origin.x) is ignored by the strict `0 <` test
        l.world_changed_positions(&[IntPair { x: 34, y: 40 }]);
        assert!(rec.take().is_empty());
        // inside: block placed at (41,40)
        tile(&world, 41, 40).update(|t| t.type_index = 1);
        l.world_changed_positions(&[IntPair { x: 41, y: 40 }]);
        assert!(!rec.take().is_empty());
        // right neighbour is now solid: raw - max(10, 6*10/6=10) = 60 - 10 = 50 -> but row stays; check it changed
        assert!(l.st.borrow().contribution_grid[((40 - 34) * 12 + (41 - 34)) * 5] < 60);
    }

    #[test]
    fn pen_matches_spec() {
        assert_eq!(pen(12, 10, 10), 20); // 120/6
        assert_eq!(pen(3, 10, 10), 10);
        assert_eq!(pen(6, 10, 10), 10); // 60/6 = 10 < 11 -> w
    }
}
