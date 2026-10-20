//! The per-tick driver of the dynamic world: `update:accurateDT:isSimulation:`, `simulate:`, the disconnected-client
//! blockhead loader and the small query helpers.
//!
//! `##...update:accurateDT:`, `##...hasDynamicObjectsToSaveInMacroPos:`.
//! Decision 2 of the contract: this is only ticked while the server "is running" (caller's business).

use super::state::{DynamicWorld, MAX_DYNAMIC_OBJECT_TYPES};
use crate::foundation::gzip::gzip_inflate;
use crate::nslog;
use crate::objects::handle::{ptr_eq, ObjRef};
use crate::objects::type_tables::{
    object_partial_update_max, object_type_has_static_position, object_type_requires_partial_update,
    DYNAMIC_OBJECT_TYPE_FREE_BLOCK, DYNAMIC_OBJECT_TYPE_SNOW_SURFACEBLOCK,
};
use crate::plist::helpers::{property_list_from_data_opt, property_list_with_data_raw};
use crate::plist::value::PlistValue;
use crate::types::structs::IntPair;

impl DynamicWorld {
    /// `-update:accurateDT:`: the `accurateDT` parameter is ignored (`dt` passed for both).
    pub fn update_accurate_dt(&self, dt: f32, _accurate_dt: f32) {
        self.update_accurate_dt_is_simulation(dt, dt, false);
    }

    /// `-simulate:`: `step = dt / 8.0f`, 8 steps of `update:step accurateDT:step isSimulation:YES`.
    pub fn simulate(&self, dt: f32) {
        let step = dt / 8.0f32;
        for _ in 0..8 {
            self.update_accurate_dt_is_simulation(step, step, true);
        }
    }

    /// `-hasDynamicObjectsToSaveInMacroPos:` (`client` is always nil here).
    pub fn has_dynamic_objects_to_save_in_macro_pos(&self, macro_pos: IntPair) -> bool {
        for t in 0..MAX_DYNAMIC_OBJECT_TYPES {
            if self.dynamic_world_changed_macro_positions.borrow()[t].iter().any(|p| *p == macro_pos) {
                return true;
            }
        }
        self.world_changed_macro_positions.borrow().iter().any(|p| *p == macro_pos)
    }

    /// `-finishSimulating`: flush all 8 inventory slots of every local blockhead.
    pub fn finish_simulating(&self) {
        let hooks = self.hooks();
        let bhs: Vec<ObjRef> = self.blockheads.borrow().clone();
        for b in bhs {
            for i in 0..8 {
                hooks.blockhead_inventory_was_changed(&b, i, -1, false);
            }
            self.save_blockhead_inventory(&b);
            hooks.blockhead_set_inventory_needs_saving(&b, false);
        }
    }

    /// `-loadAnyBlockheadsForDisconnectedClients`.
    // Note: step 6 compares NSString POINTERS in the original; strings are compared by value here.
    pub fn load_any_blockheads_for_disconnected_clients(&self) {
        let hooks = self.hooks();
        let world = self.world();
        let names: Vec<String> = self.disconnected_clients_save_dir_names.borrow().clone().unwrap_or_default();
        for client_id in names.iter() {
            // 1. load the client's save dict
            let key = format!("{}_blockheads", client_id);
            let mut save_data: Option<PlistValue> = match self.world_database.data_for_key(Some(&key)) {
                Some(d) => property_list_from_data_opt(gzip_inflate(&d).as_deref()),
                None => None,
            };
            let player_dir = player_dir_for(client_id);
            // 2. legacy file
            if save_data.is_none() && !world.has_finished_database_migration_to17() {
                let path = format!("{}/{}blockheads", self.world_save_directory.trim_end_matches('/'), player_dir);
                if std::path::Path::new(&path).exists() {
                    if let Ok(d) = std::fs::read(&path) {
                        save_data = property_list_from_data_opt(gzip_inflate(&d).as_deref());
                    }
                }
            }
            match save_data {
                None => {
                    nslog!("no save data found for disconnected client");
                }
                Some(save_data) => {
                    let dicts: Vec<PlistValue> = match save_data.object_for_key("dynamicObjects") {
                        Some(PlistValue::Array(a)) => a.items.clone(),
                        _ => Vec::new(),
                    };
                    for object_dict in dicts.iter() {
                        let unique_id = object_dict.object_for_key("uniqueID").map_or(0, |v| v.unsigned_long_value());
                        let found = self
                            .net_blockheads_with_disconnected_clients
                            .borrow()
                            .iter()
                            .any(|b| b.unique_id() == unique_id);
                        if found {
                            continue;
                        }
                        let inv_key = format!("{}_blockhead_{}_inventory", client_id, unique_id as i64);
                        let mut inventory = self.world_database.data_for_key(Some(&inv_key));
                        if inventory.is_none() && !world.has_finished_database_migration_to17() {
                            let path = format!(
                                "{}/{}blockhead_{}_inventory",
                                self.world_save_directory.trim_end_matches('/'),
                                player_dir,
                                unique_id as i64
                            );
                            if std::path::Path::new(&path).exists() {
                                inventory = std::fs::read(&path).ok();
                            }
                        }
                        let Some(inventory) = inventory else {
                            nslog!("no inventory data found");
                            continue;
                        };
                        // no @try here (contract 5.7): the raw parser may raise
                        let slots = gzip_inflate(&inventory).and_then(|d| property_list_with_data_raw(&d, 0));
                        let PlistValue::Dict(save_dict) = object_dict else { continue };
                        let blockhead = hooks.blockhead_from_save_dict(save_dict, slots, true, Some(client_id), None);
                        let Some(blockhead) = blockhead else {
                            nslog!("blockhead failed to load");
                            continue;
                        };
                        hooks.blockhead_set_client_id(&blockhead, client_id);
                        let alias =
                            self.server.borrow().clone().and_then(|s| s.player_info_for_peer_id(client_id)).and_then(
                                |d| d.object_for_key("alias").and_then(|v| v.as_str().map(|s| s.to_string())),
                            );
                        hooks.blockhead_set_client_name(&blockhead, alias.as_deref());
                        self.net_blockheads_with_disconnected_clients.borrow_mut().push(blockhead.clone());
                        let light = self.server_client(client_id).map_or(0, |sc| sc.light_block_index());
                        hooks.fully_load_if_needed_around_pos(blockhead.pos(), light as u32, &blockhead);
                        if self.disconnected_clients_cached_save_dicts_not_done.borrow().is_none() {
                            *self.disconnected_clients_cached_save_dicts_not_done.borrow_mut() =
                                Some(crate::foundation::gsdict::GsDict::new());
                        }
                        if let Some(dir) = hooks.blockhead_client_save_dir(&blockhead) {
                            let sd =
                                hooks.blockhead_save_dict_including_workbench_or_interaction_object(&blockhead, true);
                            let mut cache = self.disconnected_clients_cached_save_dicts_not_done.borrow_mut();
                            let cache = cache.as_mut().unwrap();
                            if cache.get(&dir).is_none() {
                                cache.insert(dir.clone(), Vec::new());
                            }
                            let arr = cache.get_mut(&dir).unwrap();
                            let uid = blockhead.unique_id();
                            let idx = arr.iter().position(|d| {
                                d.object_for_key("uniqueID").map_or(0, |v| v.unsigned_long_value()) == uid
                            });
                            match idx {
                                None => arr.push(sd),
                                Some(i) => arr[i] = sd,
                            }
                        }
                        hooks.blockhead_stop_interacting_if_no_interaction_object(&blockhead);
                        hooks.blockhead_stop_riding(&blockhead);
                        // 8. LAST net blockhead with the same unique id
                        let old = self
                            .net_blockheads
                            .borrow()
                            .iter()
                            .rev()
                            .find(|b| b.unique_id() == blockhead.unique_id())
                            .cloned();
                        if let Some(old) = old {
                            old.remove_from_macro_block(&old);
                            hooks.blockhead_will_be_unloaded(&old);
                            self.remove_from_list(&self.net_blockheads, &old);
                        }
                        let rules = world.custom_rules();
                        if rules.custom_rules_enabled != 0 && rules.death == 4 {
                            hooks.blockhead_die(&blockhead);
                        }
                        if let Some(s) = self.server.borrow().clone() {
                            s.notify_players_changed();
                        }
                    }
                }
            }
            // 6. LAST net blockhead whose clientID is this client
            let old = self
                .net_blockheads
                .borrow()
                .iter()
                .rev()
                .find(|b| hooks.blockhead_client_id(b).as_deref() == Some(client_id.as_str()))
                .cloned();
            if let Some(old) = old {
                old.remove_from_macro_block(&old);
                hooks.blockhead_will_be_unloaded(&old);
                self.remove_from_list(&self.net_blockheads, &old);
            }
        }
        if let Some(v) = self.disconnected_clients_save_dir_names.borrow_mut().as_mut() {
            v.clear();
        }
    }

    /// `[array removeObject:o]`: removes every element identical to `o`.
    pub(crate) fn remove_from_list(&self, list: &std::cell::RefCell<Vec<ObjRef>>, o: &ObjRef) {
        list.borrow_mut().retain(|x| !ptr_eq(x, o));
    }

    /// `-update:accurateDT:isSimulation:`: the main tick. Step numbers follow contract 5.1 /.
    /// All floats are f32. `conversion_pump` runs first (contract 3.5).
    pub fn update_accurate_dt_is_simulation(&self, dt: f32, accurate_dt: f32, is_simulation: bool) {
        let hooks = self.hooks();
        let world = self.world();
        self.conversion_pump();

        // 1
        self.update_net_objects();

        // 2. local blockheads
        let mut remove_blockheads: Vec<ObjRef> = Vec::new();
        for b in self.blockheads.borrow().clone() {
            b.update_accurate_dt_is_simulation(&b, dt, accurate_dt, is_simulation);
            if b.needs_removed() {
                remove_blockheads.push(b.clone());
            }
            self.send_net_data_if_needed_for_object_is_creation(&b, false);
        }
        // 3. (TipManager x4: no-op, blockheads empty on the server) + pole items
        if !is_simulation {
            hooks.check_and_restore_pole_items(dt);
        }
        // 4
        if !remove_blockheads.is_empty() {
            for b in &remove_blockheads {
                hooks.blockhead_will_be_unloaded(b);
                b.remove_from_macro_block(b);
                self.remove_from_list(&self.blockheads, b);
            }
            self.active_blockhead_index.set(0);
        }
        // 5
        let c = self.send_unreliable_counter.get().wrapping_add(1);
        self.send_unreliable_counter.set(if c == 8 { 0 } else { c });
        // 6
        self.load_any_blockheads_for_disconnected_clients();
        // 7. net blockheads
        let mut remove_net: Vec<ObjRef> = Vec::new();
        for b in self.net_blockheads.borrow().clone() {
            b.update_accurate_dt_is_simulation(&b, dt, accurate_dt, is_simulation);
            if b.needs_removed() {
                remove_net.push(b.clone());
            }
            if self.has_server() {
                self.send_net_data_if_needed_for_object_is_creation(&b, false);
            }
        }
        if !remove_net.is_empty() {
            for b in &remove_net {
                hooks.blockhead_will_be_unloaded(b);
                b.remove_from_macro_block(b);
                self.remove_from_list(&self.net_blockheads, b);
            }
        }
        // 8. disconnected-client blockheads
        let mut remove_local_control: Vec<ObjRef> = Vec::new();
        for b in self.net_blockheads_with_disconnected_clients.borrow().clone() {
            b.update_accurate_dt_is_simulation(&b, dt, accurate_dt, is_simulation);
            if b.needs_removed() {
                remove_local_control.push(b.clone());
            }
            if self.has_server() {
                self.send_net_data_if_needed_for_object_is_creation(&b, false);
            }
        }
        if !remove_local_control.is_empty() {
            self.save_blockheads();
            for b in &remove_local_control {
                hooks.blockhead_will_be_unloaded(b);
                b.remove_from_macro_block(b);
                self.remove_from_list(&self.net_blockheads_with_disconnected_clients, b);
            }
            // server pointer used unchecked in the original; a missing server is a nil message here
            if let Some(s) = self.server.borrow().clone() {
                s.notify_players_changed();
            }
        }
        // 9. `ucomiss`: clamps to a MINIMUM of 0.1
        let mut d = self.free_block_sound_delay.get() - dt;
        if d < 0.1f32 {
            d = 0.1f32;
        }
        self.free_block_sound_delay.set(d);

        // 10. per-type object update
        for t in 0..MAX_DYNAMIC_OBJECT_TYPES {
            let ti = t as i32;
            if !object_type_requires_partial_update(ti) {
                self.update_type_full(t, dt, accurate_dt, is_simulation);
            } else {
                self.update_type_partial(t, dt, accurate_dt, is_simulation);
            }
        }

        // 11. merge pending adds
        for t in 0..MAX_DYNAMIC_OBJECT_TYPES {
            if object_type_requires_partial_update(t as i32) {
                let pending: Vec<ObjRef> = self.dynamic_objects_to_add.borrow()[t].values().cloned().collect();
                for obj in pending {
                    let n = self.do_len(t);
                    let mut r: usize = 0;
                    if n > 0 {
                        let ri = self.random_index.get();
                        r = ((self.random_numbers[ri as usize] as i64) % (n as i64)) as usize;
                        self.random_index.set((ri + 1) % 512);
                    }
                    let mut v = self.partial_update_ordered_objects.borrow_mut();
                    let at = r.min(v[t].len());
                    v[t].insert(at, obj);
                }
            }
            let pending: Vec<(u64, ObjRef)> = {
                let mut m = self.dynamic_objects_to_add.borrow_mut();
                std::mem::take(&mut m[t]).into_iter().collect()
            };
            for (id, o) in pending {
                self.do_insert_new(t, id, o);
            }
            self.currently_adding_object_ids.borrow_mut()[t].clear();
        }

        // 12. world-change fan-out
        let mut do_world = false;
        if !self.world_changed_positions.borrow().is_empty() {
            do_world = true;
            if is_simulation {
                do_world = self.world_changed_simulate_counter.get() == 100;
            }
        }
        let do_contents = !self.world_contents_changed_positions.borrow().is_empty();
        let do_water = !self.water_changed_positions.borrow().is_empty();
        self.world_changed_simulate_counter.set(self.world_changed_simulate_counter.get().wrapping_add(1));
        if do_world || do_contents || do_water {
            let mut wc: Vec<IntPair> = self.world_changed_positions.borrow().clone();
            let mut cc: Vec<IntPair> = self.world_contents_changed_positions.borrow().clone();
            let mut ww: Vec<IntPair> = self.water_changed_positions.borrow().clone();
            if do_world {
                self.world_changed_simulate_counter.set(0);
                self.world_changed_positions.borrow_mut().clear();
            }
            if do_contents {
                self.world_contents_changed_positions.borrow_mut().clear();
            }
            if do_water {
                self.water_changed_positions.borrow_mut().clear();
            }
            let notify = |o: &ObjRef, wc: &mut Vec<IntPair>, cc: &mut Vec<IntPair>, ww: &mut Vec<IntPair>| {
                if do_world {
                    o.world_changed(o, wc);
                }
                if do_contents {
                    o.world_contents_changed(o, cc);
                }
                if do_water {
                    o.water_content_changed(o, ww);
                }
            };
            for b in self.blockheads.borrow().clone() {
                notify(&b, &mut wc, &mut cc, &mut ww);
            }
            for t in 0..MAX_DYNAMIC_OBJECT_TYPES {
                self.for_each_dynamic_object(t, |_, o| notify(o, &mut wc, &mut cc, &mut ww));
            }
        }
        // 13/14: client-only, skipped.
        // 15
        for sc in self.server_client_values() {
            sc.send_all_object_data();
        }
        let _ = world;
    }

    /// Step 10A: a type that is not partially updated. Successor iteration, deferred erase. Contract 5.1 (a), (b).
    fn update_type_full(&self, t: usize, dt: f32, accurate_dt: f32, is_simulation: bool) {
        let ti = t as i32;
        let mut remove_ids: Vec<u64> = Vec::new();
        let mut last: Option<u64> = None;
        while let Some((id, obj)) = self.do_next_after(t, last) {
            obj.update_accurate_dt_is_simulation(&obj, dt, accurate_dt, is_simulation);
            if obj.needs_removed() {
                remove_ids.push(id);
                if object_type_has_static_position(ti) {
                    let idx = self.world_index_key(obj.pos());
                    self.bp_erase_if_same(t, idx, &obj);
                }
                if ti == DYNAMIC_OBJECT_TYPE_FREE_BLOCK {
                    let idx = self.world_index_key(obj.pos());
                    self.free_blocks_erase(idx, &obj);
                }
                obj.remove_from_macro_block(&obj);
            }
            self.send_net_data_if_needed_for_object_is_creation(&obj, false);
            // third `needsRemoved` query is the autorelease: no-op in Rust
            last = Some(id);
        }
        for id in remove_ids {
            self.do_erase(t, id);
        }
    }

    /// Step 10B: types 22 and 29 (partial update). Contract 5.1 (c).
    fn update_type_partial(&self, t: usize, dt: f32, accurate_dt: f32, is_simulation: bool) {
        let ti = t as i32;
        let initial_count = self.partial_update_ordered_objects.borrow()[t].len() as i32;
        if initial_count <= 0 {
            return;
        }
        let mut object_count = initial_count;
        if object_count <= self.partial_update_current_index.borrow()[t] {
            self.partial_update_current_index.borrow_mut()[t] = 0;
        }
        let max = object_partial_update_max(ti);
        let mut si = 0;
        while si < max && si < object_count {
            let this_index = self.partial_update_current_index.borrow()[t];
            let obj = self.partial_update_ordered_objects.borrow()[t].get(this_index as usize).cloned();
            let Some(obj) = obj else { break };
            obj.update_accurate_dt_is_simulation(&obj, dt, accurate_dt, is_simulation);
            if !obj.needs_removed() {
                self.partial_update_current_index.borrow_mut()[t] += 1;
            } else {
                self.do_erase(t, obj.unique_id());
                if object_type_has_static_position(ti) {
                    let idx = self.world_index_key(obj.pos());
                    self.bp_erase_if_same(t, idx, &obj);
                }
                obj.remove_from_macro_block(&obj);
                // swap-remove: vec[thisIndex] = vec.back(); vec.pop_back()
                let mut v = self.partial_update_ordered_objects.borrow_mut();
                if let Some(last) = v[t].pop() {
                    if (this_index as usize) < v[t].len() {
                        v[t][this_index as usize] = last;
                    }
                }
                object_count -= 1;
            }
            if object_count <= self.partial_update_current_index.borrow()[t] {
                self.partial_update_current_index.borrow_mut()[t] = 0;
            }
            si += 1;
        }
        // rain (type 29 only)
        if ti == DYNAMIC_OBJECT_TYPE_SNOW_SURFACEBLOCK {
            let world = self.world();
            let rain: f32 = world.rain_fraction_not_including_snow();
            if rain > 0.04f32 {
                let mut rain_dt = dt;
                if world.fast_forward() {
                    rain_dt = dt / 20.0f32;
                }
                let hooks = self.hooks();
                self.for_each_dynamic_object(t, |_, o| hooks.update_rain_dt(o, rain, rain_dt));
            }
        }
    }
}

/// `players/a/b/rest/` (substringWithRange (0,1), (1,1), (2,len-2)).
fn player_dir_for(client_id: &str) -> String {
    let c: Vec<char> = client_id.chars().collect();
    if c.len() < 2 {
        return "players///".to_string();
    }
    let a: String = c[0..1].iter().collect();
    let b: String = c[1..2].iter().collect();
    let rest: String = c[2..].iter().collect();
    format!("players/{}/{}/{}/", a, b, rest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynworld::state::testutil::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn log() -> Rc<RefCell<Vec<String>>> {
        Rc::new(RefCell::new(vec![]))
    }

    /// contract 5.1 (a)/(b): successor iteration visits ids added during the pass; removal is deferred and
    /// `removeFromMacroBlock` runs once.
    #[test]
    fn update_full_type_with_insert_and_remove() {
        let d = dw("upd_full", 1);
        let l = log();
        let a = Obj::new(7, 10, l.clone());
        a.remove_after_update.set(true);
        let b = Obj::new(7, 20, l.clone());
        let c = Obj::new(7, 15, l.clone());
        d.do_set(7, 10, a.clone());
        d.do_set(7, 20, b.clone());
        let d2 = d.clone();
        let c2: crate::objects::handle::ObjRef = c.clone();
        *a.create_on_update.borrow_mut() = Some(Box::new(move || d2.do_set(7, 15, c2.clone())));
        d.update_accurate_dt_is_simulation(0.5, 0.5, false);
        let got: Vec<String> = l.borrow().clone();
        assert_eq!(got, vec!["upd(7,10,0.5,false)", "rfmb(10)", "upd(7,15,0.5,false)", "upd(7,20,0.5,false)"]);
        assert!(!d.do_contains(7, 10));
        assert!(d.do_contains(7, 15) && d.do_contains(7, 20));
    }

    /// contract 5.1 (d): partial-update merge draws randomNumbers only when the map is non-empty.
    #[test]
    fn partial_merge_rng() {
        let d = dw("upd_partial", 1);
        let l = log();
        // empty map -> index 0, no draw
        let a: crate::objects::handle::ObjRef = Obj::new(22, 1, l.clone());
        d.ta_set(22, 1, a);
        d.update_accurate_dt_is_simulation(0.1, 0.1, false);
        assert_eq!(d.random_index.get(), 0);
        assert_eq!(d.partial_update_ordered_objects.borrow()[22].len(), 1);
        assert_eq!(d.do_len(22), 1);
        // non-empty map -> one draw per pending object
        let b: crate::objects::handle::ObjRef = Obj::new(22, 2, l.clone());
        let c: crate::objects::handle::ObjRef = Obj::new(22, 3, l.clone());
        d.ta_set(22, 2, b);
        d.ta_set(22, 3, c);
        d.update_accurate_dt_is_simulation(0.1, 0.1, false);
        assert_eq!(d.random_index.get(), 2);
        assert_eq!(d.partial_update_ordered_objects.borrow()[22].len(), 3);
    }

    /// contract 5.1 (c): swap-remove invariants of the partial update.
    #[test]
    fn partial_swap_remove() {
        let d = dw("upd_swap", 1);
        let l = log();
        let objs: Vec<Rc<Obj>> = (1..=3).map(|i| Obj::new(22, i, l.clone())).collect();
        for o in &objs {
            let r: crate::objects::handle::ObjRef = o.clone();
            d.do_set(22, o.base.borrow().unique_id, r.clone());
            d.partial_update_ordered_objects.borrow_mut()[22].push(r);
        }
        objs[0].remove_after_update.set(true);
        d.update_accurate_dt_is_simulation(0.1, 0.1, false);
        // first object removed and replaced by the last one; objectCount drops to 2, so the loop ends after 1 and 3
        assert_eq!(d.do_len(22), 2);
        let v = d.partial_update_ordered_objects.borrow()[22].iter().map(|o| o.unique_id()).collect::<Vec<_>>();
        assert_eq!(v, vec![3, 2]);
        let got: Vec<String> = l.borrow().iter().filter(|s| s.starts_with("upd")).cloned().collect();
        assert_eq!(got, vec!["upd(22,1,0.1,false)", "upd(22,3,0.1,false)"]);
    }

    /// contract 5.1 step 5: sendUnreliableCounter cycles 1..7,0.
    #[test]
    fn unreliable_counter_period() {
        let d = dw("upd_cnt", 1);
        let mut seen = vec![];
        for _ in 0..9 {
            d.update_accurate_dt(0.1, 0.1);
            seen.push(d.send_unreliable_counter.get());
        }
        assert_eq!(seen, vec![1, 2, 3, 4, 5, 6, 7, 0, 1]);
    }

    /// `simulate` = 8 steps of dt/8 with isSimulation.
    #[test]
    fn simulate_steps() {
        let d = dw("upd_sim", 1);
        let l = log();
        let a: crate::objects::handle::ObjRef = Obj::new(7, 1, l.clone());
        d.do_set(7, 1, a);
        d.simulate(0.8);
        assert_eq!(l.borrow().len(), 8);
        assert!(l.borrow()[0].ends_with(",true)"));
    }

    ///  hasDynamicObjectsToSaveInMacroPos over 65 vectors + worldChangedMacroPositions.
    #[test]
    fn has_dynamic_objects_to_save() {
        let d = dw("upd_has", 1);
        let p = IntPair { x: 3, y: 4 };
        assert!(!d.has_dynamic_objects_to_save_in_macro_pos(p));
        d.dynamic_world_changed_macro_positions.borrow_mut()[64].push(p);
        assert!(d.has_dynamic_objects_to_save_in_macro_pos(p));
        d.dynamic_world_changed_macro_positions.borrow_mut()[64].clear();
        d.world_changed_macro_positions.borrow_mut().push(p);
        assert!(d.has_dynamic_objects_to_save_in_macro_pos(p));
        assert!(!d.has_dynamic_objects_to_save_in_macro_pos(IntPair { x: 4, y: 3 }));
    }

    /// free_block_sound_delay clamps to a minimum of 0.1.
    #[test]
    fn sound_delay_min() {
        let d = dw("upd_snd", 1);
        d.update_accurate_dt(0.5, 0.5);
        assert_eq!(d.free_block_sound_delay.get(), 0.1f32);
    }

    #[test]
    fn player_dir() {
        assert_eq!(player_dir_for("abcdef"), "players/a/b/cdef/");
    }
}
