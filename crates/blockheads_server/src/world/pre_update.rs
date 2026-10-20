//! `World.preUpdate:` (the per-tick housekeeping that also drives the save cadence) and `decommissionOldBlocks`.
//!
//! No `RefCell` borrow is held across a call.

use crate::world::lifecycle::core_hooks;
use crate::world::state::{PhysicalBlockRef, World};
use crate::world::tiles::macro_tile_at_macro_postion;

/// `-[World startBulkDatabaseUpdate]`: `[databaseEnvironment startBulkTransaction]`; a nil
/// environment (before stage 0) is a no-op. world_ops exposes the same selector; this module needs it before that
/// file exists, so it is a private function here.
pub(crate) fn core_start_bulk_database_update(w: &World) {
    let env = w.database_environment.borrow().clone();
    if let Some(e) = env {
        let _ = e.start_bulk_transaction();
    }
}

/// `-[World finishBulkDatabaseUpdate]` -> BOOL: `[databaseEnvironment finishBulkTransaction]`;
/// nil environment gives NO.
pub(crate) fn core_finish_bulk_database_update(w: &World) -> bool {
    let env = w.database_environment.borrow().clone();
    env.map_or(false, |e| e.finish_bulk_transaction())
}

impl World {
    /// `-[World decommissionOldBlocks]`: ask the `WorldHelper` to decommission every old used block, move invalid
    /// blocks to the free list and free at most one block beyond 32. Iteration order is not reproducible in the
    /// original (pointer hash); allocation order is used.
    pub fn decommission_old_blocks(&self) {
        let hooks = core_hooks();
        let used = self.used_physical_blocks();
        let mut erase: Vec<PhysicalBlockRef> = Vec::new();
        for pb in used {
            if pb.valid.get() {
                let macro_tiles = self.macro_tiles();
                let mt = macro_tile_at_macro_postion(pb.x_pos.get(), pb.y_pos.get(), &macro_tiles, self);
                // local `float minAge = 30.0f`, widened to the double parameter
                hooks.world_helper_check_if_macro_tile_can_be_decommissioned(self, mt.as_ref(), 30.0f32 as f64, None);
            }
            // re-read `valid` after the call: the helper clears it when it decommissions
            if !pb.valid.get() {
                erase.push(pb);
            }
        }
        self.move_to_free_physical_blocks(&erase);
        if self.free_physical_blocks_len() > 32 {
            self.trim_free_physical_blocks();
        }
    }

    /// `-[World preUpdate:]`. `save_delay` is NOT consulted (R5): the cadence is 11 ticks (61 without a server),
    /// 10 ticks for light blocks (100 once the light-block migration is complete). Also re-entered from
    /// `client_disconnected_was_kick`, and runs during loading (`save_all` then writes nothing, R7).
    pub fn pre_update(&self, dt: f32) {
        let hooks = core_hooks();
        let has_dw = self.dynamic_world.borrow().is_some();
        // legacy v1 -> v2 conversion pump of the DynamicWorld (request from dynamic_world_core)
        if has_dw {
            hooks.dynamic_world_conversion_pump();
        }
        core_start_bulk_database_update(self);
        let has_server = self.server.borrow().is_some();
        let convertor = self.core_convertor();
        if has_server && convertor.as_ref().map_or(false, |c| !c.server_migration_complete()) {
            hooks.server_start_bulk_transaction();
        }
        self.decommission_old_blocks();
        if has_server {
            let has_loader = self.world_tile_loader.borrow().is_some();
            if has_loader {
                hooks.loader_start_bulk_light_block_transaction();
            }
            if has_dw {
                hooks.dynamic_world_send_lightblocks_to_clients();
                hooks.dynamic_world_save_and_send_only_blocks_that_need_to_be_sent();
            }
            // the faithful SIGSEGV happens in here
            if let Some(c) = &convertor {
                c.convert_light_blocks();
            }
            self.set_lightblock_save_counter(self.lightblock_save_counter() + 1);
            let limit = if convertor.as_ref().map_or(false, |c| c.light_block_migration_complete()) { 100 } else { 10 };
            if self.lightblock_save_counter() >= limit {
                self.set_lightblock_save_counter(0);
                if has_loader && hooks.loader_finish_bulk_light_block_transaction() {
                    if let Some(c) = &convertor {
                        c.remove_light_block_files();
                    }
                }
            }
        }
        self.set_save_counter(self.save_counter() + 1);
        if let Some(c) = &convertor {
            c.convert_world();
        }
        let limit = if has_server { 11 } else { 61 };
        if self.save_counter() >= limit {
            self.set_save_counter(0);
            // `[delegate appDatabaseEnvironment]` is nil on the dedicated server: both bulk calls are no-ops
            let env = self.core_delegate().and_then(|d| d.app_database_environment());
            if let Some(e) = &env {
                let _ = e.start_bulk_transaction();
            }
            self.save_all();
            let mut safe = core_finish_bulk_database_update(self);
            if has_server && convertor.is_some() {
                // the server call is ALWAYS made (no short circuit)
                safe = hooks.server_finish_bulk_transaction() && safe;
            }
            if safe {
                if let Some(c) = &convertor {
                    c.remove_world_files();
                }
            }
            if let Some(e) = &env {
                let _ = e.finish_bulk_transaction();
            }
        }
        hooks.tip_manager_update(dt);
    }
}

#[cfg(test)]
mod tests {
    use crate::world::lifecycle::testkit::*;
    use std::rc::Rc;

    #[test]
    fn save_cadence_is_every_11th_pre_update_regardless_of_save_delay() {
        let f = fixture(32);
        let w = &f.world;
        with_dw(w);
        assert_eq!(w.save_delay(), 10);
        for i in 1..=10 {
            w.pre_update(0.2);
            assert_eq!(f.rec.saved.borrow().len(), 0, "tick {}", i);
        }
        w.pre_update(0.2);
        assert_eq!(f.rec.saved.borrow().len(), 1);
        assert_eq!(w.save_counter(), 0);
        for _ in 0..11 {
            w.pre_update(0.2);
        }
        assert_eq!(f.rec.saved.borrow().len(), 2);
        // the conversion pump runs first on every tick
        assert_eq!(f.rec.count("pump"), 22);
    }

    #[test]
    fn without_a_server_the_cadence_is_61() {
        let f = fixture(32);
        let w = &f.world;
        with_dw(w);
        *w.server.borrow_mut() = None;
        for _ in 0..60 {
            w.pre_update(0.2);
        }
        assert_eq!(f.rec.saved.borrow().len(), 0);
        w.pre_update(0.2);
        assert_eq!(f.rec.saved.borrow().len(), 1);
    }

    #[test]
    fn light_block_transaction_closes_every_10_ticks_and_loading_ticks_write_nothing() {
        let f = fixture(32);
        let w = &f.world;
        // before the dynamic world exists: preUpdate/saveAll run but nothing is written (R7)
        for _ in 0..11 {
            w.pre_update(0.2);
        }
        assert!(f.rec.saved.borrow().is_empty());
        assert!(w.creation_date().is_some(), "creationDate is taken at the first saveAll");
        *w.world_tile_loader.borrow_mut() = Some(Rc::new(crate::world::state::WorldTileLoader {}));
        w.set_lightblock_save_counter(0);
        f.rec.calls.borrow_mut().clear();
        for _ in 0..10 {
            w.pre_update(0.2);
        }
        assert_eq!(f.rec.count("lb_start"), 10);
        assert_eq!(f.rec.count("lb_finish"), 1);
        assert_eq!(w.lightblock_save_counter(), 0);
    }

    #[test]
    fn decommission_frees_at_most_one_block_beyond_32() {
        let f = fixture(32);
        let w = &f.world;
        let blocks: Vec<_> = (0..40)
            .map(|i| {
                let b = w.take_physical_block();
                b.x_pos.set(i % 32);
                b.y_pos.set(0);
                b
            })
            .collect();
        // 3 valid blocks stay used (the hook does not invalidate them), 37 invalid ones are freed
        for b in blocks.iter().take(3) {
            b.valid.set(true);
        }
        w.decommission_old_blocks();
        assert_eq!(w.used_physical_blocks_len(), 3);
        assert_eq!(w.free_physical_blocks_len(), 36, "37 moved to the free list, 1 trimmed (> 32)");
        assert_eq!(w.graveyard.borrow().len(), 1);
        assert_eq!(f.rec.count("decommission_check true 30"), 3);
        w.decommission_old_blocks();
        assert_eq!(w.free_physical_blocks_len(), 35);
        for _ in 0..10 {
            w.decommission_old_blocks();
        }
        assert_eq!(w.free_physical_blocks_len(), 32);
    }
}
