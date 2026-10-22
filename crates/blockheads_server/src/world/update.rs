//! `world_ops` update: `-[World update:accurateDT:pinchScale:dragInProgress:]` (the 0.2 s idle-loop tick).
//!
//! 5.5-5.8. Client-only steps (9, 15, 17) and the `uiManager` gates (6, 7) are not ported (section 0 item 1).

use std::cell::RefCell;

use super::misc::deps;
use super::state::World;
use super::tiles::tile_at_world_position_loaded;
use crate::foundation::clock;
use crate::objects::handle::{ptr_eq, ObjRef};
use crate::path::path_creator::{PathCreator, PathRequest};
use crate::plist::value::PlistValue;
use crate::types::structs::IntPair;

thread_local! {
    /// `World.pathQueue` (NSMutableArray of path request dictionaries). `world::state::World.path_queue` still
    /// holds the empty placeholder `PathRequest`, so the queue of real requests lives here
    ///.
    static PATH_QUEUE: RefCell<Vec<PathRequest>> = const { RefCell::new(Vec::new()) };
}

/// Number of queued path requests (test / diagnostics helper).
pub fn path_queue_len() -> usize {
    PATH_QUEUE.with(|q| q.borrow().len())
}

/// Drop every queued path request (world teardown / tests).
pub fn path_queue_clear() {
    PATH_QUEUE.with(|q| q.borrow_mut().clear());
}

fn same_user(a: &Option<ObjRef>, b: &Option<ObjRef>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

impl World {
    /// `-[World update:accurateDT:pinchScale:dragInProgress:]`. Called by `GameController.update` every
    /// 0.2 s tick with `dt = accurateDT = 0.2f32`, pinch 1.0, drag NO.
    pub fn update_accurate_dt_pinch_scale_drag_in_progress(
        &self,
        dt: f32,
        accurate_dt: f32,
        _pinch_scale: f64,
        drag_in_progress: bool,
    ) {
        let mut dt = dt;
        let mut accurate_dt = accurate_dt;
        let d = deps();

        // 0
        self.drag_in_progress.set(drag_in_progress);

        // 1: catch-up simulation (never true on the server; kept for fidelity)
        if self.is_simulating.get() {
            d.continue_simulate();
            let mut count = 0;
            while self.is_simulating.get() && count < 4 {
                d.continue_simulate();
                count += 1;
            }
            return;
        }

        // 2: path pump (always runs on the server: uiManager is nil)
        self.update_path_pump(false);

        // 3
        self.update_idle_timer_disabled();
        self.last_update_time.set(clock::reference_time());

        // 4: server clients
        if self.server().is_some() && self.server_clients_count() != 0 {
            self.update_server_clients();
        }

        // 5: trade transaction upload timer (HTTP dropped, state reset kept;)
        if self.unsent_global_trade_transactions.borrow().is_some() {
            // sendPricesConnection is always nil
            self.send_trade_transaction_timer.set(self.send_trade_transaction_timer.get() + accurate_dt);
            if self.send_trade_transaction_timer.get() > 60.0f32 {
                *self.unsent_global_trade_transactions.borrow_mut() = None;
                self.send_trade_transaction_timer.set(0.0);
            }
        }

        // 8
        let blockheads = d.all_blockheads_including_net();
        let mut all_asleep = true;
        let mut at_least_one_asleep = false;
        for b in blockheads.iter() {
            if !d.bh_asleep(b) {
                if d.bh_meditating(b) {
                    at_least_one_asleep = true;
                } else if !d.bh_regenerating(b) && !d.bh_on_trade_mission(b) {
                    all_asleep = false;
                }
            } else {
                at_least_one_asleep = true;
            }
        }

        // 10: fast-forward control (client == nil)
        let mut local_ff = self.fast_forward.get();
        if !all_asleep || !at_least_one_asleep {
            if self.fast_forward.get() {
                self.fast_forward.set(false);
                local_ff = false;
                d.send_heartbeat_data();
            }
        } else if !self.fast_forward.get() {
            self.fast_forward.set(true);
            local_ff = true;
            d.send_heartbeat_data();
        }

        // 11: before the x20 scaling
        self.save_count.set(self.save_count.get() + dt);

        // 12
        if local_ff {
            dt *= 20.0f32;
            accurate_dt *= 20.0f32;
        }

        // 13
        let mut mult = 1.0f32;
        let rules = self.custom_rules.get();
        if rules.custom_rules_enabled != 0 {
            if rules.day_night_cycle == 0 {
                mult = 0.25f32;
            } else if rules.day_night_cycle == 2 {
                mult = 4.0f32;
            }
        }

        // 14
        self.world_time.set(self.world_time.get() + (accurate_dt * mult) as f64);
        self.no_rain_timer.set(self.no_rain_timer.get() + accurate_dt);
        if accurate_dt > 1.0f32 {
            accurate_dt = 1.0f32;
        }

        // 16
        d.projectile_manager_update(dt);

        // 18
        d.update_accurate_dt(dt, accurate_dt);
    }

    /// Step 2 (a, b, c): request intake, creator pump, completion. `catch_up` is the variant that follows
    /// `[dynamicWorld simulate:]` in `-[World continueSimulate]`: after one `updatePath` it keeps pumping (up to 4
    /// more times) while the path is in progress, saving and decommissioning blocks when more than 0x80 are used.
    pub(crate) fn update_path_pump(&self, catch_up: bool) {
        let d = deps();
        let Some(pc) = d.path_creator() else { return };

        // 2a
        for pu in d.path_users() {
            let Some(pup) = pu.as_path_user() else { continue };
            if !pup.path_needs_recalculated() {
                continue;
            }
            let Some(info) = pup.info_for_path_recalculation() else { continue };
            let pd = PathRequest::from_plist_dict_with_user(&info, Some(pu.clone()));
            let (gx, gy) = (pd.goal_x, pd.goal_y);
            // LOADS/CREATES the goal block if it is not resident (side effect)
            if tile_at_world_position_loaded(gx, gy, self).is_some() {
                PATH_QUEUE.with(|q| {
                    let mut q = q.borrow_mut();
                    let pos = q.iter().position(|cd| match &cd.path_user {
                        Some(u) => ptr_eq(u, &pu) && cd.path_type == pd.path_type,
                        None => false,
                    });
                    if let Some(i) = pos {
                        q.remove(i);
                    }
                    q.push(pd.clone());
                });
                pup.set_waiting_for_path_to_pos(IntPair { x: gx, y: gy });
                pup.set_path_needs_recalculated(false);
                if (pc.in_progress() || pc.has_path())
                    && pc.path_user().as_ref().is_some_and(|u| ptr_eq(u, &pu))
                    && pc.path_type() == pd.path_type
                {
                    pc.abort_path();
                }
            }
        }

        // 2b
        if pc.in_progress() || pc.has_path() {
            pc.update_path();
            if catch_up {
                let mut sanity = 4;
                while pc.in_progress() && !pc.has_path() && sanity > 0 {
                    sanity -= 1;
                    pc.update_path();
                    if self.used_physical_blocks_len() > 0x80 {
                        self.save_all();
                        self.decommision_all_blocks_block_to_save_physcial_block(false);
                    }
                }
            }
        } else {
            let next = PATH_QUEUE.with(|q| {
                let mut q = q.borrow_mut();
                if q.is_empty() {
                    None
                } else {
                    Some(q.remove(0))
                }
            });
            if let Some(pd) = next {
                pc.create_path_with_dict(&pd);
            }
        }

        // 2c
        if !pc.in_progress() && pc.has_path() {
            self.finish_path(&pc);
        }
    }

    /// Step 2c: hand the finished path to its path user.
    fn finish_path(&self, pc: &PathCreator) {
        let d = deps();
        let pu = pc.path_user();
        let pt = pc.path_type();
        let (gx, gy) = (pc.goal_x(), pc.goal_y());
        let gi = pc.goal_interaction();
        let extra = pc.extra_data();
        let path: Option<Vec<PlistValue>> = pc.path().map(|p| p.iter().map(|s| s.to_plist_dict()).collect());

        let set_path = |extra: Option<PlistValue>| {
            if let Some(pu) = pu.as_ref() {
                if let Some(pup) = pu.as_path_user() {
                    pup.set_path_type_goal_interaction_extra_data(pu, path.clone(), pt, gi, extra);
                }
            }
        };

        match gi {
            4 => {
                // INTERACTION_CRAFT
                let mut wb = d.workbench_at_pos(IntPair { x: gx, y: gy });
                if wb.is_none() {
                    wb = d.workbench_at_pos(IntPair { x: gx, y: gy - 1 });
                }
                if wb.is_some() && extra.is_some() {
                    set_path(extra.clone());
                }
            }
            5 => {
                // INTERACTION_USE_INTERACTIVE_OBJECT
                let dw = self.dynamic_world();
                let mut io = dw.as_ref().and_then(|dw| dw.interaction_object_at_pos(IntPair { x: gx, y: gy }));
                if io.is_none() {
                    io = dw.as_ref().and_then(|dw| dw.interaction_object_at_pos(IntPair { x: gx, y: gy - 1 }));
                }
                if io.is_some() && extra.is_some() {
                    // extraData == nil would call [uiManager interactionObjectTapped:hasCancel:] (nil on the server)
                    set_path(extra.clone());
                }
            }
            _ => {
                // the painting lookup / setInventoryUIToExpanded only feed uiManager (nil): displayedOptionsUI is NO
                set_path(extra.clone());
            }
        }

        let has_more = PATH_QUEUE.with(|q| q.borrow().iter().any(|e| same_user(&e.path_user, &pu)));
        if !has_more {
            if let Some(pu) = pu.as_ref() {
                if let Some(pup) = pu.as_path_user() {
                    pup.set_no_longer_waiting_for_path();
                }
            }
        }
        pc.set_has_path(false);
    }

    /// Step 4 (server branch): serve block requests, heartbeats and drop disconnected clients.
    fn update_server_clients(&self) {
        let d = deps();
        let mut remove_peers: Vec<String> = Vec::new();
        let w = self.world_width_macro as i64;
        for peer in self.server_clients_keys_in_order() {
            if !d.sc_connected(&peer) {
                if !d.is_controlling_blockheads_for_client_player(&peer) {
                    remove_peers.push(peer);
                }
                continue;
            }
            let indices = d.sc_requested_block_indices_first_n(&peer, 4);
            for index in indices {
                let macro_x = (index % (w as u64)) as i32;
                let macro_y = (index / (w as u64)) as i32;
                let mut sent = false;
                // UB-PARITY: index >= W*32 reads out of bounds in the original; here "no block"
                if let Some(mt) = self.macro_tiles.get(index as usize) {
                    let mut block = mt.physical_block();
                    if block.is_none() || !block.is_some_and(|b| b.valid.get()) {
                        let create = d.sc_create_if_not_created_for_block_request(&peer, index as u32 as i32);
                        self.load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
                            &mt, macro_x, macro_y, false, create,
                        );
                        block = mt.physical_block();
                    }
                    if let Some(b) = block {
                        if b.valid.get() && mt.unexplored.get() == 0 {
                            d.send_block_to_client_without_saving_for_block_pos_send_to_client_server_send_dynamic_objects_reliable(
                                b,
                                IntPair { x: macro_x, y: macro_y },
                                &peer,
                                true,
                                true,
                            );
                            sent = true;
                        }
                    }
                }
                if !sent {
                    let mut data = Vec::with_capacity(13);
                    data.push(0x27u8);
                    data.extend_from_slice(&index.to_le_bytes());
                    data.extend_from_slice(&(index as u32).to_le_bytes());
                    d.send_network_data_to_peers_reliable(&data, Some(std::slice::from_ref(&peer)), true);
                }
                d.sc_remove_block_request(&peer, index as u32 as i32);
            }
            if d.sc_requests_heart_beat(&peer) {
                d.sc_set_has_ever_requested_heartbeat(&peer, true);
                d.sc_set_requests_heart_beat(&peer, false);
                d.sc_set_time_since_last_heartbeat_request(&peer, 0.0);
                let hb = d.heartbeat_data();
                d.send_network_data_to_peers_reliable(&hb, Some(std::slice::from_ref(&peer)), true);
            }
        }
        for peer in remove_peers.iter() {
            let lb = d.sc_light_block_index(peer);
            for pb in self.used_physical_blocks() {
                if (0..32).contains(&lb) {
                    pb.client_light_blocks.borrow_mut()[lb as usize] = None;
                }
            }
            if let Some(s) = self.free_client_light_block_indices.borrow_mut().as_mut() {
                s.add_index(lb as i64 as u64);
            }
            if let Some(m) = self.server_clients.borrow().as_ref() {
                m.borrow_mut().remove(peer);
            }
            d.remove_dynamic_objects_belonging_to_client(peer);
            d.client_finished_away_simulation(peer);
        }
        if !remove_peers.is_empty() {
            d.send_player_changed_notifcation_to_delegate();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::gsdict::GsDict;
    use crate::foundation::indexset::IndexSet;
    use crate::objects::handle;
    use crate::world::helpers::testutil::new_obj;
    use crate::world::misc::{install_deps, testkit, NullDeps};
    use crate::world::state::{BHServer, ServerClient};
    use std::rc::Rc;

    fn server_world(peers: &[&str]) -> Rc<World> {
        let w = World::alloc(32);
        *w.server.borrow_mut() = Some(Rc::new(BHServer { api: None }));
        *w.world_tile_loader.borrow_mut() = Some(Rc::new(crate::world::state::WorldTileLoader {}));
        super::super::blocks::install_ops_hooks(&w);
        let mut m: GsDict<String, Rc<ServerClient>> = GsDict::new();
        for p in peers {
            m.insert(p.to_string(), Rc::new(ServerClient {}));
        }
        *w.server_clients.borrow_mut() = Some(Rc::new(RefCell::new(m)));
        w
    }

    fn setup(rec: &Rc<testkit::Rec>, w: &Rc<World>) {
        *rec.world.borrow_mut() = Some(Rc::downgrade(w));
        rec.load_makes_valid.set(true);
    }

    /// T10: tick without clients: order of effects, worldTime and (dt, accurateDT).
    #[test]
    fn tick_basics_and_day_night_cycle() {
        let rec = testkit::install();
        let w = World::alloc(32);
        w.update_accurate_dt_pinch_scale_drag_in_progress(0.2, 0.2, 1.0, true);
        assert!(w.drag_in_progress.get());
        assert_eq!(w.world_time.get(), 0.2f32 as f64);
        assert_eq!(w.no_rain_timer.get(), 0.2f32);
        assert_eq!(w.save_count.get(), 0.2f32);
        assert_eq!(rec.take(), vec!["pm.update(0.2)", "dw.update(0.2,0.2)"]);
        // custom rules: dayNightCycle 0 => x0.25, 2 => x4
        let mut cr = w.custom_rules.get();
        cr.custom_rules_enabled = 1;
        cr.day_night_cycle = 0;
        w.custom_rules.set(cr);
        w.world_time.set(0.0);
        w.update_accurate_dt_pinch_scale_drag_in_progress(0.2, 0.2, 1.0, false);
        assert_eq!(w.world_time.get(), (0.2f32 * 0.25f32) as f64);
        cr.day_night_cycle = 2;
        w.custom_rules.set(cr);
        w.world_time.set(0.0);
        w.update_accurate_dt_pinch_scale_drag_in_progress(0.2, 0.2, 1.0, false);
        assert_eq!(w.world_time.get(), (0.2f32 * 4.0f32) as f64);
        // accurateDT is clamped for the dynamic world only
        rec.take();
        w.update_accurate_dt_pinch_scale_drag_in_progress(3.0, 3.0, 1.0, false);
        assert_eq!(rec.take(), vec!["pm.update(3)", "dw.update(3,1)"]);
        install_deps(Rc::new(NullDeps));
    }

    /// T10: fast-forward flip sends exactly one heartbeat; (dt, accurateDT) = (4.0, 1.0); worldTime += 4.0.
    #[test]
    fn fast_forward_when_all_asleep() {
        let rec = testkit::install();
        let w = World::alloc(32);
        let b = new_obj();
        rec.blockheads.borrow_mut().push(b.clone());
        rec.asleep.borrow_mut().push(handle::obj_id(&b));
        w.update_accurate_dt_pinch_scale_drag_in_progress(0.2, 0.2, 1.0, false);
        assert!(w.fast_forward.get());
        assert_eq!(rec.take(), vec!["sendHeartbeatData", "pm.update(4)", "dw.update(4,1)"]);
        assert_eq!(w.world_time.get(), (0.2f32 * 20.0f32) as f64);
        // stays on, no second heartbeat
        w.update_accurate_dt_pinch_scale_drag_in_progress(0.2, 0.2, 1.0, false);
        assert_eq!(rec.take(), vec!["pm.update(4)", "dw.update(4,1)"]);
        // wake up
        rec.asleep.borrow_mut().clear();
        w.update_accurate_dt_pinch_scale_drag_in_progress(0.2, 0.2, 1.0, false);
        assert!(!w.fast_forward.get());
        assert_eq!(rec.take(), vec!["sendHeartbeatData", "pm.update(0.2)", "dw.update(0.2,0.2)"]);
        // zero blockheads: fast-forward stays off
        rec.blockheads.borrow_mut().clear();
        w.update_accurate_dt_pinch_scale_drag_in_progress(0.2, 0.2, 1.0, false);
        assert!(!w.fast_forward.get());
        install_deps(Rc::new(NullDeps));
    }

    /// T10: at most 4 block replies per client per tick, ascending, 0x27 bytes, heartbeat reply.
    #[test]
    fn block_pump_cap_order_and_unavailable_reply() {
        let rec = testkit::install();
        let w = server_world(&["p1"]);
        setup(&rec, &w);
        rec.connected.borrow_mut().insert("p1".into(), true);
        rec.requested.borrow_mut().insert("p1".into(), vec![9, 3, 40, 1, 7, 5]);
        rec.create_req.set(false); // load stays pending: create == false => block unavailable? loader stub makes it valid
        rec.load_makes_valid.set(false);
        w.update_accurate_dt_pinch_scale_drag_in_progress(0.2, 0.2, 1.0, false);
        let log = rec.take();
        let sends: Vec<_> = log.iter().filter(|l| l.starts_with("send(")).collect();
        assert_eq!(sends.len(), 4);
        // ascending: 1, 3, 5, 7
        let mut expect_idx = vec![1u64, 3, 5, 7].into_iter();
        for s in sends {
            let i = expect_idx.next().unwrap();
            let mut bytes = vec![0x27u8];
            bytes.extend_from_slice(&i.to_le_bytes());
            bytes.extend_from_slice(&(i as u32).to_le_bytes());
            assert_eq!(*s, format!("send({:?},Some([\"p1\"]),true)", bytes));
        }
        assert_eq!(rec.requested.borrow()["p1"], vec![9, 40]);
        // with loadable blocks the block is sent instead (and the macro block gets loaded: create = true)
        rec.load_makes_valid.set(true);
        rec.create_req.set(true);
        rec.take();
        w.update_accurate_dt_pinch_scale_drag_in_progress(0.2, 0.2, 1.0, false);
        let log = rec.take();
        assert!(log.contains(&"sendBlock(9,0,p1)".to_string()), "{:?}", log);
        assert!(log.contains(&"sc.create(p1,9)".to_string()));
        // out-of-range index => 0x27 reply (UB-PARITY), request removed
        rec.requested.borrow_mut().insert("p1".into(), vec![100000]);
        rec.take();
        w.update_accurate_dt_pinch_scale_drag_in_progress(0.2, 0.2, 1.0, false);
        let log = rec.take();
        assert!(log.iter().any(|l| l.starts_with("send([39, ")), "{:?}", log);
        install_deps(Rc::new(NullDeps));
    }

    #[test]
    fn heartbeat_reply_and_client_removal() {
        let rec = testkit::install();
        let w = server_world(&["a", "b"]);
        setup(&rec, &w);
        *rec.heartbeat.borrow_mut() = vec![0x17, 1, 2];
        rec.connected.borrow_mut().insert("a".into(), true);
        rec.requests_hb.borrow_mut().insert("a".into(), true);
        // b: disconnected without blockheads, light index 5
        rec.light_idx.borrow_mut().insert("b".into(), 5);
        *w.free_client_light_block_indices.borrow_mut() = Some(IndexSet::new());
        let pb = w.take_physical_block();
        pb.client_light_blocks.borrow_mut()[5] = Some(Box::new([0u8; 1024]));
        w.update_accurate_dt_pinch_scale_drag_in_progress(0.2, 0.2, 1.0, false);
        let log = rec.take();
        let hb = log.iter().position(|l| l == "send([23, 1, 2],Some([\"a\"]),true)").expect("heartbeat");
        let sc = log.iter().position(|l| l == "sc.setRequestsHeartBeat(a,false)").unwrap();
        assert!(sc < hb);
        let r1 = log.iter().position(|l| l == "removeDynamicObjectsBelongingToClient(b)").unwrap();
        let r2 = log.iter().position(|l| l == "clientFinishedAwaySimulation(b)").unwrap();
        let r3 = log.iter().position(|l| l == "playerChanged").unwrap();
        assert!(r1 < r2 && r2 < r3);
        assert_eq!(w.server_clients_count(), 1);
        assert!(pb.client_light_blocks.borrow()[5].is_none());
        assert!(w.free_client_light_block_indices.borrow().as_ref().unwrap().contains_index(5));
        // a client that still controls blockheads is kept
        let w = server_world(&["c"]);
        setup(&rec, &w);
        rec.controlling.borrow_mut().push("c".into());
        w.update_accurate_dt_pinch_scale_drag_in_progress(0.2, 0.2, 1.0, false);
        assert_eq!(w.server_clients_count(), 1);
        install_deps(Rc::new(NullDeps));
    }

    #[test]
    fn trade_timer_resets() {
        let _rec = testkit::install();
        let w = World::alloc(32);
        *w.unsent_global_trade_transactions.borrow_mut() =
            Some(crate::plist::value::PlistDict { map: GsDict::new(), tag: Default::default() });
        w.send_trade_transaction_timer.set(59.9);
        w.update_accurate_dt_pinch_scale_drag_in_progress(0.2, 0.2, 1.0, false);
        assert!(w.unsent_global_trade_transactions.borrow().is_none());
        assert_eq!(w.send_trade_transaction_timer.get(), 0.0);
        install_deps(Rc::new(NullDeps));
    }

    /// The catch-up pump (`-[World continueSimulate]`) keeps pumping a path in progress, and saves and
    /// decommissions every block once more than 0x80 physical blocks are in use.
    #[test]
    fn catch_up_pump_decommissions_past_0x80_blocks() {
        use crate::path::path_creator::testutil::{new_user, new_world};
        use crate::path::path_creator::{PathCreator, PathRequest};
        let rec = testkit::install();
        let (w, _loader, _dw) = new_world(32, 20);
        let pc = PathCreator::init_with_world(&w);
        let user = new_user(IntPair { x: 100, y: 21 });
        let req = PathRequest {
            path_type: 0,
            goal_x: 400,
            goal_y: 21,
            goal_interaction: 0,
            extra_data: None,
            path_user: Some(user),
        };
        pc.create_path_with_dict(&req);
        assert!(pc.in_progress() && !pc.has_path());
        *rec.path_creator.borrow_mut() = Some(pc.clone());
        while w.used_physical_blocks_len() <= 0x80 {
            w.take_physical_block();
        }
        assert_eq!(w.free_physical_blocks_len(), 0);
        w.update_path_pump(true);
        // still searching (the goal is far away), and the fresh never-loaded blocks went back to the free set
        assert!(pc.in_progress() && !pc.has_path());
        assert!(w.free_physical_blocks_len() > 0x40);
        assert!(w.used_physical_blocks_len() < 0x40);
        // the per-tick pump advances the search once and does not catch up
        w.update_path_pump(false);
        assert!(pc.in_progress());
        install_deps(Rc::new(NullDeps));
    }
}
