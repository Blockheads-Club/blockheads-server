//! owned by worldgen
//!
//! Per-client light blocks: the `lightBlocks` LMDB store, archive / unarchive of a client's blocks, and the
//! 0x28 (`BHNDT_SERVER_LIGHT_BLOCK`) packet.
//!
//!  (archive aborts in `enumerateIndexesUsingBlock:` on the reference).

use std::rc::Rc;

use crate::foundation::exception::gnustep_block_abort;
use crate::foundation::gzip::{gzip_deflate, gzip_inflate};
use crate::foundation::{clock, paths};
use crate::nslog;
use crate::plist::helpers::{data_from_property_list, property_list_from_data};
use crate::plist::keyed_archiver::unarchive_index_set;
use crate::plist::value::PlistValue;
use crate::protocol::block_wire;
use crate::types::structs::IntPair;
use crate::world::state::{BHServer, PhysicalBlockRef};
use crate::world::tiles::{macro_index_at_macro_position, macro_pos_for_macro_index};

use super::tile_loader::{host, WorldTileLoader};

impl WorldTileLoader {
    /// -[WorldTileLoader unarchiveLightBlocksForClient:]. Leaves the lightBlocks bulk transaction open
    /// (R8, original behaviour; `World preUpdate` commits it).
    pub fn unarchive_light_blocks_for_client(&self, client_id: &str) {
        let db = match self.light_block_database() {
            Some(d) => d,
            None => return,
        };
        let index_key = format!("{}_archiveKeys", client_id);
        let index_data = match db.data_for_key(Some(&index_key)).and_then(|d| gzip_inflate(&d)) {
            Some(d) => d,
            None => return,
        };
        let t0 = clock::reference_time();
        let world = self.world.upgrade();
        let h = host();
        let has_sc = match world.as_deref() {
            Some(w) => h.client_info(w, client_id).is_some(),
            None => false,
        };
        if has_sc {
            let w = world.as_deref().unwrap();
            self.start_bulk_light_block_transaction();
            let tile_size = 0x400usize;
            let index_array: Vec<String> = match property_list_from_data(&index_data) {
                Some(PlistValue::Array(a)) => {
                    a.items.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
                }
                _ => Vec::new(),
            };
            let tile_key = format!("{}_archiveData", client_id);
            let tiles_data = db.data_for_key(Some(&tile_key)).and_then(|d| gzip_inflate(&d));
            if let Some(tiles_data) = tiles_data {
                if tiles_data.len() == tile_size * index_array.len() {
                    let mut count = 0usize;
                    for pos_key in &index_array {
                        let parts: Vec<&str> = pos_key.split('_').collect();
                        if parts.len() == 2 {
                            let key = format!("{}_{}", client_id, pos_key);
                            if !db.has_key(Some(&key)) {
                                db.set_data(Some(&tiles_data[tile_size * count..tile_size * (count + 1)]), Some(&key));
                            }
                            count += 1;
                            let mx = crate::foundation::nsstring::integer_value(parts[0]) as i32;
                            let my = crate::foundation::nsstring::integer_value(parts[1]) as i32;
                            h.client_add_light_block_index(
                                w,
                                client_id,
                                macro_index_at_macro_position(mx, my, w) as u32 as u64,
                            );
                        }
                    }
                    h.client_save_light_block_indices(w, client_id);
                    if count == index_array.len() {
                        nslog!("unarchived {} LightBlocks for client:{}", count, client_id);
                    }
                }
            }
            db.remove_data_for_key(Some(&index_key));
            db.remove_data_for_key(Some(&tile_key));
        }
        nslog!("unarchiveTime: {:.4}", clock::reference_time() - t0);
    }

    /// -[WorldTileLoader archiveLightBlocksForClient:]. With feature `faithful-crashes` (default)
    /// the reference aborts in `enumerateIndexesUsingBlock:` as soon as `<id>_allIndexes` decodes to a set
    ///; the specified archive is kept for builds without the feature.
    pub fn archive_light_blocks_for_client(&self, client_id: &str) {
        nslog!("archiveLightBlocksForClient:{}", client_id);
        self.unarchive_light_blocks_for_client(client_id);
        let db = match self.light_block_database() {
            Some(d) => d,
            None => return,
        };
        let saved = match db.data_for_key(Some(&format!("{}_allIndexes", client_id))) {
            Some(s) => s,
            None => return,
        };
        let set = match unarchive_index_set(&saved, "all") {
            Some(s) => s,
            None => return,
        };
        if cfg!(feature = "faithful-crashes") {
            gnustep_block_abort();
        }
        self.archive_light_block_indices(client_id, set.iter());
    }

    /// The archive body of for an already decoded index set: moves each existing per-block record of
    /// the client into `<id>_archiveData` (gzip of the concatenated 0x400 blocks) and `<id>_archiveKeys` (gzip of
    /// the XML plist array of `"x_y"` strings), removes `<id>_allIndexes` and logs. Split out so it is testable
    /// while the `faithful-crashes` abort sits in front of it.
    pub(crate) fn archive_light_block_indices(&self, client_id: &str, indices: impl Iterator<Item = u64>) {
        let db = match self.light_block_database() {
            Some(d) => d,
            None => return,
        };
        let world = match self.world.upgrade() {
            Some(w) => w,
            None => return,
        };
        let t0 = clock::reference_time();
        let mut index_array: Vec<PlistValue> = Vec::new();
        let mut block_data: Vec<u8> = Vec::new();
        let mut counter = 0usize;
        // block body of 2.7, once per index
        for idx in indices {
            let mp = macro_pos_for_macro_index(idx as i32, &world);
            let pos_key = format!("{}_{}", mp.x, mp.y);
            let db_key = format!("{}_{}", client_id, pos_key);
            match db.data_for_key(Some(&db_key)) {
                None => nslog!("Couldn't find expected light block at key:{}", db_key),
                Some(data) => {
                    index_array.push(PlistValue::string(pos_key));
                    let mut chunk = data;
                    chunk.resize(0x400, 0);
                    block_data.extend_from_slice(&chunk[..0x400]);
                    counter += 1;
                    db.remove_data_for_key(Some(&db_key));
                }
            }
        }
        if counter > 0 {
            let data_blob = gzip_deflate(&block_data);
            let keys_blob = data_from_property_list(&PlistValue::array(index_array)).map(|d| gzip_deflate(&d));
            db.set_data(Some(&data_blob), Some(&format!("{}_archiveData", client_id)));
            if let Some(k) = keys_blob {
                db.set_data(Some(&k), Some(&format!("{}_archiveKeys", client_id)));
            }
            db.remove_data_for_key(Some(&format!("{}_allIndexes", client_id)));
            nslog!("archived {} LightBlocksForClient:{}", counter, client_id);
            nslog!("archive time: {:.4}", clock::reference_time() - t0);
        }
    }

    /// -[WorldTileLoader loadLightBlockForClientLightBlockIndex:clientID:intoPhysicalBlock:]. Per-block
    /// values are raw 0x400 bytes (R10). A missing block is created zeroed and, at the portal, triggers
    /// `fullyLoadIfNeededAroundPos`.
    pub fn load_light_block_for_client_light_block_index_client_id_into_physical_block(
        &self,
        i: i32,
        client_id: &str,
        pb: PhysicalBlockRef,
    ) {
        let world = self.world.upgrade();
        let x_pos = pb.x_pos.get();
        let y_pos = pb.y_pos.get();
        let mut tiles_data: Option<Vec<u8>> = self
            .light_block_database()
            .and_then(|db| db.data_for_key(Some(&format!("{}_{}_{}", client_id, x_pos, y_pos))));
        if tiles_data.is_none() && !world.as_deref().map(|w| w.has_finished_database_migration_to17()).unwrap_or(false)
        {
            // legacy file layout
            use crate::foundation::nsstring::{length, substring_with_range};
            let len = length(client_id);
            if len >= 2 {
                let c0 = substring_with_range(client_id, 0, 1);
                let c1 = substring_with_range(client_id, 1, 1);
                let rest = substring_with_range(client_id, 2, len - 2);
                let name = format!("playerLightBlocks/{}/{}/{}/{}_{}_lightBlock", c0, c1, rest, x_pos, y_pos);
                let path = paths::append_path_component(&self.block_directory, &name);
                if std::path::Path::new(&path).exists() {
                    tiles_data = std::fs::read(&path).ok().and_then(|raw| gzip_inflate(&raw));
                }
            }
        }
        let idx = i as usize;
        match tiles_data {
            None => {
                pb.client_light_blocks.borrow_mut()[idx] = Some(Box::new([0u8; 1024]));
                pb.client_explored.borrow_mut()[idx] = 0;
                if let Some(w) = world.as_deref() {
                    let portal = w.start_portal_pos();
                    if x_pos * 32 <= portal.x
                        && portal.x < (x_pos + 1) * 32
                        && y_pos * 32 <= portal.y
                        && portal.y < (y_pos + 1) * 32
                    {
                        host().fully_load_if_needed_around_pos(w, portal, i);
                    }
                }
            }
            Some(d) => {
                // The original copies `[tilesData length]` bytes unchecked (stored blobs are always 0x400).
                debug_assert_eq!(d.len(), 0x400, "light block value must be 0x400 bytes");
                let mut b = Box::new([0u8; 1024]);
                let n = d.len().min(1024);
                b[..n].copy_from_slice(&d[..n]);
                pb.client_light_blocks.borrow_mut()[idx] = Some(b);
                pb.client_explored.borrow_mut()[idx] = 1;
            }
        }
    }

    /// -[WorldTileLoader sendLightBlockToClientWithoutSavingForBlock:pos:sendToClient:server:]. Packet
    /// `[0x28] ++ i32_le(y*W + x) ++ gzip(light ++ update_index_le)`, unreliable; `update_index += 1` after reading.
    pub fn send_light_block_to_client_without_saving_for_block_pos_send_to_client_server(
        &self,
        pb: PhysicalBlockRef,
        pos: IntPair,
        client: Option<&str>,
        server: Option<&Rc<BHServer>>,
    ) -> bool {
        let client = match client {
            Some(c) => c,
            None => return false,
        };
        let world = match self.world.upgrade() {
            Some(w) => w,
            None => return false,
        };
        let h = host();
        let sc = h.client_info(&world, client);
        let lbi = sc.map(|(_, li)| li).unwrap_or(0) as usize;
        if let (Some((true, _)), Some(server)) = (sc, server) {
            let light: Option<[u8; 1024]> = pb.client_light_blocks.borrow()[lbi].as_ref().map(|b| **b);
            if let Some(light) = light {
                let ui = pb.update_index.get();
                pb.update_index.set(ui.wrapping_add(1));
                let index = pos.y.wrapping_mul(world.world_width_macro()).wrapping_add(pos.x);
                let msg = block_wire::light_block(index, &light, ui);
                h.send_network_data_to_peers_reliable(server, &msg, &[client.to_string()], false);
                return true;
            }
        }
        false
    }

    /// -[WorldTileLoader saveLightBlockForClientLightBlockIndex:clientID:physicalBlock:sendNow:].
    pub fn save_light_block_for_client_light_block_index_client_id_physical_block_send_now(
        &self,
        i: i32,
        client_id: &str,
        pb: PhysicalBlockRef,
        send_now: bool,
    ) {
        let light: [u8; 1024] = match pb.client_light_blocks.borrow()[i as usize].as_ref() {
            Some(b) => **b,
            None => return,
        };
        let world = match self.world.upgrade() {
            Some(w) => w,
            None => return,
        };
        let h = host();
        if h.client_info(&world, client_id).is_none() {
            return;
        }
        let x_pos = pb.x_pos.get();
        let y_pos = pb.y_pos.get();
        if let Some(db) = self.light_block_database() {
            db.set_data(Some(&light), Some(&format!("{}_{}_{}", client_id, x_pos, y_pos)));
        }
        let mi = macro_index_at_macro_position(x_pos, y_pos, &world) as u32 as u64;
        if !h.client_contains_light_block_index(&world, client_id, mi) {
            h.client_add_light_block_index(&world, client_id, mi);
            h.client_save_light_block_indices(&world, client_id);
        }
        if send_now {
            let server = world.server();
            self.send_light_block_to_client_without_saving_for_block_pos_send_to_client_server(
                pb,
                IntPair { x: x_pos, y: y_pos },
                Some(client_id),
                server.as_ref(),
            );
        }
    }

    /// -[WorldTileLoader startBulkLightBlockTransaction].
    pub fn start_bulk_light_block_transaction(&self) {
        if let Some(env) = self.light_block_database_environment.borrow().as_ref() {
            env.start_bulk_transaction();
        }
    }

    /// -[WorldTileLoader finishBulkLightBlockTransaction]. A nil environment answers NO.
    pub fn finish_bulk_light_block_transaction(&self) -> bool {
        match self.light_block_database_environment.borrow().as_ref() {
            Some(env) => env.finish_bulk_transaction(),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tile_loader::testutil::*;
    use super::*;

    #[test]
    fn archive_then_unarchive_restores_per_block_records() {
        let host = install_host();
        let dir = scratch("archive");
        let (world, _server) = world_with_server(32);
        let l = loader_with_dbs(&world, &dir, false);
        let db = l.light_block_database().unwrap();
        // three per-block records of client "abc"
        let positions = [(1, 2), (3, 4), (31, 31)];
        for (i, (x, y)) in positions.iter().enumerate() {
            db.set_data(Some(&vec![i as u8 + 1; 0x400]), Some(&format!("abc_{}_{}", x, y)));
        }
        let idx: Vec<u64> = positions.iter().map(|(x, y)| (y * 32 + x) as u64).collect();
        l.archive_light_block_indices("abc", idx.iter().copied());
        for (x, y) in positions {
            assert!(!db.has_key(Some(&format!("abc_{}_{}", x, y))));
        }
        let keys = gzip_inflate(&db.data_for_key(Some("abc_archiveKeys")).unwrap()).unwrap();
        let arr = property_list_from_data(&keys).unwrap();
        let names: Vec<_> = match &arr {
            PlistValue::Array(a) => a.items.iter().map(|v| v.as_str().unwrap().to_string()).collect(),
            _ => panic!(),
        };
        assert_eq!(names, vec!["1_2", "3_4", "31_31"]);
        let data = gzip_inflate(&db.data_for_key(Some("abc_archiveData")).unwrap()).unwrap();
        assert_eq!(data.len(), 3 * 0x400);
        assert_eq!((data[0], data[0x400], data[0x800]), (1, 2, 3));

        // no ServerClient: unarchive leaves everything in place (only logs the time)
        l.unarchive_light_blocks_for_client("abc");
        assert!(db.has_key(Some("abc_archiveKeys")));

        host.clients.borrow_mut().insert("abc".into(), (true, 0));
        l.unarchive_light_blocks_for_client("abc");
        assert!(!db.has_key(Some("abc_archiveKeys")) && !db.has_key(Some("abc_archiveData")));
        for (i, (x, y)) in positions.iter().enumerate() {
            let v = db.data_for_key(Some(&format!("abc_{}_{}", x, y))).unwrap();
            assert_eq!(v.len(), 0x400);
            assert!(v.iter().all(|&b| b == i as u8 + 1));
        }
        assert_eq!(host.indices.borrow().len(), 3);
        assert!(host.indices.borrow().contains(&("abc".to_string(), 4 * 32 + 3)));
        assert_eq!(host.saved_indices.borrow().as_slice(), &["abc".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unarchive_with_mismatched_size_still_removes_archive_keys() {
        let host = install_host();
        let dir = scratch("archive_bad");
        let (world, _server) = world_with_server(32);
        let l = loader_with_dbs(&world, &dir, false);
        let db = l.light_block_database().unwrap();
        host.clients.borrow_mut().insert("zz".into(), (true, 0));
        let keys =
            data_from_property_list(&PlistValue::array(vec![PlistValue::string("1_1"), PlistValue::string("2_2")]))
                .unwrap();
        db.set_data(Some(&gzip_deflate(&keys)), Some("zz_archiveKeys"));
        db.set_data(Some(&gzip_deflate(&vec![9u8; 0x400])), Some("zz_archiveData")); // one block, two keys
        l.unarchive_light_blocks_for_client("zz");
        assert!(!db.has_key(Some("zz_archiveKeys")) && !db.has_key(Some("zz_archiveData")));
        assert!(!db.has_key(Some("zz_1_1")));
        assert!(host.indices.borrow().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_light_block_creates_zeroed_block_and_loads_portal() {
        let host = install_host();
        let dir = scratch("load_light");
        let (world, _server) = world_with_server(32);
        world.set_start_portal_pos(IntPair { x: 5 * 32 + 3, y: 6 * 32 + 1 });
        let l = loader_with_dbs(&world, &dir, false);
        let pb = world.take_physical_block();
        pb.x_pos.set(5);
        pb.y_pos.set(6);
        l.load_light_block_for_client_light_block_index_client_id_into_physical_block(4, "c", pb);
        assert!(pb.client_light_blocks.borrow()[4].as_ref().unwrap().iter().all(|&b| b == 0));
        assert_eq!(pb.client_explored.borrow()[4], 0);
        assert_eq!(host.fully_loaded.borrow().as_slice(), &[(IntPair { x: 5 * 32 + 3, y: 6 * 32 + 1 }, 4)]);
        // a stored record is loaded raw and marks the block explored
        l.light_block_database().unwrap().set_data(Some(&vec![7u8; 0x400]), Some("c_5_6"));
        l.load_light_block_for_client_light_block_index_client_id_into_physical_block(2, "c", pb);
        assert!(pb.client_light_blocks.borrow()[2].as_ref().unwrap().iter().all(|&b| b == 7));
        assert_eq!(pb.client_explored.borrow()[2], 1);
        assert_eq!(host.fully_loaded.borrow().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_light_block_registers_index_once_and_sends() {
        let host = install_host();
        let dir = scratch("save_light");
        let (world, _server) = world_with_server(32);
        let l = loader_with_dbs(&world, &dir, false);
        host.clients.borrow_mut().insert("c".into(), (true, 2));
        let pb = world.take_physical_block();
        pb.x_pos.set(7);
        pb.y_pos.set(8);
        // nothing loaded: no-op
        l.save_light_block_for_client_light_block_index_client_id_physical_block_send_now(2, "c", pb, true);
        assert!(!l.light_block_database().unwrap().has_key(Some("c_7_8")));
        pb.client_light_blocks.borrow_mut()[2] = Some(Box::new([3u8; 1024]));
        l.save_light_block_for_client_light_block_index_client_id_physical_block_send_now(2, "c", pb, false);
        l.save_light_block_for_client_light_block_index_client_id_physical_block_send_now(2, "c", pb, false);
        assert_eq!(l.light_block_database().unwrap().data_for_key(Some("c_7_8")).unwrap(), vec![3u8; 1024]);
        assert_eq!(host.saved_indices.borrow().len(), 1, "saveLightBlockIndices only on the first add");
        assert!(host.sent.borrow().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(not(feature = "faithful-crashes"))]
    #[test]
    fn archive_with_all_indexes_follows_the_spec_without_the_abort_feature() {
        let _host = install_host();
        let dir = scratch("archive_full");
        let (world, _server) = world_with_server(32);
        let l = loader_with_dbs(&world, &dir, false);
        let db = l.light_block_database().unwrap();
        let mut set = crate::foundation::indexset::IndexSet::new();
        set.add_index(33);
        db.set_data(Some(&crate::plist::keyed_archiver::archive_index_set(&set, "all")), Some("q_allIndexes"));
        db.set_data(Some(&vec![1u8; 0x400]), Some("q_1_1"));
        l.archive_light_blocks_for_client("q");
        assert!(db.has_key(Some("q_archiveData")) && !db.has_key(Some("q_allIndexes")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod more_tests {
    use super::super::tile_loader::testutil::*;
    use super::*;
    use crate::plist::keyed_archiver::archive_index_set;
    use super::super::tile_loader::set_host;

    fn marked(world: &crate::world::state::World, x: i32, y: i32) -> PhysicalBlockRef {
        let pb = world.take_physical_block();
        pb.x_pos.set(x);
        pb.y_pos.set(y);
        pb
    }

    #[test]
    fn nothing_happens_without_a_light_database_or_a_world() {
        let host = install_host();
        let dir = scratch("la_none");
        let (world, server) = world_with_server(32);
        let l = loader_with_dbs(&world, &dir, false);
        let pb = marked(&world, 1, 1);
        // no light block yet: nothing to save
        l.save_light_block_for_client_light_block_index_client_id_physical_block_send_now(2, "c", pb, false);
        pb.client_light_blocks.borrow_mut()[2] = Some(Box::new([5u8; 1024]));
        // an unknown client
        l.save_light_block_for_client_light_block_index_client_id_physical_block_send_now(2, "c", pb, false);
        host.clients.borrow_mut().insert("c".into(), (true, 2));
        // saved, announced to the client, and sent at once
        l.save_light_block_for_client_light_block_index_client_id_physical_block_send_now(2, "c", pb, true);
        assert_eq!(host.sent.borrow().len(), 1);
        // known index: not announced again
        l.save_light_block_for_client_light_block_index_client_id_physical_block_send_now(2, "c", pb, false);
        assert_eq!(host.saved_indices.borrow().len(), 1);
        // sending: no client, a client that is not connected, no light block
        assert!(!l.send_light_block_to_client_without_saving_for_block_pos_send_to_client_server(
            pb,
            IntPair { x: 1, y: 1 },
            None,
            Some(&server)
        ));
        // the bulk transaction of the light database
        l.start_bulk_light_block_transaction();
        assert!(l.finish_bulk_light_block_transaction());
        l.light_block_database_environment.borrow_mut().take();
        assert!(!l.finish_bulk_light_block_transaction());
        l.start_bulk_light_block_transaction();
        // without a database
        l.light_block_database.borrow_mut().take();
        l.unarchive_light_blocks_for_client("c");
        l.archive_light_blocks_for_client("c");
        l.archive_light_block_indices("c", [1u64].into_iter());
        l.save_light_block_for_client_light_block_index_client_id_physical_block_send_now(2, "c", pb, false);
        set_host(None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_world_that_is_gone_ends_every_operation() {
        let host = install_host();
        let dir = scratch("la_gone");
        let (world, server) = world_with_server(32);
        let l = loader_with_dbs(&world, &dir, false);
        let db = l.light_block_database().unwrap();
        db.set_data(Some(&gzip_deflate(b"x")), Some("c_archiveKeys"));
        let pb = marked(&world, 1, 1);
        pb.client_light_blocks.borrow_mut()[0] = Some(Box::new([0u8; 1024]));
        host.clients.borrow_mut().insert("c".into(), (true, 0));
        drop(world);
        l.unarchive_light_blocks_for_client("c");
        l.archive_light_block_indices("c", [1u64].into_iter());
        l.save_light_block_for_client_light_block_index_client_id_physical_block_send_now(0, "c", pb, false);
        assert!(!l.send_light_block_to_client_without_saving_for_block_pos_send_to_client_server(
            pb,
            IntPair { x: 1, y: 1 },
            Some("c"),
            Some(&server)
        ));
        l.load_light_block_for_client_light_block_index_client_id_into_physical_block(1, "c", pb);
        set_host(None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn archives_that_do_not_fit_are_dropped_and_missing_blocks_logged() {
        let host = install_host();
        let dir = scratch("la_odd");
        let (world, _server) = world_with_server(32);
        let l = loader_with_dbs(&world, &dir, false);
        let db = l.light_block_database().unwrap();
        host.clients.borrow_mut().insert("c".into(), (true, 0));
        // keys that are no list, with data of any size
        let keys = |v: PlistValue| gzip_deflate(&data_from_property_list(&v).unwrap());
        db.set_data(Some(&keys(PlistValue::int(3))), Some("c_archiveKeys"));
        l.unarchive_light_blocks_for_client("c");
        assert!(!db.has_key(Some("c_archiveKeys")));
        // a list whose data has the wrong size, then one that fits but contains a key that exists already
        db.set_data(Some(&keys(PlistValue::array(vec![PlistValue::string("1_2"), PlistValue::string("bad")]))), Some("c_archiveKeys"));
        db.set_data(Some(&gzip_deflate(&[1u8; 5])), Some("c_archiveData"));
        l.unarchive_light_blocks_for_client("c");
        db.set_data(Some(&keys(PlistValue::array(vec![PlistValue::string("1_2"), PlistValue::string("bad")]))), Some("c_archiveKeys"));
        db.set_data(Some(&gzip_deflate(&[1u8; 0x400])), Some("c_archiveData"));
        db.set_data(Some(&[9u8; 0x400]), Some("c_1_2"));
        l.unarchive_light_blocks_for_client("c");
        assert_eq!(db.data_for_key(Some("c_1_2")).unwrap()[0], 9);
        // archiving a block that is not stored
        l.archive_light_block_indices("c", [999u64].into_iter());
        // the saved index set of the client decodes: the archive runs (unless the reference crash is built in)
        let mut set = crate::foundation::indexset::IndexSet::new();
        set.add_index(2 * 32 + 1);
        db.set_data(Some(&archive_index_set(&set, "all")), Some("c_allIndexes"));
        db.set_data(Some(&[4u8; 0x400]), Some("c_1_2"));
        if !cfg!(feature = "faithful-crashes") {
            l.archive_light_blocks_for_client("c");
            assert!(db.has_key(Some("c_archiveData")));
        }
        db.set_data(Some(b"not an archive"), Some("d_allIndexes"));
        l.archive_light_blocks_for_client("d");
        // a legacy light block file
        std::fs::create_dir_all(dir.join("blocks/playerLightBlocks/e/f/ghi")).unwrap();
        std::fs::write(dir.join("blocks/playerLightBlocks/e/f/ghi/2_3_lightBlock"), gzip_deflate(&[7u8; 0x400])).unwrap();
        let pb = marked(&world, 2, 3);
        l.load_light_block_for_client_light_block_index_client_id_into_physical_block(1, "efghi", pb);
        assert_eq!(pb.client_light_blocks.borrow()[1].as_ref().unwrap()[0], 7);
        set_host(None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
