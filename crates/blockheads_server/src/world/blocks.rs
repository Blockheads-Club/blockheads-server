//! `world_ops` blocks: physical-block lifecycle (load / save / decommission), light blocks, `fullyLoad*`,
//! `unmodifiedGroundLevelForX:` and the database wrappers.
//!
//!
//! This file also provides the [`WorldOpsHooks`] implementation through which world_state reaches the block
//! loader and the protection check ([`install_ops_hooks`]).

use std::rc::Rc;

use super::misc::{deps, ub_null_deref};
use super::state::{MacroTileRef, PhysicalBlockRef, World, WorldOpsHooks};
use super::tiles::{macro_tile_at_macro_postion, macro_tile_at_world_postion, tile_at_world_position_loaded};
use crate::foundation::clock;
use crate::objects::handle::ObjRef;
use crate::types::structs::IntPair;

/// `a == b` on `MacroTile*` (NULL == NULL).
fn same_mt(a: &Option<MacroTileRef>, b: &Option<MacroTileRef>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

impl World {
    /// `-[World decommisionAllBlocksBlockToSavePhyscialBlock:]`.
    pub fn decommision_all_blocks_block_to_save_physcial_block(&self, block_to_save: bool) {
        let d = deps();
        let mut erase: Vec<PhysicalBlockRef> = Vec::new();
        for pb in self.used_physical_blocks() {
            if pb.valid.get() {
                let mt = macro_tile_at_macro_postion(pb.x_pos.get(), pb.y_pos.get(), &self.macro_tiles(), self);
                d.check_if_macro_tile_can_be_decommissioned_world_min_age_block_to_save_physcial_block(
                    mt.as_ref(),
                    -1.0f64,
                    block_to_save,
                );
            }
            // re-tested after the call: it may have invalidated pb
            if !pb.valid.get() {
                erase.push(pb);
            }
        }
        self.move_to_free_physical_blocks(&erase);
    }

    /// `-[World savePhysicalBlockForMacroTile:sendReliably:dontSend:onlySaveIfClientsNeedIt:]`. Returns `true`
    /// when nothing was done (no block) and `false` only for `onlySaveIfClientsNeedIt` with no recipients.
    pub fn save_physical_block_for_macro_tile_send_reliably_dont_send_only_save_if_clients_need_it(
        &self,
        mt: &MacroTileRef,
        send_reliably: bool,
        dont_send: bool,
        only_save_if_clients_need_it: bool,
    ) -> bool {
        let Some(pb) = mt.physical_block() else { return true };
        let d = deps();
        let mut clients_to_send: Vec<String> = Vec::new();
        if !dont_send && self.server().is_some() && self.server_clients_count() != 0 {
            let macro_index = pb.y_pos.get().wrapping_mul(self.world_width_macro).wrapping_add(pb.x_pos.get());
            for peer in self.server_clients_keys_in_order() {
                if d.sc_block_is_wired(&peer, macro_index) && d.sc_connected(&peer) {
                    clients_to_send.push(peer);
                }
            }
        }
        if only_save_if_clients_need_it && clients_to_send.is_empty() {
            return false;
        }
        if self.world_tile_loader.borrow().is_some() {
            d.save_physical_block_macro_tile_send_to_clients_server_send_reliably(
                pb,
                mt,
                &clients_to_send,
                send_reliably,
            );
        }
        true
    }

    /// `-[World decommisionBlock:blockToSavePhyscialBlock:]`: evicts the macro tile's block (the block is NOT moved
    /// to the free list here). Dereferences `physicalBlock`: a missing block is UB-PARITY (SIGSEGV).
    pub fn decommision_block_block_to_save_physcial_block(&self, mt: &MacroTileRef, block_to_save: bool) {
        if mt.dynamic_objects_loaded.get() {
            deps().remove_dynamic_objects_for_macro_tile(mt);
        }
        if block_to_save {
            self.save_physical_block_for_macro_tile_send_reliably_dont_send_only_save_if_clients_need_it(
                mt, true, false, false,
            );
        }
        match mt.physical_block() {
            // UB-PARITY
            None => ub_null_deref("decommisionBlock:blockToSavePhyscialBlock:"),
            Some(pb) => pb.valid.set(false),
        }
        *mt.physical_block.borrow_mut() = None;
    }

    /// `-[World loadDynamicObjectsIfNotAlreadyLoadedForMacroTile:includeSurfaceBlocks:]`.
    pub fn load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(
        &self,
        mt: &MacroTileRef,
        include_surface_blocks: bool,
    ) {
        if !mt.dynamic_objects_loaded.get() && mt.physical_block().is_some() {
            mt.dynamic_objects_loaded.set(true);
            if mt.dynamic_objects.borrow().is_none() {
                *mt.dynamic_objects.borrow_mut() = Some(Vec::new());
            }
            deps().load_dynamic_objects_for_macro_tile_include_surface_blocks(mt, include_surface_blocks);
        }
    }

    /// The client whose `lightBlockIndex == idx` (first in enumeration order).
    fn client_for_light_block_index(&self, idx: i32) -> Option<String> {
        let d = deps();
        self.server_clients_keys_in_order().into_iter().find(|k| d.sc_light_block_index(k) == idx)
    }

    /// `-[World loadLightBlockForClientLightBlockIndex:intoPhysicalBlock:]`.
    pub fn load_light_block_for_client_light_block_index_into_physical_block(
        &self,
        client_light_block_index: i32,
        pb: &PhysicalBlockRef,
    ) {
        if self.world_tile_loader.borrow().is_some() && self.server().is_some() && client_light_block_index != -1 {
            if let Some(id) = self.client_for_light_block_index(client_light_block_index) {
                deps().load_light_block_for_client_light_block_index_client_id_into_physical_block(
                    client_light_block_index,
                    &id,
                    *pb,
                );
            }
        }
    }

    /// `-[World saveLightBlockForClientLightBlockIndex:physicalBlock:sendNow:]`.
    pub fn save_light_block_for_client_light_block_index_physical_block_send_now(
        &self,
        client_light_block_index: i32,
        pb: &PhysicalBlockRef,
        send_now: bool,
    ) {
        if self.world_tile_loader.borrow().is_some() && self.server().is_some() && client_light_block_index != -1 {
            if let Some(id) = self.client_for_light_block_index(client_light_block_index) {
                deps().save_light_block_for_client_light_block_index_client_id_physical_block_send_now(
                    client_light_block_index,
                    &id,
                    *pb,
                    send_now,
                );
            }
        }
    }

    /// `-[World loadPhysicalBlockForMacroTile:atX:y:loadSurroundingBlocks:createIfNotCreated:]`. The macro tile's
    /// `physicalBlock` / `unexplored` / `dynamicObjectsLoaded` / `valid` are assigned inside the tile loader
    /// not here. Returns `mt.physical_block.is_some()` afterwards (addition for object_model).
    pub fn load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
        &self,
        mt: &MacroTileRef,
        x: i32,
        y: i32,
        load_surrounding_blocks: bool,
        create_if_not_created: bool,
    ) -> bool {
        if mt.physical_block().is_none() || mt.dirty.get() != 0 {
            // clientTileLoader is nil on the server: no request path
            let pb = match mt.physical_block() {
                Some(pb) => pb,
                None => self.take_physical_block(),
            };
            if self.world_tile_loader.borrow().is_some() {
                let d = deps();
                d.load_physical_block_at_x_pos_y_pos_create_if_not_created(pb, x, y, create_if_not_created);
                if pb.valid.get() {
                    d.add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(x, y);
                }
            }
            mt.time_physical_block_created.set(clock::reference_time() as f32);
        }
        if load_surrounding_blocks {
            let neighbours = [
                (x - 1, y + 1),
                (x, y + 1),
                (x + 1, y + 1),
                (x - 1, y),
                (x + 1, y),
                (x - 1, y - 1),
                (x, y - 1),
                (x + 1, y - 1),
            ];
            for (nx, ny) in neighbours {
                if let Some(n) = macro_tile_at_macro_postion(nx, ny, &self.macro_tiles(), self) {
                    self.load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
                        &n,
                        nx,
                        ny,
                        false,
                        create_if_not_created,
                    );
                }
            }
        }
        mt.physical_block().is_some()
    }

    /// `-[World fullyLoadAndUpdateIfNeededForMacroBlock:includingPos:clientLightBlockIndex:forBlockhead:]`.
    pub fn fully_load_and_update_if_needed_for_macro_block_including_pos_client_light_block_index_for_blockhead(
        &self,
        mt: Option<&MacroTileRef>,
        pos: IntPair,
        client_light_block_index: i32,
        bh: Option<&ObjRef>,
    ) {
        if let Some(mt) = mt {
            if mt.physical_block().is_none() {
                // loads the block as a side effect
                tile_at_world_position_loaded(pos.x, pos.y, self);
            }
            deps().recalculate_lighting_for_physical_block_if_needed_world_client_light_block_index_for_blockhead(
                mt.physical_block(),
                client_light_block_index,
                bh,
            );
            self.load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(mt, true);
        }
    }

    /// `-[World fullyLoadIfNeededAroundPos:clientLightBlockIndex:forBlockhead:]`, radius 22 tiles. The four
    /// corners are always processed; the others are de-duplicated by macro tile identity.
    pub fn fully_load_if_needed_around_pos_client_light_block_index_for_blockhead(
        &self,
        pos: IntPair,
        client_light_block_index: i32,
        bh: Option<&ObjRef>,
    ) {
        let max = IntPair { x: pos.x + 22, y: pos.y + 22 };
        let min = IntPair { x: pos.x - 22, y: pos.y - 22 };
        let mts = self.macro_tiles();
        let t = |a: i32, b: i32| macro_tile_at_world_postion(a, b, &mts, self);
        let f = |m: &Option<MacroTileRef>, p: IntPair| {
            self.fully_load_and_update_if_needed_for_macro_block_including_pos_client_light_block_index_for_blockhead(
                m.as_ref(),
                p,
                client_light_block_index,
                bh,
            );
        };
        let t1 = t(min.x, min.y);
        f(&t1, min);
        let t2 = t(min.x, max.y);
        f(&t2, IntPair { x: min.x, y: max.y });
        let t3 = t(max.x, max.y);
        f(&t3, max);
        let t4 = t(max.x, min.y);
        f(&t4, IntPair { x: max.x, y: min.y });
        let t5 = t(pos.x, max.y);
        if !same_mt(&t5, &t3) && !same_mt(&t5, &t2) {
            f(&t5, IntPair { x: pos.x, y: max.y });
        }
        let t6 = t(pos.x, min.y);
        if !same_mt(&t6, &t1) && !same_mt(&t6, &t4) {
            f(&t6, IntPair { x: pos.x, y: min.y });
        }
        let t7 = t(min.x, pos.y);
        if !same_mt(&t7, &t1) && !same_mt(&t7, &t2) && !same_mt(&t7, &t5) && !same_mt(&t7, &t6) {
            f(&t7, IntPair { x: min.x, y: pos.y });
        }
        let t8 = t(max.x, pos.y);
        if !same_mt(&t8, &t4) && !same_mt(&t8, &t3) && !same_mt(&t8, &t5) && !same_mt(&t8, &t6) {
            f(&t8, IntPair { x: max.x, y: pos.y });
        }
        let t9 = t(pos.x, pos.y);
        if ![&t8, &t7, &t4, &t3, &t1, &t2, &t5, &t6].iter().any(|o| same_mt(&t9, o)) {
            f(&t9, pos);
        }
    }

    /// `-[World unmodifiedGroundLevelForX:]`: `worldTileLoader` (the nil `clientTileLoader` answers 0).
    pub fn unmodified_ground_level_for_x(&self, x: i32) -> i32 {
        if self.world_tile_loader.borrow().is_some() {
            deps().unmodified_ground_level_for_x(x)
        } else {
            0
        }
    }

    /// `-[World archiveLightBlocksForClient:]`: forwarder to the tile loader.
    pub fn archive_light_blocks_for_client(&self, client_id: &str) {
        if self.world_tile_loader.borrow().is_some() {
            deps().archive_light_blocks_for_client(client_id);
        }
    }

    /// `-[World startBulkDatabaseUpdate]`: `[databaseEnvironment startBulkTransaction]`.
    pub fn start_bulk_database_update(&self) {
        let env = self.database_environment.borrow().clone();
        if let Some(env) = env {
            env.start_bulk_transaction();
        }
    }

    /// `-[World finishBulkDatabaseUpdate]`: `[databaseEnvironment finishBulkTransaction]` (nil => NO).
    pub fn finish_bulk_database_update(&self) -> bool {
        let env = self.database_environment.borrow().clone();
        match env {
            Some(env) => env.finish_bulk_transaction(),
            None => false,
        }
    }
}

/// The [`WorldOpsHooks`] implementation world_state calls into (block loading, dynamic-object loading,
/// protection).
pub struct WorldOpsImpl;

impl WorldOpsHooks for WorldOpsImpl {
    fn load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
        &self,
        world: &World,
        mt: &MacroTileRef,
        mx: i32,
        my: i32,
        load_surrounding_blocks: bool,
        create_if_not_created: bool,
    ) -> bool {
        world.load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
            mt,
            mx,
            my,
            load_surrounding_blocks,
            create_if_not_created,
        )
    }
    fn load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(
        &self,
        world: &World,
        mt: &MacroTileRef,
        include_surface_blocks: bool,
    ) {
        world.load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(
            mt,
            include_surface_blocks,
        )
    }
    fn tile_is_protected_at_pos_against_blockhead(&self, world: &World, pos: IntPair, b: &ObjRef) -> bool {
        world.tile_is_protected_at_pos_against_blockhead(pos, b)
    }
}

/// Install [`WorldOpsImpl`] into `world` (World init).
pub fn install_ops_hooks(world: &World) {
    world.set_ops_hooks(Some(Rc::new(WorldOpsImpl)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::gsdict::GsDict;
    use crate::world::misc::{install_deps, testkit, NullDeps};
    use crate::world::state::{BHServer, ServerClient, WorldTileLoader};
    use std::cell::RefCell;

    fn loader_world(rec: &Rc<testkit::Rec>) -> Rc<World> {
        let w = World::alloc(32);
        *w.world_tile_loader.borrow_mut() = Some(Rc::new(WorldTileLoader {}));
        *rec.world.borrow_mut() = Some(Rc::downgrade(&w));
        rec.load_makes_valid.set(true);
        install_ops_hooks(&w);
        w
    }

    #[test]
    fn load_with_surrounding_order_and_pool() {
        let rec = testkit::install();
        let w = loader_world(&rec);
        let mt = w.macro_tiles().get(32 * 3 + 5).unwrap();
        assert!(w.load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
            &mt, 5, 3, true, true
        ));
        let log: Vec<String> = rec.take().into_iter().filter(|l| l.starts_with("loader.load")).collect();
        assert_eq!(
            log,
            vec![
                "loader.load(5,3,true)",
                "loader.load(4,4,true)",
                "loader.load(5,4,true)",
                "loader.load(6,4,true)",
                "loader.load(4,3,true)",
                "loader.load(6,3,true)",
                "loader.load(4,2,true)",
                "loader.load(5,2,true)",
                "loader.load(6,2,true)"
            ]
        );
        assert_eq!(w.used_physical_blocks_len(), 9);
        // already loaded: no loader call, but surrounding still processed
        let log2 = {
            w.load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
                &mt, 5, 3, false, true,
            );
            rec.take()
        };
        assert!(log2.is_empty());
        // out-of-range neighbours (y = -1) are skipped; x wraps (-1 -> 31) and passes the unwrapped x
        let mt0 = w.macro_tiles().get(0).unwrap();
        w.load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
            &mt0, 0, 0, true, true,
        );
        let log3: Vec<String> = rec.take();
        assert!(log3.contains(&"loader.load(-1,1,true)".to_string()), "{:?}", log3);
        assert!(!log3.iter().any(|l| l.contains(",-1,")));
    }

    #[test]
    fn tile_lookup_loads_block_through_hooks() {
        let rec = testkit::install();
        let w = loader_world(&rec);
        let t = tile_at_world_position_loaded(40, 70, &w).expect("tile");
        assert_eq!(t.local_index(), (70 % 32) * 32 + 40 % 32);
        assert!(rec.take().contains(&"loader.load(1,2,true)".to_string()));
        assert!(tile_at_world_position_loaded(0, 1024, &w).is_none());
    }

    #[test]
    fn save_and_decommission() {
        let rec = testkit::install();
        let w = loader_world(&rec);
        *w.server.borrow_mut() = Some(Rc::new(BHServer { api: None }));
        let mut m: GsDict<String, Rc<ServerClient>> = GsDict::new();
        for p in ["a", "b"] {
            m.insert(p.to_string(), Rc::new(ServerClient {}));
        }
        *w.server_clients.borrow_mut() = Some(Rc::new(RefCell::new(m)));
        let mt = w.macro_tiles().get(32 * 2 + 3).unwrap();
        // no block: true, nothing done
        assert!(w.save_physical_block_for_macro_tile_send_reliably_dont_send_only_save_if_clients_need_it(
            &mt, true, false, true
        ));
        assert!(rec.take().is_empty());
        w.load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
            &mt, 3, 2, false, true,
        );
        rec.take();
        // onlySaveIfClientsNeedIt with nobody wired => false, no save
        assert!(!w.save_physical_block_for_macro_tile_send_reliably_dont_send_only_save_if_clients_need_it(
            &mt, true, false, true
        ));
        assert!(rec.take().is_empty());
        // wired + connected
        rec.wired.borrow_mut().push(("b".into(), 2 * 32 + 3));
        rec.connected.borrow_mut().insert("b".into(), true);
        rec.wired.borrow_mut().push(("a".into(), 2 * 32 + 3)); // wired but not connected
        assert!(w.save_physical_block_for_macro_tile_send_reliably_dont_send_only_save_if_clients_need_it(
            &mt, false, false, true
        ));
        assert_eq!(rec.take(), vec!["savePhysicalBlock([\"b\"],false)"]);
        // dontSend
        assert!(w.save_physical_block_for_macro_tile_send_reliably_dont_send_only_save_if_clients_need_it(
            &mt, true, true, false
        ));
        assert_eq!(rec.take(), vec!["savePhysicalBlock([],true)"]);
        // decommission: removes dynamic objects, saves (no only-if-needed), invalidates, drops the pointer
        mt.dynamic_objects_loaded.set(true);
        let pb = mt.physical_block().unwrap();
        w.decommision_block_block_to_save_physcial_block(&mt, true);
        assert_eq!(rec.take(), vec!["removeDynamicObjectsForMacroTile", "savePhysicalBlock([\"b\"],true)"]);
        assert!(mt.physical_block().is_none());
        assert!(!pb.valid.get());
        // the block stays in used until the sweep
        assert_eq!(w.used_physical_blocks_len(), 1);
        w.decommision_all_blocks_block_to_save_physcial_block(false);
        assert_eq!(w.used_physical_blocks_len(), 0);
        assert_eq!(w.free_physical_blocks_len(), 1);
    }

    #[test]
    fn fully_load_dedup() {
        let rec = testkit::install();
        let w = loader_world(&rec);
        // local offset 16 in both axes: the 45x45 window touches 3x3 macro tiles => 9 distinct calls
        w.fully_load_if_needed_around_pos_client_light_block_index_for_blockhead(
            IntPair { x: 16 * 32 + 16, y: 16 * 32 + 16 },
            3,
            None,
        );
        let n = rec.take().iter().filter(|l| l.starts_with("recalcLighting")).count();
        assert_eq!(n, 9);
        // local offset 5 in both axes: 2x2 macro tiles, the four corners only
        w.fully_load_if_needed_around_pos_client_light_block_index_for_blockhead(
            IntPair { x: 16 * 32 + 5, y: 16 * 32 + 5 },
            3,
            None,
        );
        let n = rec.take().iter().filter(|l| l.starts_with("recalcLighting")).count();
        assert_eq!(n, 4);
        // x spans 2 macro tiles, y spans 3: 6 distinct tiles
        w.fully_load_if_needed_around_pos_client_light_block_index_for_blockhead(
            IntPair { x: 16 * 32 + 5, y: 16 * 32 + 16 },
            3,
            None,
        );
        let n = rec.take().iter().filter(|l| l.starts_with("recalcLighting")).count();
        assert_eq!(n, 6);
        // light block index lookups need a loader, a server and an existing client
        let pb = w.take_physical_block();
        w.load_light_block_for_client_light_block_index_into_physical_block(2, &pb);
        assert!(rec.take().is_empty());
        install_deps(Rc::new(NullDeps));
    }
}
