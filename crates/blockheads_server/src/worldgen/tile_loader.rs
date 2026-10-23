//! owned by worldgen
//!
//! `WorldTileLoader`: the struct, `init` (the whole world generation with its retry loop), accessors, `dealloc`,
//! `refine_terrain` glue and the terrain queries other modules call. Also the [`WorldgenHost`] seam through which
//! this module reaches `ServerClient`, `BHServer` and `DynamicWorld` (types of higher layers that are still
//! placeholders in `world::state`
//!

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::fmath::cconv::f32_to_i32;
use crate::foundation::{nsstring, paths};
use crate::nslog;
use crate::plist::value::PlistValue;
use crate::rng::glibc_rand;
use crate::storage::lmdb::{Database, DatabaseEnvironment};
use crate::types::structs::IntPair;
use crate::world::helpers::quick_sort;
use crate::world::state::{BHServer, TileRef, World};

use super::noise::NoiseFunction;
use super::terrain::{GenParams, Terrain};

// =====================================================================================================
// Host seam
// =====================================================================================================

/// Dynamic-object net data of one macro tile as `DynamicWorld initialDynamicObjectsNetDataForMacroTileIndex:
/// wireForClient:` returns it: the `creationData` and `updateData` dictionaries as `(type, array)` pairs in
/// dictionary (GsDict) enumeration order.
#[derive(Clone, Debug, Default)]
pub struct DynamicObjectsNetData {
    /// `creationData`: `NSNumber type -> NSArray` entries.
    pub creation: Vec<(u32, PlistValue)>,
    /// `updateData`: `NSNumber type -> NSArray` entries.
    pub update: Vec<(u32, PlistValue)>,
}

/// What the worldgen module needs from `ServerClient`, `BHServer`, `DynamicWorld` and `World` actions of higher
/// layers. The owners
/// of those modules implement this once and install it with [`set_host`]; until then every query answers "no
/// such client / nothing to do". All methods take the world explicitly (the original messages `self->world`).
pub trait WorldgenHost {
    /// `[[world serverClients] objectForKey:id]` reduced to `(connected, lightBlockIndex)`; `None` == nil.
    fn client_info(&self, _world: &World, _client_id: &str) -> Option<(bool, i32)> {
        None
    }
    /// `[sc getAndRemoveAllRecieptDataForMacroPos:pos world:world]` (array of unsigned ints, `None` == nil).
    fn client_receipts(&self, _world: &World, _client_id: &str, _pos: IntPair) -> Option<Vec<u32>> {
        None
    }
    /// `[[sc allLightBlockIndices] containsIndex:i]`.
    fn client_contains_light_block_index(&self, _world: &World, _client_id: &str, _index: u64) -> bool {
        false
    }
    /// `[[sc allLightBlockIndices] addIndex:i]`.
    fn client_add_light_block_index(&self, _world: &World, _client_id: &str, _index: u64) {}
    /// `[sc saveLightBlockIndices]`.
    fn client_save_light_block_indices(&self, _world: &World, _client_id: &str) {}
    /// `[server sendNetworkData:data toPeers:peers reliable:reliable]`.
    fn send_network_data_to_peers_reliable(
        &self,
        _server: &Rc<BHServer>,
        _data: &[u8],
        _peers: &[String],
        _reliable: bool,
    ) {
    }
    /// `[server playerIsAdminWithID:id]`.
    fn player_is_admin_with_id(&self, _server: &Rc<BHServer>, _client_id: &str) -> bool {
        false
    }
    /// `[[world dynamicWorld] initialDynamicObjectsNetDataForMacroTileIndex:wireForClient:]`.
    fn initial_dynamic_objects_net_data(
        &self,
        _world: &World,
        _macro_index: i32,
        _client_id: &str,
    ) -> Option<DynamicObjectsNetData> {
        None
    }
    /// `[[world dynamicWorld] createTreasureChestOrTrollAtTile:atPos:loadTroll:loadTreasure:]`.
    fn create_treasure_chest_or_troll_at_tile(
        &self,
        _world: &World,
        _tile: TileRef,
        _pos: IntPair,
        _load_troll: bool,
        _load_treasure: bool,
    ) {
    }
    /// `[world decommisionAllBlocksBlockToSavePhyscialBlock:YES]`.
    fn decommision_all_blocks(&self, _world: &World) {}
    /// `[world fullyLoadIfNeededAroundPos:pos clientLightBlockIndex:i forBlockhead:nil]`.
    fn fully_load_if_needed_around_pos(&self, _world: &World, _pos: IntPair, _client_light_block_index: i32) {}
}

/// The default host: no clients, no server, no dynamic world.
pub struct NullHost;
impl WorldgenHost for NullHost {}

thread_local! {
    static HOST: RefCell<Option<Rc<dyn WorldgenHost>>> = const { RefCell::new(None) };
}

/// Installs (or removes) the process-wide [`WorldgenHost`] (single threaded, ).
pub fn set_host(h: Option<Rc<dyn WorldgenHost>>) {
    HOST.with(|c| *c.borrow_mut() = h);
}

/// The installed host or [`NullHost`].
pub fn host() -> Rc<dyn WorldgenHost> {
    HOST.with(|c| c.borrow().clone()).unwrap_or_else(|| Rc::new(NullHost))
}

// =====================================================================================================
// WorldTileLoader
// =====================================================================================================

/// `WorldTileLoader` (C: 0x168 bytes). All generation data lives in the immutable [`Terrain`] (
/// item 1); only the counters are `Cell`s.
pub struct WorldTileLoader {
    pub(crate) world: Weak<World>,
    pub(crate) t: Terrain,
    pub(crate) refine_terrain_count: Cell<i32>,
    pub(crate) has_refined_terrain: Cell<bool>,
    pub(crate) needs_to_exit: Cell<bool>,
    pub(crate) block_directory: String,
    /// Released first in `dealloc` (its drop commits a pending bulk transaction), then the environment.
    pub(crate) light_block_database: RefCell<Option<Rc<Database>>>,
    pub(crate) light_block_database_environment: RefCell<Option<Rc<DatabaseEnvironment>>>,
    pub(crate) block_database: Option<Rc<Database>>,
}

/// Mutable lake detection state of Step C (`reset()` keeps `lake_start` / `lake_end`).
struct Lake {
    count: i32,
    height: i32,
    start: i32,
    end: i32,
    down_count: i32,
    validating: bool,
}

impl Lake {
    fn new() -> Lake {
        Lake { count: 0, height: -1, start: 0, end: 0, down_count: 0, validating: false }
    }
    fn reset(&mut self) {
        self.count = 0;
        self.height = -1;
        self.down_count = 0;
        self.validating = false;
    }
}

/// `max(a, b)` as `if (a < b) a = b`.
#[inline]
fn imax(a: i32, b: i32) -> i32 {
    if a < b {
        b
    } else {
        a
    }
}

impl WorldTileLoader {
    /// -[WorldTileLoader initWithWorld:randomSeed:isNewWorld:saveID:loadedVersion:blockDatabase:].
    /// Runs the whole generation (including retries with `srand(seed + 1)`) synchronously. `loaded_version` is
    /// ignored, as in the original.
    pub fn init_with_world_random_seed_is_new_world_save_id_loaded_version_block_database(
        world: &Rc<World>,
        random_seed: u32,
        is_new_world: bool,
        save_id: &str,
        _loaded_version: i32,
        block_database: Option<Rc<Database>>,
    ) -> Rc<WorldTileLoader> {
        let w0 = world.world_width_macro();
        let mut t = Terrain::new_empty(w0, random_seed);

        // ---- directories / light block DB
        let appsup = paths::appsup();
        let block_directory = format!("{}/saves/{}/blocks/", appsup, save_id);
        let database_path = format!("{}/saves/{}/lightBlocks/", appsup, save_id);
        let mut env: Option<Rc<DatabaseEnvironment>> = None;
        let mut light_db: Option<Rc<Database>> = None;
        if world.server().is_some() {
            env = DatabaseEnvironment::new(&database_path, 1, 0x400);
            if let Some(e) = &env {
                light_db = Database::new(e, "main").map(Rc::new);
            }
            if light_db.is_none() {
                nslog!("Failed to load lightBlockDatabase.");
            }
        }

        // ---- legacy light-block file migration ("1.5.1")
        Self::migrate_legacy_light_block_files(&block_directory);

        // ---- GEN loop (retry until a valid world)
        loop {
            let p = GenParams::from_world(world);
            Self::step_a_noise(&mut t, &p);
            Self::step_b_c_heights_and_lakes(&mut t, &p);

            // Step D
            if !is_new_world {
                // bestStartPosition stays (0,0) from zeroed memory
            } else {
                t.best_start_position = t.find_best_start_position(&p, Some(world));
                if t.best_start_position.y == -1 || t.best_start_position.x == -1 {
                    nslog!("no start position found, trying with new random seed.");
                    t.random_seed = t.random_seed.wrapping_add(1);
                    glibc_rand::srand_tagged("worldgen::init_retry_no_start", t.random_seed);
                    t.height_a = None;
                    t.height_b = None;
                    t.fault = None;
                    t.sand = None;
                    t.cave_a = None;
                    t.cave_b = None;
                    continue;
                }
            }

            Self::step_e_noise(&mut t, &p);
            let food = t.generate_spawn_tables(&p, Some(world));
            t.compute_highest_point(&p);

            // Step H
            if is_new_world {
                let mut distance_values = [0.0f32; 31];
                let mut above_zero_count = 0i32;
                let mut total = 0i32;
                for i in 0..31 {
                    distance_values[i] = food[i] as f32;
                    if distance_values[i] > 0.0f32 {
                        above_zero_count += 1;
                    }
                    total = f32_to_i32((total as f32) + distance_values[i]);
                }
                nslog!("aboveZeroCount:{} total:{}", above_zero_count, total);
                let r = &p.rules;
                if r.oceans == 2
                    && r.temperature == 2
                    && r.rainfall == 2
                    && r.trees != 0
                    && r.trees != 1
                    && r.trees != 2
                    && r.trees != 3
                    && r.plants != 0
                    && r.plants != 1
                    && (above_zero_count < 6 || total < 10000)
                {
                    nslog!("not enough food found near by. Trying with new random seed.");
                    t.height_a = None;
                    t.height_b = None;
                    t.fault = None;
                    t.sand = None;
                    t.cave_a = None;
                    t.cave_b = None;
                    t.tree_density = None;
                    t.rock_type = None;
                    t.flint = None;
                    t.tin = None;
                    t.season_offset = None;
                    t.gem = None;
                    t.random_seed = t.random_seed.wrapping_add(1);
                    glibc_rand::srand_tagged("worldgen::init_retry_no_food", t.random_seed);
                    continue;
                }
                let mut ordered = [0u32; 31];
                quick_sort(&mut distance_values, 31, &mut ordered);
                for i in 0..31 {
                    t.distance_ordered_food_types[i] = ordered[30 - i] as i32;
                }
            }
            break;
        }

        // Step I: `compressBlocks` (empty method on a detached thread) is omitted.
        Rc::new(WorldTileLoader {
            world: Rc::downgrade(world),
            t,
            refine_terrain_count: Cell::new(0),
            has_refined_terrain: Cell::new(false),
            needs_to_exit: Cell::new(false),
            block_directory,
            light_block_database: RefCell::new(light_db),
            light_block_database_environment: RefCell::new(env),
            block_database,
        })
    }

    /// Legacy 1.5.1 light block upgrade: every 32-character file name in the block directory that does not contain
    /// "ock" moves to `playerLightBlocks/<c0>/<c1>/<rest>`.
    fn migrate_legacy_light_block_files(block_directory: &str) {
        let mut upgraded = false;
        let names: Vec<String> = match std::fs::read_dir(block_directory) {
            Ok(rd) => rd.filter_map(|e| e.ok()).filter_map(|e| e.file_name().into_string().ok()).collect(),
            Err(_) => Vec::new(),
        };
        for file_name in names {
            if nsstring::length(&file_name) == 0x20 && nsstring::range_of_string(&file_name, "ock").is_none() {
                if !upgraded {
                    upgraded = true;
                    nslog!("Upgrading player light blocks to 1.5.1...");
                }
                let len = nsstring::length(&file_name);
                let a = nsstring::substring_with_range(&file_name, 0, 1);
                let b = nsstring::substring_with_range(&file_name, 1, 1);
                let c = nsstring::substring_with_range(&file_name, 2, len - 2);
                let sub = format!("playerLightBlocks/{}/{}/{}/", a, b, c);
                let from = paths::append_path_component(block_directory, &file_name);
                let to = paths::append_path_component(block_directory, &sub);
                let parent = std::path::Path::new(&to).parent().map(|p| p.to_path_buf());
                if let Some(parent) = parent {
                    let _ = std::fs::create_dir_all(parent);
                }
                // NSFileManager refuses to move onto an existing destination (std::fs::rename would replace it);
                // that failure removes the source, as does any other move failure.
                let moved = !std::path::Path::new(&to).exists() && std::fs::rename(&from, &to).is_ok();
                if !moved {
                    let pf = std::path::Path::new(&from);
                    if pf.is_dir() {
                        let _ = std::fs::remove_dir_all(pf);
                    } else {
                        let _ = std::fs::remove_file(pf);
                    }
                }
            }
        }
        if upgraded {
            nslog!("Finished upgrading light blocks.");
        }
    }

    /// Step A: cave / height / sand / fault noise functions (tables in).
    fn step_a_noise(t: &mut Terrain, p: &GenParams) {
        let x = t.x_frequency_multiplier;
        let s = t.random_seed as i32;
        let cave_off = if p.rules.caves == 2 { 2 } else { 0 };
        // A8: tileable form; A9: tileableX / tileableY form.
        let a8 = |fx: i32, fy: i32, fz: i32, amp: f64, seed: i32, tileable: bool, loop_: bool, pers: f64| {
            Some(Rc::new(
                NoiseFunction::init_with_frequency_x_frequency_y_frequency_z_amplitude_seed_tileable_loop_persistance(
                    fx, fy, fz, amp, seed, tileable, loop_, pers,
                ),
            ))
        };
        let a9 = |fx: i32, fy: i32, fz: i32, amp: f64, seed: i32, tx: bool, ty: bool, loop_: bool, pers: f64| {
            Some(Rc::new(
                NoiseFunction::init_with_frequency_x_frequency_y_frequency_z_amplitude_seed_tileable_x_tileable_y_loop_persistance(
                    fx, fy, fz, amp, seed, tx, ty, loop_, pers,
                ),
            ))
        };
        if p.w < 0x200 {
            t.height_a = a8(x + 3, 3, 2, 0.9, s.wrapping_add(1), true, false, 1.2);
            t.height_b = a8(x + 4, 4, 4, 0.9, s.wrapping_add(3), true, false, 1.2);
            t.sand = a8(x + 4, 1, 1, 1.0, s.wrapping_add(9), true, false, 1.5);
            t.fault = a9(x + 8, 8, 4, 0.9, s.wrapping_add(0x49), true, false, false, 1.4);
            t.cave_a = a9(x + 6 + cave_off, cave_off + 3, 1, 1.0, s.wrapping_add(8), true, false, false, 3.5);
            t.cave_b = a9(x + 0xc + cave_off, cave_off + 8, 1, 1.0, s.wrapping_add(9), true, false, false, 1.5);
        } else {
            t.height_a = a8(x + 3, x + 3, 2, 0.9, s.wrapping_add(1), true, false, 1.2);
            t.height_b = a8(x + 4, x + 4, 4, 0.9, s.wrapping_add(3), true, false, 1.2);
            t.sand = a8(x + 4, x + 1, 1, 1.0, s.wrapping_add(9), true, false, 1.5);
            t.fault = a8(x + 8, x + 8, 4, 0.9, s.wrapping_add(0x49), true, false, 1.4);
            t.cave_a = a8(x + 6 + cave_off, x + 3 + cave_off, 1, 1.0, s.wrapping_add(8), true, false, 3.5);
            t.cave_b = a8(x + 0xc + cave_off, x + 8 + cave_off, 1, 1.0, s.wrapping_add(9), true, false, 1.5);
        }
    }

    /// Steps B and C: raw float heights, normalisation, int heights and lakes.
    fn step_b_c_heights_and_lakes(t: &mut Terrain, p: &GenParams) {
        let wt = p.wt();
        let n = wt.max(0) as usize;
        let r = p.rules;
        let mut dirt_f = vec![0.0f32; n];
        let mut rock_f = vec![0.0f32; n];
        for v in t.lake_heights.iter_mut() {
            *v = 0;
        }
        if t.lake_heights.len() != n {
            t.lake_heights = vec![0; n];
        }
        if t.rock_heights.len() != n {
            t.rock_heights = vec![0; n];
            t.dirt_heights = vec![0; n];
        }
        let mut max_height = 0i32;
        let mut min_height = 0x400i32;
        for x in 0..n {
            let (rk, dt) = t.get_initial_rock_and_dirt_height(p, x as i32);
            rock_f[x] = rk;
            dirt_f[x] = dt;
            if (max_height as f32) < rock_f[x] {
                max_height = f32_to_i32(rock_f[x]);
            }
            if rock_f[x] < (min_height as f32) {
                min_height = f32_to_i32(rock_f[x]);
            }
        }
        let mut above = ((max_height as f32) - 512.0f32) / 256.0f32;
        let mut below = -((min_height as f32) - 512.0f32) / 256.0f32;
        if r.oceans == 4 || r.terrain_type == 0 || r.terrain_type == 1 {
            above = 1.0f32;
        } else if r.oceans == 3 {
            above = above * 0.5f32 + 0.5f32;
        } else if r.terrain_type == 3 {
            above = above / 1.5f32;
        }
        if r.oceans == 0 || r.terrain_type == 0 || r.terrain_type == 1 {
            below = 1.0f32;
        } else if r.oceans == 1 {
            below = below * 0.5f32 + 0.5f32;
        }

        // Step C
        let loops = if r.lakes == 2 { 8 } else { 1 };
        let mut max_lake_size: i32 = if r.lakes == 2 { 0x200 } else { 0x100 };
        let norm = |v: f32| -> i32 {
            f32_to_i32(if v <= 512.0f32 {
                (v - 512.0f32) / below + 512.0f32
            } else {
                (v - 512.0f32) / above + 512.0f32
            })
        };
        for lp in 0..loops {
            let mut lk = Lake::new();
            // ---- forward pass
            for x in 0..n {
                if lp == 0 {
                    t.rock_heights[x] = norm(rock_f[x]);
                    t.dirt_heights[x] = norm(dirt_f[x]);
                }
                let xi = x as i32;
                let max_h = imax(t.rock_heights[x], t.dirt_heights[x]);
                if t.lake_heights[x] != 0 {
                    lk.reset();
                    continue;
                }
                if r.lakes == 0 || x < 1 || max_h <= 0x200 {
                    lk.reset();
                    continue;
                }
                if t.dirt_heights[x] <= t.rock_heights[x] {
                    let fo = t.fault_offset(p, xi, max_h - 1);
                    if t.is_cave(p, xi, max_h - 1, fo) {
                        lk.reset();
                        continue;
                    }
                }
                if lk.count < 1 && !lk.validating {
                    let prev_max = imax(t.rock_heights[x - 1], t.dirt_heights[x - 1]);
                    if max_h < prev_max {
                        Self::lake_descent(t, p, &mut lk, xi, max_h);
                    } else {
                        lk.down_count = 0;
                        lk.validating = false;
                    }
                } else {
                    let is_desert =
                        if r.lakes != 2 { t.is_desert(p, IntPair { x: xi, y: max_h }, max_h) } else { false };
                    if max_h < lk.height {
                        lk.count += 1;
                        if lk.count > max_lake_size || lk.validating || is_desert {
                            lk.reset();
                        }
                    } else if !lk.validating {
                        lk.validating = true;
                        lk.end = xi;
                    } else if lk.height + 2 < max_h {
                        for lx in lk.start..lk.end {
                            t.lake_heights[lx as usize] = lk.height;
                        }
                        lk.reset();
                    }
                }
            }
            // ---- reverse pass (only if R.lakes != 0)
            if r.lakes != 0 {
                lk.reset();
                lk.start = 0;
                lk.end = 0;
                let mut xi = wt - 2;
                while xi > 1 {
                    let x = xi as usize;
                    let max_h = imax(t.rock_heights[x], t.dirt_heights[x]);
                    if t.lake_heights[x] != 0 {
                        lk.reset();
                        xi -= 1;
                        continue;
                    }
                    if r.lakes == 0 || max_h <= 0x200 {
                        lk.reset();
                        xi -= 1;
                        continue;
                    }
                    if t.dirt_heights[x] <= t.rock_heights[x] {
                        let fo = t.fault_offset(p, xi, max_h - 1);
                        if t.is_cave(p, xi, max_h - 1, fo) {
                            lk.reset();
                            xi -= 1;
                            continue;
                        }
                    }
                    if lk.count < 1 && !lk.validating {
                        let next_max = imax(t.rock_heights[x + 1], t.dirt_heights[x + 1]);
                        if max_h < next_max {
                            Self::lake_descent(t, p, &mut lk, xi, max_h);
                        } else {
                            lk.down_count = 0;
                            lk.validating = false;
                        }
                    } else {
                        let is_desert =
                            if r.lakes != 2 { t.is_desert(p, IntPair { x: xi, y: max_h }, max_h) } else { false };
                        if max_h < lk.height {
                            lk.count += 1;
                            if lk.count > max_lake_size || lk.validating || is_desert {
                                lk.reset();
                            }
                        } else if !lk.validating {
                            lk.validating = true;
                            lk.end = xi;
                        } else if lk.height + 2 < max_h {
                            let mut lx = lk.end + 1;
                            while lx <= lk.start {
                                t.lake_heights[lx as usize] = lk.height;
                                lx += 1;
                            }
                            lk.reset();
                        }
                    }
                    xi -= 1;
                }
            }
            max_lake_size /= 2;
        }
    }

    /// The "descent" branch shared by the forward and reverse lake passes: count the descent, draw `rand()` only when
    /// `downCount > 1`, and open a lake on success.
    fn lake_descent(t: &Terrain, p: &GenParams, lk: &mut Lake, x: i32, max_h: i32) {
        lk.down_count += 1;
        let mut ok = false;
        if lk.down_count > 1 {
            let rv = glibc_rand::r01_tagged("worldgen::lake_start");
            let thr = if p.rules.lakes == 2 { 0.5f32 } else { 0.97f32 };
            ok = false;
            if thr < rv {
                ok = true;
                if p.rules.lakes != 2 {
                    ok = !t.is_desert(p, IntPair { x, y: max_h }, max_h);
                }
            }
        }
        if ok {
            lk.height = max_h + 1;
            lk.count = 1;
            lk.start = x;
            lk.down_count = 0;
            lk.validating = false;
        }
    }

    /// Step E: tree density, rock type, flint, tin, season and gem noise functions.
    fn step_e_noise(t: &mut Terrain, p: &GenParams) {
        let x = t.x_frequency_multiplier;
        let s = t.random_seed as i32;
        let ore_off = if p.rules.ore_distribution == 2 {
            -1
        } else if p.rules.ore_distribution == 0 {
            1
        } else {
            0
        };
        let mk = |fx: i32, fy: i32, seed: i32, tileable: bool| {
            Some(Rc::new(
                NoiseFunction::init_with_frequency_x_frequency_y_frequency_z_amplitude_seed_tileable_loop_persistance(
                    fx, fy, 1, 1.0, seed, tileable, false, 1.6,
                ),
            ))
        };
        if p.w < 0x200 {
            t.tree_density = mk(x + 7, 7, s.wrapping_add(4), true);
            t.rock_type = mk(x + 8, 8, s.wrapping_add(9), true);
            t.flint = mk((ore_off + 10) + x, ore_off + 10, s.wrapping_add(5), true);
            t.tin = mk((ore_off + 7) + x, ore_off + 5, s.wrapping_add(8), false);
            t.season_offset = mk(x + 0xb, 0xb, s.wrapping_add(7), false);
            t.gem = mk(x + 10, 10, s.wrapping_add(0xe), false);
        } else {
            t.tree_density = mk(x + 7, x + 7, s.wrapping_add(4), true);
            t.rock_type = mk(x + 8, x + 8, s.wrapping_add(9), true);
            t.flint = mk((ore_off + 10) + x, (ore_off + 10) + x, s.wrapping_add(5), true);
            t.tin = mk((ore_off + 7) + x, (ore_off + 5) + x, s.wrapping_add(8), false);
            t.season_offset = mk(x + 0xb, x + 0xb, s.wrapping_add(7), false);
            t.gem = mk(x + 10, x + 10, s.wrapping_add(0xe), false);
        }
    }

    // ------------------------------------------------------------------------------------------------
    // dealloc / refine
    // ------------------------------------------------------------------------------------------------

    /// -[WorldTileLoader dealloc]: the light DB is released first (its drop commits a pending bulk transaction),
    /// then the environment. The remaining ivars drop with the struct.
    pub fn dealloc(&self) {
        self.light_block_database.borrow_mut().take();
        self.light_block_database_environment.borrow_mut().take();
    }

    /// -[WorldTileLoader refineTerrain]: one 1/8-width slice (World calls it 8 times for a new world, stages
    pub fn refine_terrain(&self) {
        let world = match self.world.upgrade() {
            Some(w) => w,
            None => return,
        };
        let p = GenParams::from_world(&world);
        let start = self.refine_terrain_count.get();
        self.t.refine_terrain_slice(&p, &world, start);
        let stride = p.wt() / 8;
        self.refine_terrain_count.set(start + stride);
        host().decommision_all_blocks(&world);
        // The original also tests `count == WT - 1` and then sets `hasRefinedTerrain`: never true, because `count`
        // only takes multiples of WT/8 (a multiple of 4 since WT is a multiple of 32) and WT - 1 is 3 mod 4. The
        // flag is not read anywhere.
    }

    // ------------------------------------------------------------------------------------------------
    // accessors
    // ------------------------------------------------------------------------------------------------

    /// The immutable generator data.
    pub fn terrain(&self) -> &Terrain {
        &self.t
    }
    /// `randomSeed`; may be larger than the seed passed to `init` after retries.
    pub fn random_seed(&self) -> u32 {
        self.t.random_seed
    }
    /// `bestStartPosition` ((0,0) for loaded worlds).
    pub fn best_start_position(&self) -> IntPair {
        self.t.best_start_position
    }
    /// `highestPoint`.
    pub fn highest_point(&self) -> IntPair {
        self.t.highest_point
    }
    /// `treeDensityNoiseFunction`.
    pub fn tree_density_noise_function(&self) -> Option<Rc<NoiseFunction>> {
        self.t.tree_density.clone()
    }
    /// `seasonOffsetNoiseFunction`.
    pub fn season_offset_noise_function(&self) -> Option<Rc<NoiseFunction>> {
        self.t.season_offset.clone()
    }
    /// `treePositions` (TreeType per column).
    pub fn tree_positions(&self) -> &[i32] {
        &self.t.tree_positions
    }
    /// `npcPositions` (NPCType per column).
    pub fn npc_positions(&self) -> &[i32] {
        &self.t.npc_positions
    }
    /// `plantPositions` (PlantType per column).
    pub fn plant_positions(&self) -> &[i32] {
        &self.t.plant_positions
    }
    /// `distanceOrderedFoodTypes` (FoodType, best first; all zero for loaded worlds).
    pub fn distance_ordered_food_types(&self) -> &[i32; 31] {
        &self.t.distance_ordered_food_types
    }
    /// `xFrequencyMultiplier`.
    pub fn x_frequency_multiplier(&self) -> i32 {
        self.t.x_frequency_multiplier
    }
    /// `yHeightDivider`.
    pub fn y_height_divider(&self) -> f32 {
        self.t.y_height_divider
    }
    /// `needsToExit`.
    pub fn needs_to_exit(&self) -> bool {
        self.needs_to_exit.get()
    }
    /// `setNeedsToExit:`.
    pub fn set_needs_to_exit(&self, v: bool) {
        self.needs_to_exit.set(v);
    }
    /// `lightBlockDatabase`.
    pub fn light_block_database(&self) -> Option<Rc<Database>> {
        self.light_block_database.borrow().clone()
    }
    /// `blockDirectory` (legacy block files).
    pub fn block_directory(&self) -> &str {
        &self.block_directory
    }
    /// The owning world, if still alive.
    pub fn world(&self) -> Option<Rc<World>> {
        self.world.upgrade()
    }

    /// Fresh per-operation parameters.
    pub(crate) fn gen_params(&self) -> GenParams {
        let w = self.world.upgrade();
        GenParams::from_world_opt(w.as_deref())
    }

    // ------------------------------------------------------------------------------------------------
    // terrain queries
    // ------------------------------------------------------------------------------------------------

    /// `-unmodifiedGroundLevelForX:`.
    pub fn unmodified_ground_level_for_x(&self, x: i32) -> i32 {
        self.t.unmodified_ground_level(&self.gen_params(), x)
    }
    /// `-maxOfRockAndDirtHeightForX:`.
    pub fn max_of_rock_and_dirt_height_for_x(&self, x: i32) -> i32 {
        self.t.max_of_rock_and_dirt_height(&self.gen_params(), x)
    }
    /// `-lakeHeightForX:`.
    pub fn lake_height_for_x(&self, x: i32) -> i32 {
        self.t.lake_height(&self.gen_params(), x)
    }
    /// `-getRockAndDirtHeightforX:rockHeight:dirtHeight:` -> `(rock, dirt)`.
    pub fn get_rock_and_dirt_height_for_x_rock_height_dirt_height(&self, x: i32) -> (i32, i32) {
        self.t.get_rock_and_dirt_height(&self.gen_params(), x)
    }
    /// `-getInitialRockAndDirtHeightforX:rockHeight:dirtHeight:` -> `(rock, dirt)` floats.
    pub fn get_initial_rock_and_dirt_height_for_x_rock_height_dirt_height(&self, x: i32) -> (f32, f32) {
        self.t.get_initial_rock_and_dirt_height(&self.gen_params(), x)
    }
    /// `-sandFractionForPos:highRes:`.
    pub fn sand_fraction_for_pos_high_res(&self, pos: IntPair, high_res: bool) -> f32 {
        self.t.sand_fraction_for_pos(&self.gen_params(), pos, high_res)
    }
    /// `-sandFractionForPos:height:highRes:`.
    pub fn sand_fraction_for_pos_height_high_res(&self, pos: IntPair, height: i32, high_res: bool) -> f32 {
        self.t.sand_fraction_for_pos_height(&self.gen_params(), pos, height, high_res)
    }
    /// `-isCaveForX:y:faultOffset:`.
    pub fn is_cave_for_x_y_fault_offset(&self, x: i32, y: i32, fo: i32) -> bool {
        self.t.is_cave(&self.gen_params(), x, y, fo)
    }
    /// `-isFloatingIslandCaveForX:y:`.
    pub fn is_floating_island_cave_for_x_y(&self, x: i32, y: i32) -> bool {
        self.t.is_floating_island_cave(&self.gen_params(), x, y)
    }
    /// `-faultOffsetForX:y:`.
    pub fn fault_offset_for_x_y(&self, x: i32, y: i32) -> i32 {
        self.t.fault_offset(&self.gen_params(), x, y)
    }
    /// `-getCloudHeightForX:`.
    pub fn get_cloud_height_for_x(&self, x: i32) -> i32 {
        self.t.get_cloud_height(&self.gen_params(), x)
    }
    /// `-limestoneFractionForX:y:faultOffset:`.
    pub fn limestone_fraction_for_x_y_fault_offset(&self, x: i32, y: i32, fo: i32) -> f32 {
        self.t.limestone_fraction(&self.gen_params(), x, y, fo)
    }
    /// `-sandstoneFractionForX:y:faultOffset:limestoneFraction:`.
    pub fn sandstone_fraction_for_x_y_fault_offset_limestone_fraction(&self, x: i32, y: i32, fo: i32, lf: f32) -> f32 {
        self.t.sandstone_fraction(&self.gen_params(), x, y, fo, lf)
    }
    /// `-isDesertForPos:height:`.
    pub fn is_desert_for_pos_height(&self, pos: IntPair, height: i32) -> bool {
        self.t.is_desert(&self.gen_params(), pos, height)
    }
    /// `-isBeachForPos:height:`.
    pub fn is_beach_for_pos_height(&self, pos: IntPair, height: i32) -> bool {
        self.t.is_beach(&self.gen_params(), pos, height)
    }
    /// `-isDesertOrBeachForPos:height:`.
    pub fn is_desert_or_beach_for_pos_height(&self, pos: IntPair, height: i32) -> bool {
        self.t.is_desert_or_beach(&self.gen_params(), pos, height)
    }
    /// `-recursivelyFlowOutWaterFromTile:atPos:`.
    pub fn recursively_flow_out_water_from_tile_at_pos(&self, wp: IntPair) {
        if let Some(world) = self.world.upgrade() {
            self.t.recursively_flow_out_water_from_tile(&world, wp);
        }
    }
    /// `-fillDirtTile:worldPos:worldDirtHeight:parentType:`.
    pub fn fill_dirt_tile_world_pos_world_dirt_height_parent_type(
        &self,
        tile: &TileRef,
        wp: IntPair,
        world_dirt_height: i32,
        parent_type: u8,
    ) {
        let p = self.gen_params();
        tile.update(|t| self.t.fill_dirt_tile(&p, t, wp, world_dirt_height, parent_type));
    }
    /// `-placeGemsInCaveForPhysicalBlock:tileIndex:worldX:worldY:floatingIslandType:`.
    pub fn place_gems_in_cave_for_physical_block_tile_index_world_x_world_y_floating_island_type(
        &self,
        pb: crate::world::state::PhysicalBlockRef,
        ti: usize,
        wx: i32,
        wy: i32,
        isl: u32,
    ) {
        let p = self.gen_params();
        self.t.place_gems_in_cave_for_physical_block_tile_index_world_x_world_y_floating_island_type(
            &p, pb, ti, wx, wy, isl,
        );
    }
    /// `-findBestStartPosition`.
    pub fn find_best_start_position(&self) -> IntPair {
        let w = self.world.upgrade();
        self.t.find_best_start_position(&GenParams::from_world_opt(w.as_deref()), w.as_deref())
    }
}

impl Drop for WorldTileLoader {
    fn drop(&mut self) {
        // : light DB first, then the environment.
        self.dealloc();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::custom_rules::default_custom_rules;

    fn world_with(w: i32, enabled: bool) -> Rc<World> {
        let world = World::alloc(w);
        let mut r = default_custom_rules(false);
        r.custom_rules_enabled = enabled as u8;
        world.set_custom_rules(r);
        world.set_expert_mode(false);
        world
    }

    ///  table, sample seedA: custom seed 12345, W = 512, default rules with
    /// `customRulesEnabled = 1`: first attempt accepted (aboveZeroCount 10, total 31931), final seed 12345,
    /// best start pos 5900.0 524.0.
    #[test]
    fn seed_a_start_position_and_seed() {
        let world = world_with(512, true);
        glibc_rand::srand(12345);
        let l = WorldTileLoader::init_with_world_random_seed_is_new_world_save_id_loaded_version_block_database(
            &world,
            12345,
            true,
            "wgtest_seed_a",
            0,
            None,
        );
        assert_eq!(l.random_seed(), 12345);
        assert_eq!(l.best_start_position(), IntPair { x: 5900, y: 524 });
        assert_eq!(l.x_frequency_multiplier(), 0);
        assert_eq!(l.y_height_divider(), 32.0);
        assert_eq!(l.tree_positions().len(), 16384);
        // 31 distinct food types in the ordering
        let mut seen = [false; 31];
        for &f in l.distance_ordered_food_types() {
            seen[f as usize] = true;
        }
        assert!(seen.iter().all(|&b| b));
    }

    ///  table: final seed and `best start pos` of every sample (retry seeds increment by 1).
    #[test]
    fn sample_start_positions_and_retry_seeds() {
        // (width, first seed, rules, expert, expected final seed, expected start)
        let cases: Vec<(i32, u32, crate::types::structs::CustomRules, bool, u32, IntPair)> = vec![
            (512, 1792182285, default_custom_rules(false), false, 1792182285, IntPair { x: 6350, y: 579 }),
            (512, 1792182285, default_custom_rules(true), true, 1792182285, IntPair { x: 6350, y: 579 }),
            (512, 1792183669, default_custom_rules(false), false, 1792183669, IntPair { x: 6117, y: 524 }),
            (32, 1792182285, default_custom_rules(false), false, 1792182287, IntPair { x: 312, y: 524 }),
            (2048, 1792182285, default_custom_rules(false), false, 1792182286, IntPair { x: 56862, y: 519 }),
        ];
        for (w, seed, rules, expert, want_seed, want_pos) in cases {
            let world = World::alloc(w);
            world.set_custom_rules(rules);
            world.set_expert_mode(expert);
            glibc_rand::srand(seed);
            let l = WorldTileLoader::init_with_world_random_seed_is_new_world_save_id_loaded_version_block_database(
                &world,
                seed,
                true,
                "wgtest_table",
                0,
                None,
            );
            assert_eq!(l.random_seed(), want_seed, "W={w}");
            assert_eq!(l.best_start_position(), want_pos, "W={w}");
        }
    }

    /// : a reloaded world has the same height / lake tables and highest point, a (0,0) start
    /// and an all-zero food order; the spawn tables may differ near x in [-4, 4].
    #[test]
    fn reload_keeps_heights_and_zeroes_start_and_food() {
        let mk = |is_new| {
            let world = World::alloc(32);
            world.set_custom_rules(default_custom_rules(false));
            glibc_rand::srand(1792182287);
            WorldTileLoader::init_with_world_random_seed_is_new_world_save_id_loaded_version_block_database(
                &world,
                1792182287,
                is_new,
                "wgtest_reload",
                0,
                None,
            )
        };
        let a = mk(true);
        let b = mk(false);
        assert_eq!(a.terrain().rock_heights, b.terrain().rock_heights);
        assert_eq!(a.terrain().dirt_heights, b.terrain().dirt_heights);
        assert_eq!(a.terrain().lake_heights, b.terrain().lake_heights);
        assert_eq!(a.highest_point(), b.highest_point());
        assert_eq!(b.best_start_position(), IntPair { x: 0, y: 0 });
        assert_eq!(b.distance_ordered_food_types(), &[0; 31]);
        assert_ne!(a.distance_ordered_food_types(), &[0; 31]);
        assert_eq!(b.random_seed(), 1792182287);
    }

    ///  "Legacy light-block file migration": 32-character names without "ock" move to
    /// `playerLightBlocks/<c0>/<c1>/<rest>`.
    #[test]
    fn legacy_light_block_files_are_moved() {
        let dir = testutil::scratch("migrate");
        let id = "0123456789abcdef0123456789abcdef";
        std::fs::write(dir.join(id), b"x").unwrap();
        std::fs::write(dir.join("0_0_physicalBlock"), b"y").unwrap();
        std::fs::write(dir.join("0123456789abcdef0123456789abclock"), b"z").unwrap();
        WorldTileLoader::migrate_legacy_light_block_files(&format!("{}/", dir.display()));
        assert!(!dir.join(id).exists());
        assert!(dir.join(format!("playerLightBlocks/0/1/{}", &id[2..])).exists());
        assert!(dir.join("0_0_physicalBlock").exists());
        assert!(dir.join("0123456789abcdef0123456789abclock").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Test support shared by the block_io / light_archive tests: a recording [`WorldgenHost`] and a loader built
/// directly on temporary LMDB environments (no HOME writes).
#[cfg(test)]
pub(crate) mod testutil {
    use super::*;
    use std::collections::{BTreeSet, HashMap, HashSet};

    /// Everything the fake host saw / answers.
    #[derive(Default)]
    pub struct FakeHost {
        pub clients: RefCell<HashMap<String, (bool, i32)>>,
        pub receipts: RefCell<HashMap<String, Vec<u32>>>,
        pub indices: RefCell<BTreeSet<(String, u64)>>,
        pub saved_indices: RefCell<Vec<String>>,
        pub sent: RefCell<Vec<(Vec<u8>, Vec<String>, bool)>>,
        pub admins: RefCell<HashSet<String>>,
        pub dyn_data: RefCell<Option<DynamicObjectsNetData>>,
        pub fully_loaded: RefCell<Vec<(IntPair, i32)>>,
        pub chests: RefCell<Vec<(IntPair, bool, bool)>>,
        pub decommisions: Cell<u32>,
    }

    impl WorldgenHost for FakeHost {
        fn client_info(&self, _w: &World, id: &str) -> Option<(bool, i32)> {
            self.clients.borrow().get(id).copied()
        }
        fn client_receipts(&self, _w: &World, id: &str, _pos: IntPair) -> Option<Vec<u32>> {
            self.receipts.borrow_mut().remove(id)
        }
        fn client_contains_light_block_index(&self, _w: &World, id: &str, i: u64) -> bool {
            self.indices.borrow().contains(&(id.to_string(), i))
        }
        fn client_add_light_block_index(&self, _w: &World, id: &str, i: u64) {
            self.indices.borrow_mut().insert((id.to_string(), i));
        }
        fn client_save_light_block_indices(&self, _w: &World, id: &str) {
            self.saved_indices.borrow_mut().push(id.to_string());
        }
        fn send_network_data_to_peers_reliable(
            &self,
            _s: &Rc<BHServer>,
            data: &[u8],
            peers: &[String],
            reliable: bool,
        ) {
            self.sent.borrow_mut().push((data.to_vec(), peers.to_vec(), reliable));
        }
        fn player_is_admin_with_id(&self, _s: &Rc<BHServer>, id: &str) -> bool {
            self.admins.borrow().contains(id)
        }
        fn initial_dynamic_objects_net_data(&self, _w: &World, _i: i32, _id: &str) -> Option<DynamicObjectsNetData> {
            self.dyn_data.borrow().clone()
        }
        fn create_treasure_chest_or_troll_at_tile(
            &self,
            _w: &World,
            _t: TileRef,
            pos: IntPair,
            troll: bool,
            treasure: bool,
        ) {
            self.chests.borrow_mut().push((pos, troll, treasure));
        }
        fn decommision_all_blocks(&self, _w: &World) {
            self.decommisions.set(self.decommisions.get() + 1);
        }
        fn fully_load_if_needed_around_pos(&self, _w: &World, pos: IntPair, i: i32) {
            self.fully_loaded.borrow_mut().push((pos, i));
        }
    }

    /// Installs a fresh [`FakeHost`] and returns it.
    pub fn install_host() -> Rc<FakeHost> {
        let h = Rc::new(FakeHost::default());
        set_host(Some(h.clone()));
        h
    }

    /// A scratch directory unique to `name` and the process.
    pub fn scratch(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("wg_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A loader on an empty generator (W from the world) with a real `lightBlocks` LMDB under `dir` and an optional
    /// `blocks` DB under `dir/blocks_db`.
    pub fn loader_with_dbs(world: &Rc<World>, dir: &std::path::Path, with_block_db: bool) -> Rc<WorldTileLoader> {
        let env = DatabaseEnvironment::new(dir.join("lightBlocks").to_str().unwrap(), 1, 64).unwrap();
        let db = Rc::new(Database::new(&env, "main").unwrap());
        let block_database = if with_block_db {
            let benv = DatabaseEnvironment::new(dir.join("blocks_db").to_str().unwrap(), 4, 64).unwrap();
            let bdb = Rc::new(Database::new(&benv, "blocks").unwrap());
            std::mem::forget(benv); // the Database keeps its own Rc; the leak only avoids drop-order noise in tests
            Some(bdb)
        } else {
            None
        };
        Rc::new(WorldTileLoader {
            world: Rc::downgrade(world),
            t: Terrain::new_empty(world.world_width_macro(), 1),
            refine_terrain_count: Cell::new(0),
            has_refined_terrain: Cell::new(false),
            needs_to_exit: Cell::new(false),
            block_directory: format!("{}/blocks/", dir.display()),
            light_block_database: RefCell::new(Some(db)),
            light_block_database_environment: RefCell::new(Some(env)),
            block_database,
        })
    }

    /// A world with a (placeholder) server so `World::server()` is `Some`.
    pub fn world_with_server(w: i32) -> (Rc<World>, Rc<BHServer>) {
        let world = World::alloc(w);
        let server = Rc::new(BHServer { api: None });
        *world.server.borrow_mut() = Some(server.clone());
        (world, server)
    }
}

/// Whole-world generation over many rule sets: terrain, spawn tables, islands, ores, gems and the blocks themselves.
#[cfg(test)]
mod sweep_tests {
    use super::*;
    use crate::types::structs::CustomRules;
    use crate::world::custom_rules::default_custom_rules;
    use crate::worldgen::block_io::LoadOutcome;
    use crate::worldgen::terrain::GenParams;

    struct GenOps {
        loader: RefCell<Option<Rc<WorldTileLoader>>>,
    }
    impl crate::world::state::WorldOpsHooks for GenOps {
        fn load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
            &self,
            world: &World,
            mt: &crate::world::state::MacroTileRef,
            mx: i32,
            my: i32,
            _load_surrounding_blocks: bool,
            create_if_not_created: bool,
        ) -> bool {
            if mt.physical_block().is_none() || mt.dirty.get() != 0 {
                let pb = mt.physical_block().unwrap_or_else(|| world.take_physical_block());
                let l = self.loader.borrow().clone().unwrap();
                match l.load_physical_block_at_x_pos_y_pos_create_if_not_created(
                    pb,
                    mx,
                    my,
                    create_if_not_created,
                    mt.unexplored.get() != 0,
                ) {
                    LoadOutcome::Loaded | LoadOutcome::Generated => {
                        *mt.physical_block.borrow_mut() = Some(pb);
                        mt.unexplored.set(0);
                        mt.dynamic_objects_loaded.set(false);
                    }
                    LoadOutcome::NotFound => mt.unexplored.set(1),
                    LoadOutcome::Skipped => {}
                }
            }
            true
        }
        fn load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(
            &self,
            _world: &World,
            _mt: &crate::world::state::MacroTileRef,
            _include_surface_blocks: bool,
        ) {
        }
        fn tile_is_protected_at_pos_against_blockhead(
            &self,
            _world: &World,
            _pos: IntPair,
            _b: &crate::objects::handle::ObjRef,
        ) -> bool {
            false
        }
    }

    fn generate(w: i32, seed: u32, rules: CustomRules, expert: bool, is_new: bool) -> (Rc<World>, Rc<WorldTileLoader>) {
        let world = World::alloc(w);
        world.set_custom_rules(rules);
        world.set_expert_mode(expert);
        let ops = Rc::new(GenOps { loader: RefCell::new(None) });
        world.set_ops_hooks(Some(ops.clone()));
        glibc_rand::srand(seed);
        let l = WorldTileLoader::init_with_world_random_seed_is_new_world_save_id_loaded_version_block_database(
            &world,
            seed,
            is_new,
            "wgtest_sweep",
            0,
            None,
        );
        *ops.loader.borrow_mut() = Some(l.clone());
        glibc_rand::srand(l.random_seed());
        for _ in 0..8 {
            l.refine_terrain();
        }
        (world, l)
    }

    fn rules_with(f: impl FnOnce(&mut CustomRules)) -> CustomRules {
        let mut r = default_custom_rules(false);
        r.custom_rules_enabled = 1;
        f(&mut r);
        r
    }

    /// Loads a spread of blocks of the world (surface, caves, sky, depths).
    fn load_blocks(world: &Rc<World>, step: i32) {
        let w = world.world_width_macro();
        for my in (0..32).step_by(step as usize) {
            for mx in (0..w).step_by((w / 4).max(1) as usize) {
                crate::world::tiles::load_block_at_macro_pos(IntPair { x: mx, y: my }, world);
            }
        }
    }

    #[test]
    fn terrain_queries_outside_the_world_and_in_odd_settings() {
        for (k, rainfall) in [0i8, 1, 2, 3, 4, 5].into_iter().enumerate() {
            let (world, l) = generate(
                32,
                200 + k as u32,
                rules_with(|r| {
                    r.rainfall = rainfall;
                    r.caves = (k % 4) as i8;
                    r.lakes = 2;
                }),
                false,
                true,
            );
            let p = GenParams::from_world(&world);
            let wt = p.wt();
            let t = l.terrain();
            for x in [-5, -wt - 5, wt + 5, 2 * wt + 9, 3, wt - 1, wt] {
                let _ = t.sand_fraction_for_pos(&p, IntPair { x, y: 520 }, true);
            }
            for x in [-5, wt + 3, 7, wt] {
                let _ = (
                    t.max_of_rock_and_dirt_height(&p, x.min(wt - 1).max(-wt)),
                    t.lake_height(&p, x.min(wt - 1).max(-wt)),
                );
            }
            for x in 0..wt {
                let _ = t.unmodified_ground_level(&p, x);
            }
            if k % 2 == 0 {
                for isl in 0..=12u32 {
                    for wx in (0..1024).step_by(23) {
                        for wy in (0..1024).step_by(29) {
                            let mut tile = crate::types::tile::Tile::default();
                            t.place_gems_in_cave_tile(&p, &mut tile, wx, wy, isl);
                        }
                    }
                }
            }
            for y in [0, 100, 300, 600, 900] {
                for x in [10, 400, 900] {
                    let fo = t.fault_offset(&p, x, y);
                    let _ = (
                        t.is_cave(&p, x, y, fo),
                        t.is_floating_island_cave(&p, x, y),
                        t.limestone_fraction(&p, x, y, fo),
                    );
                }
            }
        }
    }

    #[test]
    fn dirt_flows_down_through_air_and_water() {
        let (world, l) = generate(32, 300, rules_with(|_| {}), false, true);
        let p = GenParams::from_world(&world);
        crate::world::tiles::load_block_at_macro_pos(IntPair { x: 3, y: 10 }, &world);
        crate::world::tiles::load_block_at_macro_pos(IntPair { x: 0, y: 10 }, &world);
        crate::world::tiles::load_block_at_macro_pos(IntPair { x: 31, y: 10 }, &world);
        for x0 in [96 + 3, 1, 1020i32] {
            for dy in 0..6 {
                for dx in -4..=4 {
                    let x = (x0 + dx).rem_euclid(p.wt());
                    if let Some(t) = crate::world::tiles::tile_at_world_position_loaded(x, 330 + dy, &world) {
                        t.update(|v| {
                            v.type_index = if dx % 2 == 0 { 2 } else { 3 };
                            v.zone_type_index = 1;
                        });
                    }
                }
            }
            for seed in 1..8u32 {
                glibc_rand::srand(seed);
                let start = crate::world::tiles::tile_at_world_position_loaded(x0, 335, &world).unwrap();
                l.terrain().recursively_flow_out_dirt_from_tile(&p, &world, &start, IntPair { x: x0, y: 335 });
            }
        }
    }

    #[test]
    fn blocks_that_are_not_there_stay_unexplored() {
        let (world, l) = generate(32, 99, rules_with(|_| {}), false, true);
        let ops = world.ops_hooks().unwrap();
        let mt = world.macro_tiles().get(5 * 32 + 7).unwrap();
        // nothing stored and not asked to create: unexplored; asked again: skipped
        assert!(ops.load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
            &world, &mt, 7, 5, false, false
        ));
        mt.unexplored.set(1);
        assert!(ops.load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
            &world, &mt, 7, 5, false, false
        ));
        let b = crate::world::helpers::testutil::new_obj();
        let o: crate::objects::handle::ObjRef = b;
        ops.load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(&world, &mt, true);
        assert!(!ops.tile_is_protected_at_pos_against_blockhead(&world, IntPair { x: 1, y: 1 }, &o));
        let h = testutil::install_host();
        let tile = TileRef::new(world.take_physical_block(), 0);
        h.create_treasure_chest_or_troll_at_tile(&world, tile, IntPair { x: 0, y: 0 }, false, true);
        h.decommision_all_blocks(&world);
        assert_eq!(h.decommisions.get(), 1);
        set_host(None);
        let _ = l;
    }

    #[test]
    fn every_rule_in_every_setting() {
        let cases: Vec<(i32, u32, CustomRules, bool)> = vec![
            (32, 1792182287, rules_with(|_| {}), false),
            (32, 11, rules_with(|r| r.sky_islands = 3), false),
            (32, 12, rules_with(|r| r.sky_islands = 1), true),
            (32, 13, rules_with(|r| r.ore_quantity = 0), false),
            (32, 14, rules_with(|r| r.ore_quantity = 1), false),
            (32, 15, rules_with(|r| r.ore_quantity = 3), true),
            (32, 16, rules_with(|r| r.gems = 3), false),
            (32, 17, rules_with(|r| r.gems = 1), false),
            (32, 18, rules_with(|r| r.treasure = 3), false),
            (32, 19, rules_with(|r| r.treasure = 1), false),
            (32, 20, rules_with(|r| r.caves = 3), false),
            (32, 21, rules_with(|r| r.caves = 1), false),
            (32, 22, rules_with(|r| r.lakes = 3), false),
            (32, 23, rules_with(|r| r.lakes = 1), false),
            (32, 24, rules_with(|r| r.oceans = 4), false),
            (32, 25, rules_with(|r| r.oceans = 1), false),
            (32, 26, rules_with(|r| r.oceans = 3), false),
            (32, 27, rules_with(|r| r.oceans = 0), false),
            (32, 28, rules_with(|r| r.terrain_type = 1), false),
            (32, 29, rules_with(|r| r.terrain_type = 3), false),
            (32, 30, rules_with(|r| r.terrain_type = 0), false),
            (32, 31, rules_with(|r| r.temperature = 0), false),
            (32, 32, rules_with(|r| r.temperature = 4), false),
            (32, 33, rules_with(|r| r.rainfall = 3), false),
            (32, 34, rules_with(|r| r.rainfall = 1), false),
            (32, 35, rules_with(|r| r.plants = 3), false),
            (32, 36, rules_with(|r| r.plants = 0), false),
            (32, 37, rules_with(|r| r.trees = 0), false),
            (32, 38, rules_with(|r| r.trees = 1), false),
            (32, 39, rules_with(|r| r.trees = 3), false),
            (32, 40, rules_with(|r| r.ore_distribution = 0), false),
            (32, 41, rules_with(|r| r.ore_distribution = 2), false),
            (32, 42, rules_with(|r| r.trees = 5), false),
            (32, 43, rules_with(|r| r.plants = 1), false),
            (32, 44, rules_with(|r| r.treasure = 0), false),
            (32, 45, rules_with(|r| r.caves = 2), true),
            (32, 46, rules_with(|r| r.sky_islands = 3), true),
            (64, 47, rules_with(|r| r.sky_islands = 3), false),
            (64, 48, rules_with(|r| r.sky_islands = 3), false),
            (32, 49, rules_with(|r| r.sky_islands = 3), false),
            (32, 50, rules_with(|r| r.sky_islands = 3), false),
            (32, 51, rules_with(|r| r.sky_islands = 3), false),
            (32, 52, rules_with(|r| r.sky_islands = 3), false),
            (32, 53, rules_with(|r| r.sky_islands = 3), false),
        ];
        for (w, seed, rules, expert) in cases {
            let (world, l) = generate(w, seed, rules, expert, true);
            load_blocks(&world, 2);
            let _ = (l.highest_point(), l.best_start_position());
        }
        // a world that is reloaded rather than created
        let (world, _l) = generate(32, 77, rules_with(|_| {}), false, false);
        load_blocks(&world, 4);
    }
}

#[cfg(test)]
mod loader_more_tests {
    use super::*;
    use crate::types::structs::CustomRules;
    use crate::world::custom_rules::default_custom_rules;
    use crate::worldgen::terrain::GenParams;

    #[test]
    fn the_null_host_and_the_accessors() {
        let h = NullHost;
        let w = World::alloc(2);
        let server = Rc::new(BHServer { api: None });
        let pb: crate::world::state::PhysicalBlockRef =
            Box::leak(Box::new(crate::world::state::PhysicalBlock::new_fresh()));
        let tile = TileRef::new(pb, 0);
        let p = IntPair { x: 0, y: 0 };
        assert!(h.client_info(&w, "c").is_none() && h.client_receipts(&w, "c", p).is_none());
        assert!(!h.client_contains_light_block_index(&w, "c", 1) && !h.player_is_admin_with_id(&server, "c"));
        h.client_add_light_block_index(&w, "c", 1);
        h.client_save_light_block_indices(&w, "c");
        h.send_network_data_to_peers_reliable(&server, &[], &[], true);
        assert!(h.initial_dynamic_objects_net_data(&w, 0, "c").is_none());
        h.create_treasure_chest_or_troll_at_tile(&w, tile, p, true, true);
        h.fully_load_if_needed_around_pos(&w, p, -1);
        h.decommision_all_blocks(&w);
    }

    fn quick_loader() -> (Rc<World>, Rc<WorldTileLoader>) {
        let world = World::alloc(32);
        let mut r: CustomRules = default_custom_rules(false);
        r.custom_rules_enabled = 1;
        world.set_custom_rules(r);
        glibc_rand::srand(1792182287);
        let l = WorldTileLoader::init_with_world_random_seed_is_new_world_save_id_loaded_version_block_database(
            &world,
            1792182287,
            true,
            "wgtest_more",
            0,
            None,
        );
        (world, l)
    }

    #[test]
    fn every_accessor_answers() {
        let (world, l) = quick_loader();
        let _ = (l.tree_density_noise_function(), l.season_offset_noise_function());
        assert_eq!(l.npc_positions().len(), 1024);
        assert_eq!(l.plant_positions().len(), 1024);
        assert!(!l.needs_to_exit());
        l.set_needs_to_exit(true);
        assert!(l.needs_to_exit());
        assert!(l.block_directory().ends_with("blocks/"));
        assert!(l.world().is_some());
        let _ = l.gen_params();
        let p = IntPair { x: 300, y: 520 };
        let _ = (
            l.unmodified_ground_level_for_x(300),
            l.max_of_rock_and_dirt_height_for_x(300),
            l.lake_height_for_x(300),
            l.get_rock_and_dirt_height_for_x_rock_height_dirt_height(300),
            l.get_initial_rock_and_dirt_height_for_x_rock_height_dirt_height(300),
            l.sand_fraction_for_pos_high_res(p, true),
            l.sand_fraction_for_pos_height_high_res(p, 520, false),
            l.is_cave_for_x_y_fault_offset(300, 100, 0),
            l.is_floating_island_cave_for_x_y(300, 800),
            l.fault_offset_for_x_y(300, 100),
            l.get_cloud_height_for_x(300),
            l.limestone_fraction_for_x_y_fault_offset(300, 100, 0),
            l.sandstone_fraction_for_x_y_fault_offset_limestone_fraction(300, 100, 0, 0.5),
            l.is_desert_for_pos_height(p, 520),
            l.is_beach_for_pos_height(p, 520),
            l.is_desert_or_beach_for_pos_height(p, 520),
            l.find_best_start_position(),
        );
        // pouring water and dirt into a block
        let pb = world.take_physical_block();
        pb.valid.set(true);
        let t = TileRef::new(pb, 5);
        l.fill_dirt_tile_world_pos_world_dirt_height_parent_type(&t, IntPair { x: 5, y: 10 }, 12, 6);
        l.place_gems_in_cave_for_physical_block_tile_index_world_x_world_y_floating_island_type(pb, 7, 300, 800, 0);
        l.recursively_flow_out_water_from_tile_at_pos(IntPair { x: 5, y: 10 });
        l.dealloc();
        // a loader whose world is gone refines nothing
        let (world, l) = quick_loader();
        drop(world);
        l.refine_terrain();
        assert!(l.world().is_none());
        let _ = l.gen_params();
        l.recursively_flow_out_water_from_tile_at_pos(IntPair { x: 5, y: 10 });
    }

    #[test]
    fn legacy_light_blocks_that_cannot_move_are_dropped() {
        let dir = testutil::scratch("migrate2");
        let id = "0123456789abcdef0123456789abcdef";
        let id2 = "1123456789abcdef0123456789abcdef";
        // a folder and a file whose destinations exist already
        std::fs::create_dir_all(dir.join(id)).unwrap();
        std::fs::write(dir.join(id).join("inner"), b"x").unwrap();
        std::fs::write(dir.join(id2), b"y").unwrap();
        std::fs::create_dir_all(dir.join(format!("playerLightBlocks/0/1/{}", &id[2..]))).unwrap();
        std::fs::create_dir_all(dir.join(format!("playerLightBlocks/1/1/{}", &id2[2..]))).unwrap();
        WorldTileLoader::migrate_legacy_light_block_files(&format!("{}/", dir.display()));
        assert!(!dir.join(id).exists() && !dir.join(id2).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn heights_are_rebuilt_when_the_world_changes_size() {
        let mut t = Terrain::new_empty(16, 5);
        let mut r = default_custom_rules(false);
        r.custom_rules_enabled = 1;
        let p = GenParams { w: 32, rules: r, expert: false };
        WorldTileLoader::step_a_noise(&mut t, &p);
        WorldTileLoader::step_b_c_heights_and_lakes(&mut t, &p);
        assert_eq!(t.rock_heights.len(), 1024);
    }

    #[test]
    fn a_server_world_keeps_light_blocks_in_its_own_database() {
        let g = crate::world::lifecycle::testkit::HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = format!("{}/bhwg_server_{}", std::env::temp_dir().display(), std::process::id());
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        crate::foundation::paths::set_home_for_test(&home);
        let (world, _s) = testutil::world_with_server(32);
        let mut r = default_custom_rules(false);
        r.custom_rules_enabled = 1;
        world.set_custom_rules(r);
        glibc_rand::srand(5);
        let l = WorldTileLoader::init_with_world_random_seed_is_new_world_save_id_loaded_version_block_database(
            &world, 5, false, "wgserver", 0, None,
        );
        assert!(l.light_block_database.borrow().is_some());
        l.dealloc();
        drop(g);
    }
}
