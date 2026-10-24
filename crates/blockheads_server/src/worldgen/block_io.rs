//! owned by worldgen
//!
//! Block persistence and networking: `loadPhysicalBlock`, `savePhysicalBlock`, `sendBlockToClientWithoutSaving`,
//! `updatePhysicalBlockToLatestVersion` and the block <-> bytes helpers.
//!

use std::collections::HashMap;
use std::rc::Rc;

use crate::fmath::cconv::f32_to_i32;
use crate::foundation::gzip::{gzip_deflate, gzip_inflate};
use crate::foundation::{clock, paths};
use crate::nslog;
use crate::plist::value::PlistValue;
use crate::protocol::block_wire::{self, ExtraData};
use crate::types::structs::IntPair;
use crate::types::tile::{serialize_block_plain, tiles_from_bytes, BLOCK_TILES, TILE_SIZE};
use crate::world::state::{BHServer, PhysicalBlock, PhysicalBlockRef, TileRef};

use super::noise::noise_xy;
use super::terrain::{set_lava_gradient, tt, GenParams};
use super::tile_loader::{host, WorldTileLoader};

/// Result of `loadPhysicalBlock`. The CALLER applies the macro tile writes (R9): for `Loaded` /
/// `Generated`, `physicalBlock = pb`, `unexplored = 0`, `dynamicObjectsLoaded = 0`; for `NotFound`, `unexplored = 1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadOutcome {
    /// Early return: `!(create || macroTile.unexplored == 0)`. The caller changes nothing.
    Skipped,
    /// Nothing on disk and `!create`. The caller sets `macroTile.unexplored = 1`.
    NotFound,
    /// Loaded from the blocks DB or a legacy file.
    Loaded,
    /// Procedurally generated.
    Generated,
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
    /// Block value as stored in the `blocks` DB before gzip: tiles(65536) ++ version(1) ++ update_index(u32 LE).
    /// Shared by save and send.
    pub(crate) fn block_payload(pb: &PhysicalBlock) -> Vec<u8> {
        let tiles: Vec<_> = pb.tiles.iter().map(|c| c.get()).collect();
        serialize_block_plain(&tiles, pb.version.get(), pb.update_index.get())
    }

    /// -[WorldTileLoader loadPhysicalBlock:atXPos:yPos:createIfNotCreated:]. On `Loaded` / `Generated`
    /// the loader has already filled `pb` (`x_pos` / `y_pos` wrapped, `valid`, `last_required_time`, indices 0,
    /// light blocks freed). `macro_unexplored` is `macroTile->unexplored != 0` of the (wrapped) macro tile.
    pub fn load_physical_block_at_x_pos_y_pos_create_if_not_created(
        &self,
        pb: PhysicalBlockRef,
        x_pos: i32,
        y_pos: i32,
        create_if_not_created: bool,
        macro_unexplored: bool,
    ) -> LoadOutcome {
        let world = self.world.upgrade();
        let p = GenParams::from_world_opt(world.as_deref());
        let w = p.w;
        // 1. wrap X (single wrap; Y is not wrapped)
        let mut x_pos = x_pos;
        if x_pos < 0 {
            x_pos += w;
        } else if x_pos >= w {
            x_pos -= w;
        }
        let create = create_if_not_created;
        if !(create || !macro_unexplored) {
            return LoadOutcome::Skipped;
        }

        // 2. load
        let mut load_success = false;
        let key = format!("{}_{}", x_pos, y_pos);
        let mut tiles_data: Option<Vec<u8>> =
            self.block_database.as_ref().and_then(|db| db.data_for_key(Some(&key))).and_then(|d| gzip_inflate(&d));
        if tiles_data.is_none() {
            let path =
                paths::append_path_component(&self.block_directory, &format!("{}_{}_physicalBlock", x_pos, y_pos));
            if std::path::Path::new(&path).exists() {
                tiles_data = std::fs::read(&path).ok();
            }
        }
        if let Some(d) = &tiles_data {
            if d.len() > 0x10000 {
                Self::copy_tiles_into(pb, d);
                pb.version.set(d[0x10000]);
                load_success = true;
                pb.update_index.set(0);
                pb.light_block_update_index.set(0);
            }
        }
        if !load_success {
            let path2 =
                paths::append_path_component(&self.block_directory, &format!("{}_{}_compressedBlock", x_pos, y_pos));
            if std::path::Path::new(&path2).exists() {
                let d = std::fs::read(&path2).ok().and_then(|raw| gzip_inflate(&raw));
                if let Some(d) = d {
                    if d.len() > 0x10000 {
                        Self::copy_tiles_into(pb, &d);
                        pb.version.set(d[0x10000]);
                        load_success = true;
                    }
                }
            }
        }
        if load_success || create {
            pb.x_pos.set(x_pos);
            pb.y_pos.set(y_pos);
            pb.valid.set(true);
            pb.last_required_time.set(clock::reference_time());
            pb.update_index.set(0);
            pb.light_block_update_index.set(0);
            for c in pb.client_light_blocks.borrow_mut().iter_mut() {
                *c = None;
            }
        }
        if !load_success && !create {
            return LoadOutcome::NotFound;
        }

        // 4a. re-assert the lava layer on a loaded block (every load)
        if load_success {
            for x in 0..32i32 {
                let world_x = pb.x_pos.get() * 32 + x;
                let (r, _d) = self.t.get_rock_and_dirt_height(&p, world_x);
                let lava_top = r / 16 - pb.y_pos.get() * 32;
                let local_lava = lava_top + 16;
                for y in 0..32i32 {
                    let cell = &pb.tiles[(y * 32 + x) as usize];
                    let mut t = cell.get();
                    if y < local_lava {
                        t.type_index = tt::LAVA;
                        t.artificial_light_r = 0xff;
                        t.artificial_light_g = 0x7f;
                        t.artificial_light_b = 0x1f;
                        t.artificial_heat = 0x7f;
                    } else if y < lava_top + 48 {
                        set_lava_gradient(&mut t, y, local_lava);
                    } else {
                        t.artificial_light_r = 0;
                        t.artificial_light_g = 0;
                        t.artificial_light_b = 0;
                        t.artificial_heat = 0;
                    }
                    cell.set(t);
                }
            }
            return LoadOutcome::Loaded;
        }

        // 4b. generate
        self.t.generate_block_tiles(&p, pb, x_pos, y_pos, world.as_deref());
        LoadOutcome::Generated
    }

    /// `memcpy(pb->tiles, bytes, 0x10000)`.
    fn copy_tiles_into(pb: PhysicalBlockRef, bytes: &[u8]) {
        let tiles = tiles_from_bytes(&bytes[..BLOCK_TILES * TILE_SIZE]).expect("0x10000 bytes");
        for (c, t) in pb.tiles.iter().zip(tiles) {
            c.set(t);
        }
    }

    /// -[WorldTileLoader savePhysicalBlock:macroTile:sendToClients:server:sendReliably:]. The `macroTile`
    /// argument is unused in the original and dropped. Raises `DataSaveException` if gzip fails. `send_reliably`
    /// is ignored by the original (always reliable).
    pub fn save_physical_block_send_to_clients_server_send_reliably(
        &self,
        pb: PhysicalBlockRef,
        clients: &[String],
        server: Option<&Rc<BHServer>>,
        _send_reliably: bool,
    ) {
        let world = self.world.upgrade();
        let x_pos = pb.x_pos.get();
        let y_pos = pb.y_pos.get();
        let tiles_data = Self::block_payload(pb);
        // `gzip_deflate` only answers an empty buffer for an empty input and the payload is 0x10005 bytes long, so
        // the original's "Failed to save tilesData." DataSaveException cannot be raised.
        let deflated = gzip_deflate(&tiles_data);
        pb.update_index.set(pb.update_index.get().wrapping_add(1));
        let mut fill_receipts: HashMap<String, Vec<u8>> = HashMap::new();
        let h = host();
        for peer in clients {
            // The `sc` lookup is not nil-checked in the original: a nil `sc` (or nil world) answers
            // `lightBlockIndex == 0` and `getAndRemoveAllRecieptDataForMacroPos:` == nil by ObjC nil messaging,
            // so the block is still saved and pushed with light block 0 instead of crashing.
            let li = world.as_deref().and_then(|w| h.client_info(w, peer)).map(|(_c, li)| li).unwrap_or(0);
            let li = li as usize;
            if pb.client_light_blocks.borrow()[li].is_none() {
                self.load_light_block_for_client_light_block_index_client_id_into_physical_block(li as i32, peer, pb);
            }
            let light: Option<[u8; 1024]> = pb.client_light_blocks.borrow()[li].as_ref().map(|b| **b);
            if let Some(light) = light {
                let admin = match server {
                    Some(s) => h.player_is_admin_with_id(s, peer),
                    None => false,
                };
                let extra = ExtraData {
                    hash: block_wire::block_hash(peer, x_pos, y_pos, admin),
                    receipts: world.as_deref().and_then(|w| h.client_receipts(w, peer, IntPair { x: x_pos, y: y_pos })),
                };
                // `light_blob` fails only if `write_xml` raised, which it never does for a string, ints and data
                let blob = block_wire::light_blob(&light, &extra).expect("light blob");
                fill_receipts.insert(peer.clone(), blob);
            }
        }
        if let Some(db) = &self.block_database {
            db.set_data(Some(&deflated), Some(&format!("{}_{}", x_pos, y_pos)));
        }
        if let Some(server) = server {
            if !clients.is_empty() {
                let w = world.as_deref().map(|w| w.world_width_macro()).unwrap_or(0);
                let index = (y_pos.wrapping_mul(w).wrapping_add(x_pos)) as u32;
                for peer in clients {
                    let light_gz = fill_receipts.get(peer).map(|l| gzip_deflate(l));
                    let m = block_wire::server_block(index, &deflated, light_gz.as_deref()).expect("server block");
                    h.send_network_data_to_peers_reliable(server, &m, std::slice::from_ref(peer), true);
                }
            }
        }
    }

    /// -[WorldTileLoader sendBlockToClientWithoutSavingForBlock:pos:sendToClient:server:sendDynamicObjects:reliable:]
    /// Returns `sent`. Side effects in the original's order: `update_index += 1`, receipts removed from
    /// the client, light block loaded, then the block packet and the dynamic object packets.
    pub fn send_block_to_client_without_saving_for_block_pos_send_to_client_server_send_dynamic_objects_reliable(
        &self,
        pb: PhysicalBlockRef,
        pos: IntPair,
        send_to_client: Option<&str>,
        server: Option<&Rc<BHServer>>,
        send_dynamic_objects: bool,
        reliable: bool,
    ) -> bool {
        let (server, client_id) = match (server, send_to_client) {
            (Some(s), Some(c)) => (s, c),
            _ => return false,
        };
        let h = host();
        let tiles_blob = Self::block_payload(pb);
        pb.update_index.set(pb.update_index.get().wrapping_add(1));
        // A nil world gives a nil `sc` (ObjC nil messaging): logged and NO, after the increment.
        let world = self.world.upgrade();
        let sc = world.as_deref().and_then(|w| h.client_info(w, client_id));
        let world = match world {
            Some(w) if sc.is_some() => w,
            _ => {
                nslog!("no serverclient for block request");
                return false;
            }
        };
        let li = match sc {
            Some((true, li)) => li as usize,
            _ => {
                nslog!("no serverclient for block request");
                return false;
            }
        };
        if pb.client_light_blocks.borrow()[li].is_none() {
            self.load_light_block_for_client_light_block_index_client_id_into_physical_block(li as i32, client_id, pb);
        }
        let light: [u8; 1024] = match pb.client_light_blocks.borrow()[li].as_ref() {
            Some(b) => **b,
            None => {
                nslog!("unable to load light block");
                return false;
            }
        };
        let admin = h.player_is_admin_with_id(server, client_id);
        let extra = ExtraData {
            hash: block_wire::block_hash(client_id, pos.x, pos.y, admin),
            receipts: h.client_receipts(&world, client_id, pos),
        };
        // `light_blob` fails only if `write_xml` raised, which it never does for a string, ints and data
        let light_blob = block_wire::light_blob(&light, &extra).expect("light blob");
        let w = world.world_width_macro();
        let index = (pos.y.wrapping_mul(w).wrapping_add(pos.x)) as u32;
        let dyn_info = if send_dynamic_objects {
            h.initial_dynamic_objects_net_data(&world, index as i32, client_id)
        } else {
            None
        };
        let packet = block_wire::server_block(index, &gzip_deflate(&tiles_blob), Some(&gzip_deflate(&light_blob)))
            .expect("server block");
        h.send_network_data_to_peers_reliable(server, &packet, &[client_id.to_string()], reliable);
        if let Some(dyn_info) = dyn_info {
            let peers = [client_id.to_string()];
            for (kind, entries) in [(7u8, &dyn_info.creation), (8u8, &dyn_info.update)] {
                for (type_num, arr) in entries {
                    let mut m = vec![kind, *type_num as u8];
                    if let Some(x) = crate::plist::helpers::data_from_property_list(arr) {
                        m.extend_from_slice(&gzip_deflate(&x));
                    }
                    h.send_network_data_to_peers_reliable(server, &m, &peers, true);
                }
            }
        }
        true
    }

    /// -[WorldTileLoader updatePhysicalBlockToLatestVersion:]. Does NOT write `pb.version`
    /// (DynamicWorld sets 8 afterwards).
    pub fn update_physical_block_to_latest_version(&self, pb: PhysicalBlockRef) {
        let world = self.world.upgrade();
        let p = GenParams::from_world_opt(world.as_deref());
        let version = pb.version.get();
        let bsx = self.t.best_start_position.x;
        let wall_w = p.wt();
        let yh = self.t.y_height_divider;
        let y_pos = pb.y_pos.get();
        let x_pos = pb.x_pos.get();
        let h = host();
        let den = if p.w < 512 { 512 } else { p.w };

        // Pass A: treasure chests on beach sand
        if version < 3 {
            for x in 0..32i32 {
                let world_x = x_pos * 32 + x;
                let (r, d) = self.t.get_rock_and_dirt_height(&p, world_x);
                let local_rock = r - y_pos * 32;
                let local_dirt = d - y_pos * 32;
                if 0 <= local_rock
                    && local_rock < 32
                    && r > 0x1f0
                    && world_x != 0
                    && world_x != wall_w / 4
                    && world_x != (wall_w / 4) * 2
                    && world_x != (wall_w / 4) * 3
                {
                    let ti = (local_rock * 32 + x) as usize;
                    let t = pb.tiles[ti].get();
                    if t.contents == 0 && t.type_index == tt::SAND_BEACH && (local_dirt - local_rock) > 2 {
                        let fo = self.t.fault_offset(&p, world_x, r - 1);
                        if !self.t.is_cave(&p, world_x, r - 1, fo) {
                            let x_f = (((world_x as f32) + 1453.0f32) / 32.0f32) / yh;
                            let y_f = (((r as f32) + 1247.0f32) / 32.0f32) / yh;
                            let tc =
                                noise_xy(&self.t.flint, (1.423f32 * x_f) as f64, (1.4536f32 * y_f) as f64, 5) as f32;
                            if 0.85f32 - 0.3f32 < tc && tc < 0.85f32 {
                                let tr = TileRef::new(pb, ti);
                                tr.update(|t| {
                                    t.zone_type_index = 1;
                                    t.back_wall_type_index = 1;
                                    t.type_index = tt::AIR;
                                });
                                if let Some(wd) = world.as_deref() {
                                    h.create_treasure_chest_or_troll_at_tile(
                                        wd,
                                        tr,
                                        IntPair { x: world_x, y: r },
                                        false,
                                        true,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // Pass B: black sand on beaches
        if version < 5 {
            for x in 0..32i32 {
                let world_x = x_pos * 32 + x;
                if world_x == bsx {
                    continue;
                }
                let (r, d) = self.t.get_rock_and_dirt_height(&p, world_x);
                let local_rock = r - y_pos * 32;
                let local_dirt = d - y_pos * 32;
                let mut y = imax(local_rock, 0);
                while y < local_dirt && y < 32 {
                    let world_y = y_pos * 32 + y;
                    let ti = (y * 32 + x) as usize;
                    let combined = imax(self.t.dirt_heights[world_x as usize], self.t.rock_heights[world_x as usize]);
                    if self.t.is_beach(&p, IntPair { x: world_x, y: world_y }, combined)
                        && pb.tiles[ti].get().type_index == tt::SAND_BEACH
                    {
                        let rh = self.t.rock_heights[world_x as usize];
                        let xf = ((world_x as f32) / 32.0f32) / (p.w as f32);
                        let yf = ((((world_y as f64 - rh as f64 * 0.5) as f32) / 32.0f32) / (den as f32)) * 4.0f32;
                        let n = noise_xy(&self.t.flint, xf as f64 * 0.0625, yf as f64 * 0.625, 3) as f32;
                        if n > 0.4f32 {
                            let mut t = pb.tiles[ti].get();
                            t.type_index = tt::BLACK_SAND;
                            t.back_wall_type_index = tt::BLACK_SAND;
                            pb.tiles[ti].set(t);
                        }
                    }
                    y += 1;
                }
            }
        }

        // Pass C: platinum and titanium
        if version < 7 {
            for x in 0..32i32 {
                let world_x = x_pos * 32 + x;
                if world_x == bsx {
                    continue;
                }
                let (r, _d) = self.t.get_rock_and_dirt_height(&p, world_x);
                let mut y = 0i32;
                while y < (r - y_pos * 32) && y < 32 {
                    let world_y = y_pos * 32 + y;
                    let ti = (y * 32 + x) as usize;
                    let fault = self.t.fault_offset(&p, world_x, world_y);
                    let t0 = pb.tiles[ti].get();
                    if t0.type_index == tt::ROCK && t0.contents == 0 {
                        let xf = ((((world_x as f32) + 5673.0f32) / 32.0f32) / (p.w as f32)) * 4.0f32;
                        let yf = ((((world_y + 0x18c7) - fault) as f32) / 32.0f32 / (den as f32)) * 4.0f32;
                        let (xd, yd) = (xf as f64, yf as f64);
                        let g = noise_xy(&self.t.flint, xd * 0.112, yd * 0.084, 2) as f32;
                        if g > 0.0f32 {
                            let wy = imax(world_y, 400);
                            let a = (1.0f32 - (wy as f32) / (self.t.rock_heights[world_x as usize] as f32)) * 0.004f32;
                            let n = noise_xy(&self.t.tin, xd * 0.012 + 0.212, yd + 0.47, 2) as f32;
                            if (0.299f32 - a < n && n < 0.3f32) || (n < -0.299f32 + a && -0.3f32 < n) {
                                let rr = noise_xy(&self.t.flint, xd * 0.012, yd * 0.054, 2) as f32;
                                if 0.1f32 < rr && rr < 0.15f32 {
                                    let mut t = pb.tiles[ti].get();
                                    t.contents = 0x6a;
                                    pb.tiles[ti].set(t);
                                }
                            } else {
                                let n = noise_xy(&self.t.tin, xd * 0.028 + 0.612, yd + 0.27, 2) as f32;
                                if (0.298f32 - a < n && n < 0.3f32) || (n < -0.298f32 + a && -0.3f32 < n) {
                                    let rr = noise_xy(&self.t.flint, xd * 0.035, yd * 0.072, 2) as f32;
                                    if 0.1f32 < rr && rr < 0.15f32 {
                                        let mut t = pb.tiles[ti].get();
                                        t.contents = 0x6b;
                                        pb.tiles[ti].set(t);
                                    }
                                }
                            }
                        }
                    }
                    y += 1;
                }
            }
        }
        let _ = f32_to_i32;
        let _ = PlistValue::Null;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::glibc_rand;
    use crate::storage::lmdb::{Database, DatabaseEnvironment};
    use crate::types::tile::{parse_block_plain, Tile};
    use crate::world::custom_rules::default_custom_rules;
    use crate::world::state::World;
    use crate::worldgen::tile_loader::sweep_tests::GenOps;

    /// Copies a sample `world_db` to a scratch dir (opening LMDB may touch lock files) and returns all stored blocks.
    fn sample_blocks_under(root: &str, sample: &str, id: &str) -> Option<Vec<(i32, i32, Vec<u8>)>> {
        let id = if id.is_empty() {
            std::fs::read_dir(format!("{}/{}", root, sample))
                .ok()?
                .filter_map(|e| e.ok())
                .find(|e| e.path().join("world_db").exists())?
                .file_name()
                .into_string()
                .ok()?
        } else {
            id.to_string()
        };
        let src = format!("{}/{}/{}/world_db", root, sample, id);
        if !std::path::Path::new(&src).exists() {
            return None;
        }
        let dst = std::env::temp_dir().join(format!("wg_sample_{}_{}", sample, std::process::id()));
        let _ = std::fs::remove_dir_all(&dst);
        std::fs::create_dir_all(&dst).unwrap();
        for f in ["data.mdb", "lock.mdb"] {
            std::fs::copy(format!("{}/{}", src, f), dst.join(f)).unwrap();
        }
        let env = DatabaseEnvironment::new(dst.to_str().unwrap(), 8, 256)?;
        let db = Database::new(&env, "blocks")?;
        let mut out = Vec::new();
        for (k, v) in db.entries() {
            let key = String::from_utf8(k).unwrap();
            let (xs, ys) = key.split_once('_').unwrap();
            let plain = gzip_inflate(&v).unwrap();
            out.push((xs.parse().unwrap(), ys.parse().unwrap(), plain));
        }
        drop(db);
        drop(env);
        let _ = std::fs::remove_dir_all(&dst);
        Some(out)
    }

    /// Tiles that DynamicWorld touched after generation (tree trunks with a season offset) are not generation:
    /// the generated tile takes over what the stored one has there.
    fn without_later_changes(mut g: Tile, t: &Tile) -> Tile {
        if t.season_offset != 0 && (g.contents == 0 || g.contents == t.contents) {
            g.season_offset = t.season_offset;
            g.contents = t.contents;
            g.dynamic_object_owner = t.dynamic_object_owner;
        }
        g
    }

    #[test]
    fn later_changes_by_the_dynamic_world_are_not_generation() {
        let stored = Tile { season_offset: 9, contents: 4, dynamic_object_owner: 77, ..Tile::default() };
        assert_eq!(without_later_changes(Tile::default(), &stored), stored);
        let other = Tile { contents: 5, ..Tile::default() };
        assert_eq!(without_later_changes(other, &stored), other);
        assert_eq!(without_later_changes(other, &Tile::default()), other);
    }

    /// Full creation pipeline of a sample (srand, init, srand, 8 x refine_terrain with on-demand block loading)
    /// followed by a block-for-block comparison of every fresh stored block.
    fn compare_sample(
        sample: &str,
        id: &str,
        w: i32,
        seed: u32,
        rules: crate::types::structs::CustomRules,
        expert: bool,
    ) {
        compare_sample_under(&crate::testenv::refenv_samples(), sample, id, w, seed, rules, expert);
    }

    fn compare_sample_under(
        root: &str,
        sample: &str,
        id: &str,
        w: i32,
        seed: u32,
        rules: crate::types::structs::CustomRules,
        expert: bool,
    ) {
        let blocks = match sample_blocks_under(root, sample, id) {
            Some(b) => b,
            None => return,
        };
        let world = World::alloc(w);
        world.set_custom_rules(rules);
        world.set_expert_mode(expert);
        let ops = Rc::new(GenOps { loader: std::cell::RefCell::new(None) });
        world.set_ops_hooks(Some(ops.clone()));
        glibc_rand::srand(seed);
        let l = WorldTileLoader::init_with_world_random_seed_is_new_world_save_id_loaded_version_block_database(
            &world,
            seed,
            true,
            &format!("wgtest_{sample}"),
            0,
            None,
        );
        *ops.loader.borrow_mut() = Some(l.clone());
        glibc_rand::srand(l.random_seed());
        for _ in 0..8 {
            l.refine_terrain();
        }
        let mut compared = 0;
        let mut diff_blocks = 0;
        let macro_tiles = world.macro_tiles();
        for (x, y, plain) in blocks.iter().filter(|b| parse_block_plain(&b.2).unwrap().1 == 0) {
            let (tiles, _version, _upd) = parse_block_plain(plain).unwrap();
            let mt = macro_tiles.get((*y * w + *x) as usize).unwrap();
            if mt.physical_block().is_none() {
                crate::world::tiles::load_block_at_macro_pos(IntPair { x: *x, y: *y }, &world);
            }
            let pb = mt.physical_block().expect("block generated");
            compared += 1;
            let differs = tiles.iter().enumerate().any(|(i, t)| without_later_changes(pb.tiles[i].get(), t) != *t);
            diff_blocks += usize::from(differs);
        }
        eprintln!("{sample}: compared {compared} fresh blocks, {diff_blocks} differ");
        assert!(compared > 0);
        assert_eq!(diff_blocks, 0, "{sample}: generated blocks differ from the reference");
    }

    fn rules_default(enabled: bool, expert: bool) -> crate::types::structs::CustomRules {
        let mut r = default_custom_rules(expert);
        r.custom_rules_enabled = enabled as u8;
        r
    }

    /// Block-for-block comparison against `the reference environment/seedA` (custom seed 12345, W = 512, default rules with
    /// `customRulesEnabled = 1`).
    #[test]
    fn generated_blocks_match_seed_a_sample() {
        compare_sample("seedA", "cccccccccccccccccccccccccccccc01", 512, 12345, rules_default(true, false), false);
    }

    /// Sample w1 (time seed 1792182285, W = 512, default rules).
    #[test]
    fn generated_blocks_match_w1_sample() {
        compare_sample("w1", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa01", 512, 1792182285, rules_default(false, false), false);
    }

    /// Sample nocust (time seed 1792183669).
    #[test]
    fn generated_blocks_match_nocust_sample() {
        compare_sample(
            "nocust",
            "cccccccccccccccccccccccccccccc01",
            512,
            1792183669,
            rules_default(false, false),
            false,
        );
    }

    /// Sample expert (expert mode changes ore spread, flint/clay thresholds, treasure and gem tables).
    #[test]
    fn generated_blocks_match_expert_sample() {
        compare_sample("expert", "", 512, 1792182285, rules_default(false, true), true);
    }

    /// Sample w16th (W = 32): two food rejections, the retry seeds are 1792182286 and 1792182287.
    #[test]
    fn generated_blocks_match_w16th_sample() {
        compare_sample("w16th", "", 32, 1792182285, rules_default(false, false), false);
    }

    /// Sample w4 (W = 2048): one food rejection (9/7799, then 8/50609 with seed 1792182286).
    #[test]
    fn generated_blocks_match_w4_sample() {
        compare_sample("w4", "", 2048, 1792182285, rules_default(false, false), false);
    }

    /// Sample custom (hash seed 166199516, non-default rules: ocean world, hot).
    #[test]
    fn generated_blocks_match_custom_sample() {
        let json = br#"{"seed":"custom","custom_seed_value":"hello world","oceans":"ocean world","temperature":"hot","health":"easy","day/night cycle":"faster","world modification":"admin only","sunColor_0":255,"sunColor_1":128,"sunColor_2":64}"#;
        let v = crate::plist::json::json_object_with_data(json).value.unwrap();
        let PlistValue::Dict(d) = &v else { unreachable!() };
        let rules = crate::world::custom_rules::custom_rules_from_dict(Some(d), false);
        compare_sample("custom", "", 512, 166199516, rules, false);
    }

    /// Blocks in the old per-block files load, and the lava layer is re-asserted on every load.
    #[test]
    fn legacy_block_files_load_and_get_their_lava_back() {
        let dir = scratch("legacy_blocks");
        let (world, _server) = world_with_server(32);
        let l = loader_with_dbs(&world, &dir, false);
        std::fs::create_dir_all(dir.join("blocks")).unwrap();
        let mut tiles = vec![Tile::default(); 1024];
        for t in tiles.iter_mut() {
            t.type_index = 6;
        }
        let plain = crate::types::tile::serialize_block_plain(&tiles, 8, 0);
        // the raw file, and the gzip one; a gzip file that is no gzip; a block that is too short
        std::fs::write(dir.join("blocks/3_0_physicalBlock"), &plain).unwrap();
        std::fs::write(dir.join("blocks/4_1_compressedBlock"), gzip_deflate(&plain)).unwrap();
        std::fs::write(dir.join("blocks/5_1_compressedBlock"), b"junk").unwrap();
        std::fs::write(dir.join("blocks/6_1_physicalBlock"), b"short").unwrap();
        for (x, y) in [(3, 0), (4, 1), (5, 1), (6, 1), (7, 7), (-1, 0), (40, 0)] {
            let pb = world.take_physical_block();
            let outcome = l.load_physical_block_at_x_pos_y_pos_create_if_not_created(pb, x, y, false, false);
            if (x, y) == (3, 0) || (x, y) == (4, 1) {
                assert_eq!(outcome, LoadOutcome::Loaded);
                assert_eq!(pb.version.get(), 8);
                // the lava layer sits at the bottom of the low block
                if y == 0 {
                    assert_eq!(pb.tiles[0].get().type_index, tt::LAVA);
                }
            } else {
                assert_eq!(outcome, LoadOutcome::NotFound);
            }
        }
        // asked to create but not allowed to look: skipped
        let pb = world.take_physical_block();
        assert_eq!(
            l.load_physical_block_at_x_pos_y_pos_create_if_not_created(pb, 9, 9, false, true),
            LoadOutcome::Skipped
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Saving and sending when the light blocks, the server or the peer are missing.
    #[test]
    fn saving_and_sending_with_missing_pieces() {
        let host = install_host();
        let dir = scratch("save_missing");
        let (world, server) = world_with_server(32);
        let l = loader_with_dbs(&world, &dir, true);
        host.clients.borrow_mut().insert("c1".into(), (true, 3));
        // saved to nobody without a server: only the database is written
        let pb = marked_block(&world, 5, 6, 8, 41);
        l.save_physical_block_send_to_clients_server_send_reliably(pb, &["c1".to_string()], None, false);
        assert!(host.sent.borrow().is_empty());
        // no light block database: nothing to attach, still saved and pushed
        l.light_block_database.borrow_mut().take();
        let pb = marked_block(&world, 5, 7, 8, 1);
        l.save_physical_block_send_to_clients_server_send_reliably(pb, &["c1".to_string()], Some(&server), false);
        assert_eq!(host.sent.borrow().len(), 1);
        // sending without a light block database still hands out an empty light block
        let pb = marked_block(&world, 5, 8, 8, 1);
        assert!(l
            .send_block_to_client_without_saving_for_block_pos_send_to_client_server_send_dynamic_objects_reliable(
                pb,
                IntPair { x: 5, y: 8 },
                Some("c1"),
                Some(&server),
                false,
                true,
            ));
        // a connected client that is not connected
        host.clients.borrow_mut().insert("c2".into(), (false, 1));
        assert!(!l
            .send_block_to_client_without_saving_for_block_pos_send_to_client_server_send_dynamic_objects_reliable(
                pb,
                IntPair { x: 5, y: 8 },
                Some("c2"),
                Some(&server),
                false,
                true,
            ));
        // no server or no client given
        assert!(!l
            .send_block_to_client_without_saving_for_block_pos_send_to_client_server_send_dynamic_objects_reliable(
                pb,
                IntPair { x: 5, y: 8 },
                None,
                Some(&server),
                false,
                true,
            ));
        set_host(None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The comparison itself against a reference made on the spot: blocks generated once, stored in a `world_db`,
    /// generated again from the same seed and compared.
    #[test]
    fn a_world_matches_its_own_stored_blocks() {
        let root = std::env::temp_dir().join(format!("wg_synth_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let db_dir = root.join("synth/id0/world_db");
        std::fs::create_dir_all(&db_dir).unwrap();
        let rules = rules_default(false, false);
        // first pass: generate and store a spread of blocks
        {
            let world = World::alloc(32);
            world.set_custom_rules(rules);
            let ops = Rc::new(GenOps { loader: std::cell::RefCell::new(None) });
            world.set_ops_hooks(Some(ops.clone()));
            glibc_rand::srand(1792182287);
            let l = WorldTileLoader::init_with_world_random_seed_is_new_world_save_id_loaded_version_block_database(
                &world,
                1792182287,
                true,
                "wgtest_synth",
                0,
                None,
            );
            *ops.loader.borrow_mut() = Some(l.clone());
            glibc_rand::srand(l.random_seed());
            for _ in 0..8 {
                l.refine_terrain();
            }
            let env = DatabaseEnvironment::new(db_dir.to_str().unwrap(), 8, 64).unwrap();
            let db = Database::new(&env, "blocks").unwrap();
            for (x, y) in [(3, 15), (3, 16), (10, 16), (20, 17)] {
                crate::world::tiles::load_block_at_macro_pos(IntPair { x, y }, &world);
                let mt = world.macro_tiles().get((y * 32 + x) as usize).unwrap();
                let pb = mt.physical_block().unwrap();
                let tiles: Vec<Tile> = pb.tiles.iter().map(|t| t.get()).collect();
                let plain = crate::types::tile::serialize_block_plain(&tiles, 0, 0);
                db.set_data(Some(&gzip_deflate(&plain)), Some(&format!("{x}_{y}")));
            }
        }
        // a leftover that is not a block key is skipped by the reader
        let blocks = sample_blocks_under(root.to_str().unwrap(), "synth", "").unwrap();
        assert_eq!(blocks.len(), 4);
        assert!(sample_blocks_under(root.to_str().unwrap(), "nothing", "").is_none());
        assert!(sample_blocks_under(root.to_str().unwrap(), "synth", "missing").is_none());
        compare_sample_under(root.to_str().unwrap(), "synth", "id0", 32, 1792182287, rules, false);
        let _ = std::fs::remove_dir_all(&root);
    }

    // ------------------------------------------------------------------------------------------------
    // save / send / update-version tests
    // ------------------------------------------------------------------------------------------------
    use super::super::tile_loader::set_host;
    use super::super::tile_loader::testutil::*;
    use crate::protocol::block_wire::{decode_light_block, decode_server_block};

    fn marked_block(world: &World, x: i32, y: i32, version: u8, upd: u32) -> PhysicalBlockRef {
        let pb = world.take_physical_block();
        pb.x_pos.set(x);
        pb.y_pos.set(y);
        pb.version.set(version);
        pb.update_index.set(upd);
        pb.valid.set(true);
        for (i, c) in pb.tiles.iter().enumerate() {
            let mut t = Tile::default();
            t.type_index = (i % 7) as u8 + 1;
            t.contents = (i % 5) as u8;
            c.set(t);
        }
        pb
    }

    #[test]
    fn block_payload_layout() {
        let (world, _s) = world_with_server(32);
        let pb = marked_block(&world, 3, 4, 8, 0x01020304);
        let p = WorldTileLoader::block_payload(pb);
        assert_eq!(p.len(), 65541);
        assert_eq!(&p[65536..], &[8, 4, 3, 2, 1]);
        assert_eq!(p[0], 1); // tile 0 type
        assert_eq!(p[64], 2); // tile 1 type
    }

    #[test]
    fn save_stores_pre_increment_index_and_pushes_to_peers() {
        let host = install_host();
        let dir = scratch("save");
        let (world, server) = world_with_server(32);
        let l = loader_with_dbs(&world, &dir, true);
        host.clients.borrow_mut().insert("c1".into(), (true, 3));
        host.admins.borrow_mut().insert("c1".into());
        let pb = marked_block(&world, 5, 6, 8, 41);
        l.save_physical_block_send_to_clients_server_send_reliably(pb, &["c1".to_string()], Some(&server), false);
        assert_eq!(pb.update_index.get(), 42);
        // DB record: gzip(payload) with the index BEFORE the increment
        let stored = l.block_database.as_ref().unwrap().data_for_key(Some("5_6")).unwrap();
        let plain = gzip_inflate(&stored).unwrap();
        let (_t, ver, upd) = parse_block_plain(&plain).unwrap();
        assert_eq!((ver, upd), (8, 41));
        // one reliable packet to c1
        let sent = host.sent.borrow();
        assert_eq!(sent.len(), 1);
        let (pkt, peers, reliable) = &sent[0];
        assert_eq!(peers, &vec!["c1".to_string()]);
        assert!(*reliable, "save always sends reliably");
        assert_eq!(pkt[0], 4);
        let msg = decode_server_block(&pkt[1..]).unwrap();
        assert_eq!(msg.macro_index, 6 * 32 + 5);
        assert_eq!((msg.version, msg.update_index), (8, 41));
        let extra = msg.extra.unwrap();
        assert_eq!(extra.object_for_key("hash").unwrap().as_str().unwrap(), block_wire::block_hash("c1", 5, 6, true));
        assert!(extra.object_for_key("receipts").is_none());
        // the missing light block was created zeroed
        assert!(pb.client_light_blocks.borrow()[3].is_some());
        assert_eq!(pb.client_explored.borrow()[3], 0);
        set_host(None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// : the `sc` lookup is not nil-checked; nil messaging gives light block index 0 and no receipts, so
    /// an unknown peer still gets the block saved and pushed (no crash).
    #[test]
    fn save_with_unknown_peer_uses_light_block_zero() {
        let host = install_host();
        let dir = scratch("save_unknown");
        let (world, server) = world_with_server(32);
        let l = loader_with_dbs(&world, &dir, true);
        let pb = marked_block(&world, 2, 3, 8, 7);
        l.save_physical_block_send_to_clients_server_send_reliably(pb, &["ghost".to_string()], Some(&server), false);
        assert_eq!(pb.update_index.get(), 8);
        assert!(l.block_database.as_ref().unwrap().data_for_key(Some("2_3")).is_some());
        assert!(pb.client_light_blocks.borrow()[0].is_some());
        assert_eq!(host.sent.borrow().len(), 1);
        set_host(None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn send_block_unknown_client_still_increments_index() {
        let host = install_host();
        let dir = scratch("send_unknown");
        let (world, server) = world_with_server(32);
        let l = loader_with_dbs(&world, &dir, false);
        let pb = marked_block(&world, 1, 2, 8, 9);
        let sent = l
            .send_block_to_client_without_saving_for_block_pos_send_to_client_server_send_dynamic_objects_reliable(
                pb,
                IntPair { x: 1, y: 2 },
                Some("nobody"),
                Some(&server),
                true,
                true,
            );
        assert!(!sent);
        assert_eq!(pb.update_index.get(), 10);
        assert!(host.sent.borrow().is_empty());
        // disconnected client behaves the same
        host.clients.borrow_mut().insert("gone".into(), (false, 1));
        let sent = l
            .send_block_to_client_without_saving_for_block_pos_send_to_client_server_send_dynamic_objects_reliable(
                pb,
                IntPair { x: 1, y: 2 },
                Some("gone"),
                Some(&server),
                true,
                true,
            );
        assert!(!sent);
        assert_eq!(pb.update_index.get(), 11);
        // nil server / client: NO without touching the index
        assert!(!l
            .send_block_to_client_without_saving_for_block_pos_send_to_client_server_send_dynamic_objects_reliable(
                pb,
                IntPair { x: 1, y: 2 },
                None,
                Some(&server),
                true,
                true
            ));
        assert_eq!(pb.update_index.get(), 11);
        set_host(None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn send_block_packets_and_receipts() {
        use super::super::tile_loader::DynamicObjectsNetData;
        let host = install_host();
        let dir = scratch("send_ok");
        let (world, server) = world_with_server(32);
        let l = loader_with_dbs(&world, &dir, false);
        host.clients.borrow_mut().insert("c9".into(), (true, 7));
        host.receipts.borrow_mut().insert("c9".into(), vec![3, 4]);
        *host.dyn_data.borrow_mut() = Some(DynamicObjectsNetData {
            creation: vec![
                (5, PlistValue::array(vec![PlistValue::int(1)])),
                (2, PlistValue::array(vec![PlistValue::int(2)])),
            ],
            update: vec![(9, PlistValue::array(vec![PlistValue::int(3)]))],
        });
        let pb = marked_block(&world, 4, 3, 8, 100);
        let sent = l
            .send_block_to_client_without_saving_for_block_pos_send_to_client_server_send_dynamic_objects_reliable(
                pb,
                IntPair { x: 4, y: 3 },
                Some("c9"),
                Some(&server),
                true,
                false,
            );
        assert!(sent);
        assert_eq!(pb.update_index.get(), 101);
        let s = host.sent.borrow();
        assert_eq!(s.len(), 4);
        // block packet: reliable flag as given
        assert_eq!(s[0].0[0], 4);
        assert!(!s[0].2);
        let m = decode_server_block(&s[0].0[1..]).unwrap();
        assert_eq!(m.macro_index, 3 * 32 + 4);
        assert_eq!(m.update_index, 100);
        let extra = m.extra.unwrap();
        let PlistValue::Array(a) = extra.object_for_key("receipts").unwrap() else { unreachable!() };
        let rec = a.items.iter().map(|v| v.unsigned_int_value()).collect::<Vec<_>>();
        assert_eq!(rec, vec![3, 4]);
        // dynamic objects: creation types first (in the given order), then updates, always reliable
        assert_eq!(&s[1].0[..2], &[7, 5]);
        assert_eq!(&s[2].0[..2], &[7, 2]);
        assert_eq!(&s[3].0[..2], &[8, 9]);
        assert!(s[1].2 && s[2].2 && s[3].2);
        let arr = crate::protocol::codec::gz_plist_from(&s[1].0[2..]).unwrap();
        assert!(matches!(arr, PlistValue::Array(_)));
        // the light block of the client is now present
        assert!(pb.client_light_blocks.borrow()[7].is_some());
        set_host(None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn light_block_packet_roundtrips_update_index() {
        let host = install_host();
        let dir = scratch("light_pkt");
        let (world, server) = world_with_server(32);
        let l = loader_with_dbs(&world, &dir, false);
        host.clients.borrow_mut().insert("c2".into(), (true, 1));
        let pb = marked_block(&world, 2, 2, 8, 77);
        let mut lb = Box::new([0u8; 1024]);
        lb[5] = 0xaa;
        pb.client_light_blocks.borrow_mut()[1] = Some(lb);
        assert!(l.send_light_block_to_client_without_saving_for_block_pos_send_to_client_server(
            pb,
            IntPair { x: 2, y: 2 },
            Some("c2"),
            Some(&server)
        ));
        assert_eq!(pb.update_index.get(), 78);
        let s = host.sent.borrow();
        assert_eq!(s[0].0[0], 0x28);
        assert!(!s[0].2, "light blocks are sent unreliably");
        let m = decode_light_block(&s[0].0[1..]).unwrap();
        assert_eq!(m.macro_index, 2 * 32 + 2);
        assert_eq!(m.update_index, 77);
        assert_eq!(m.light[5], 0xaa);
        // no light block loaded -> NO
        let pb2 = marked_block(&world, 3, 3, 8, 0);
        assert!(!l.send_light_block_to_client_without_saving_for_block_pos_send_to_client_server(
            pb2,
            IntPair { x: 3, y: 3 },
            Some("c2"),
            Some(&server)
        ));
        assert_eq!(pb2.update_index.get(), 0);
        set_host(None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_physical_block_dispatch_and_skip() {
        let _h = install_host();
        let dir = scratch("load_dispatch");
        let (world, _s) = world_with_server(32);
        let l = loader_with_dbs(&world, &dir, true);
        let pb = world.take_physical_block();
        // !create && unexplored -> nothing happens
        assert_eq!(
            l.load_physical_block_at_x_pos_y_pos_create_if_not_created(pb, 1, 1, false, true),
            LoadOutcome::Skipped
        );
        // !create, not unexplored, nothing stored -> NotFound
        assert_eq!(
            l.load_physical_block_at_x_pos_y_pos_create_if_not_created(pb, 1, 1, false, false),
            LoadOutcome::NotFound
        );
        // stored block is loaded; indices reset; x wrapped
        let src = marked_block(&world, 1, 1, 6, 55);
        let bytes = WorldTileLoader::block_payload(src);
        l.block_database.as_ref().unwrap().set_data(Some(&gzip_deflate(&bytes)), Some("1_1"));
        let out = l.load_physical_block_at_x_pos_y_pos_create_if_not_created(pb, 33, 1, false, false);
        assert_eq!(out, LoadOutcome::Loaded);
        assert_eq!((pb.x_pos.get(), pb.y_pos.get(), pb.version.get(), pb.update_index.get()), (1, 1, 6, 0));
        assert!(pb.valid.get());
        assert_eq!(pb.tiles[1].get().type_index, 2);
        set_host(None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_to_latest_version_runs_all_passes_on_an_old_block() {
        let host = install_host();
        let dir = scratch("update_ver");
        let world = World::alloc(32);
        world.set_custom_rules(rules_default(true, false));
        glibc_rand::srand(12345);
        let l = WorldTileLoader::init_with_world_random_seed_is_new_world_save_id_loaded_version_block_database(
            &world,
            12345,
            true,
            "wgtest_update_ver",
            0,
            None,
        );
        let _ = dir;
        let start = l.best_start_position();
        // a block around the start strip, generated version 0
        let pb = world.take_physical_block();
        let out = l.load_physical_block_at_x_pos_y_pos_create_if_not_created(pb, start.x / 32 + 1, 16, true, false);
        assert_eq!(out, LoadOutcome::Generated);
        let before: Vec<Tile> = pb.tiles.iter().map(|c| c.get()).collect();
        pb.version.set(0);
        l.update_physical_block_to_latest_version(pb);
        assert_eq!(pb.version.get(), 0, "the version byte is written by DynamicWorld, not here");
        // a freshly generated block already satisfies the passes: only ore contents may be added
        let after: Vec<Tile> = pb.tiles.iter().map(|c| c.get()).collect();
        for (a, b) in before.iter().zip(after.iter()) {
            assert_eq!((a.back_wall_type_index, a.zone_type_index), (b.back_wall_type_index, b.zone_type_index));
        }
        assert!(host.chests.borrow().iter().all(|(_, troll, treasure)| !troll && *treasure));
        set_host(None);
    }

    /// Blocks of every old version walk through exactly the passes newer than them, the start column is skipped,
    /// and beach sand over a generated rock line gets its treasure chest.
    #[test]
    fn old_blocks_get_only_the_passes_after_their_version() {
        let host = install_host();
        let world = World::alloc(32);
        world.set_custom_rules(rules_default(true, false));
        glibc_rand::srand(777);
        let l = WorldTileLoader::init_with_world_random_seed_is_new_world_save_id_loaded_version_block_database(
            &world,
            777,
            true,
            "wgtest_old_blocks",
            0,
            None,
        );
        let start = l.best_start_position();
        let p = crate::worldgen::terrain::GenParams::from_world_opt(Some(&world));
        for version in [0u8, 2, 3, 4, 5, 6, 7, 8] {
            let pb = world.take_physical_block();
            let out = l.load_physical_block_at_x_pos_y_pos_create_if_not_created(pb, start.x / 32, 16, true, false);
            assert_eq!(out, LoadOutcome::Generated);
            pb.version.set(version);
            l.update_physical_block_to_latest_version(pb);
        }
        // chests: put sand on the rock line of every column of the blocks around the surface and see who answers
        'search: for y_pos in 14..20 {
            for x_pos in 0..32 {
                let pb = world.take_physical_block();
                let out = l.load_physical_block_at_x_pos_y_pos_create_if_not_created(pb, x_pos, y_pos, true, false);
                assert_eq!(out, LoadOutcome::Generated);
                for x in 0..32 {
                    let (r, _d) = l.t.get_rock_and_dirt_height(&p, x_pos * 32 + x);
                    let lr = r - y_pos * 32;
                    if (0..32).contains(&lr) {
                        let cell = &pb.tiles[(lr * 32 + x) as usize];
                        let mut t = cell.get();
                        t.contents = 0;
                        t.type_index = tt::SAND_BEACH;
                        cell.set(t);
                    }
                }
                pb.version.set(0);
                l.update_physical_block_to_latest_version(pb);
                if !host.chests.borrow().is_empty() {
                    break 'search;
                }
            }
        }
        assert!(!host.chests.borrow().is_empty(), "a beach chest was placed");
        set_host(None);
    }

    /// A host that throws the light block away while the block is being sent or saved: the packet is not sent.
    #[test]
    fn light_blocks_that_vanish_while_loading_stop_the_send() {
        let host = install_host();
        let dir = scratch("light_vanish");
        let (world, server) = world_with_server(32);
        let l = loader_with_dbs(&world, &dir, true);
        host.clients.borrow_mut().insert("c1".into(), (true, 3));
        // the start portal lies in this block, so creating its zeroed light block fully loads around it
        world.set_start_portal_pos(IntPair { x: 5 * 32 + 3, y: 6 * 32 + 1 });
        let pb = marked_block(&world, 5, 6, 8, 41);
        *host.clobber.borrow_mut() = Some(pb);
        let sent = l
            .send_block_to_client_without_saving_for_block_pos_send_to_client_server_send_dynamic_objects_reliable(
                pb,
                IntPair { x: 5, y: 6 },
                Some("c1"),
                Some(&server),
                false,
                true,
            );
        assert!(!sent);
        assert!(host.sent.borrow().is_empty());
        // saving does the same lookup: the block is stored, nothing is pushed to the peer
        l.save_physical_block_send_to_clients_server_send_reliably(pb, &["c1".to_string()], Some(&server), false);
        assert!(l.block_database.as_ref().unwrap().data_for_key(Some("5_6")).is_some());
        *host.clobber.borrow_mut() = None;
        // a server without any peer to push to
        l.save_physical_block_send_to_clients_server_send_reliably(pb, &[], Some(&server), false);
        set_host(None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
