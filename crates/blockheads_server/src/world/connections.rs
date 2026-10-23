//! Client connection bookkeeping, remote-object relays, light/zoom helpers, ownership signs, welcome message,
//! mute list, report/password and the small forwards of `World`.
//!

use std::rc::Rc;

use crate::foundation::gsdict::GsDict;
use crate::foundation::gzip::gzip_deflate;
use crate::nslog;
use crate::objects::handle::ObjRef;
use crate::plist::helpers::{data_from_property_list, property_list_from_data};
use crate::plist::value::PlistValue;
use crate::types::netdata::ClientMacroBlockRequest;
use crate::types::structs::{IntPair, Vector2};
use crate::world::lifecycle::{
    core_hooks, int_key, new_mutable_array, new_mutable_dict, same_object, save_dir_for, write_file_atomically,
};
use crate::world::state::{PhysicalBlockRef, ServerClient, TileRef, World};
use crate::world::tiles::{macro_pos_for_world_pos, macro_tile_at_world_postion, tile_at_world_position_loaded};

/// `BHNDT_SERVER_OWNERSHIP_SIGN_MODIFIED_NOTIFATION`
const BHNDT_SERVER_OWNERSHIP_SIGN_MODIFIED_NOTIFATION: u8 = 0x3d;

/// The `NSNull`-safe "null" string of `%@` for a nil argument.
fn s_or_null(s: Option<&str>) -> String {
    s.unwrap_or("(null)").to_string()
}

impl World {
    // =================================================================================================
    // light / small forwards
    // =================================================================================================

    /// `-[World saveSunlightChangedAtPos:]`: `[dynamicWorld lightChangedAtMacroPos:(x/32, y/32) sendReliably:NO]`
    /// (C signed division truncates toward zero).
    pub fn save_sunlight_changed_at_pos(&self, world_pos: IntPair) {
        if self.dynamic_world.borrow().is_some() {
            core_hooks().dynamic_world_light_changed_at_macro_pos_send_reliably(
                IntPair { x: world_pos.x / 32, y: world_pos.y / 32 },
                false,
            );
        }
    }

    /// `-[World allowsRotation]`: `activeBlockhead` is nil on the server so YES.
    pub fn allows_rotation(&self) -> bool {
        true
    }
    /// `-[World requiresSwipeEvents]`: NO.
    pub fn requires_swipe_events(&self) -> bool {
        false
    }
    /// `-[World allowsPanning]`: YES (nil `activeBlockhead`).
    pub fn allows_panning(&self) -> bool {
        true
    }
    /// `-[World requiresDirectionalSwipes]`: NO.
    pub fn requires_directional_swipes(&self) -> bool {
        false
    }
    /// `-[World directionalSwipe:]`: empty (nil fishing rod).
    pub fn directional_swipe(&self, _vec: Vector2) {}
    /// `-[World currentTipText]`: `repairMode ? "-- REPAIR MODE --": TipManager.currentTip`.
    pub fn current_tip_text(&self) -> Option<String> {
        if self.repair_mode() {
            return Some("-- REPAIR MODE --".to_string());
        }
        core_hooks().tip_manager_current_tip()
    }
    /// `-[World currentTipColor]`: white.
    pub fn current_tip_color(&self) -> [f32; 4] {
        [1.0; 4]
    }
    /// `-[World mapVisible]`: `[uiManager mapDisplayed]` on nil is NO.
    pub fn map_visible(&self) -> bool {
        false
    }
    /// `-[World showDoubleTimePromptIfGoodTime]`: empty.
    pub fn show_double_time_prompt_if_good_time(&self) {}
    /// `-[World zoomToPos:pinchZoom:]` (client camera): empty on the server.
    pub fn zoom_to_pos_pinch_zoom(&self, _pos: IntPair, _pinch_zoom: bool) {}
    /// `-[World waterMovedFrom:fromTile:to:toTile:amount:]`: empty (R12).
    pub fn water_moved_from_from_tile_to_to_tile_amount(
        &self,
        _from: IntPair,
        _from_tile: Option<TileRef>,
        _to: IntPair,
        _to_tile: Option<TileRef>,
        _amount: i32,
    ) {
    }
    /// `-[World worldUI]`: nil in the server build.
    pub fn world_ui(&self) -> Option<std::convert::Infallible> {
        None
    }
    /// `-[World reportAchievementWithIdentifier:]`: empty.
    pub fn report_achievement_with_identifier(&self, _id: &str) {}

    /// `-[World activeBlockheadPos]`: `activeBlockhead` is nil (no local blockheads) so `startPortalPos`.
    pub fn active_blockhead_pos(&self) -> IntPair {
        self.start_portal_pos()
    }

    /// `-[World abortInProgressPathIfForBlockhead:]`.
    pub fn abort_in_progress_path_if_for_blockhead(&self, blockhead: &ObjRef) {
        if self.path_creator.borrow().is_none() {
            return;
        }
        let hooks = core_hooks();
        if hooks.path_creator_in_progress() {
            if let Some(u) = hooks.path_creator_path_user() {
                if same_object(&u, blockhead) {
                    hooks.path_creator_abort_path();
                }
            }
        }
    }

    // =================================================================================================
    // client connection bookkeeping
    // =================================================================================================

    /// `-[World clientConnected:]` -> BOOL. Creation order is observable: the `ServerClient` is created and inserted
    /// into `serverClients` BEFORE the light blocks are unarchived, the `DynamicWorld` is told and the start area is
    /// loaded.
    pub fn client_connected(&self, client_id: &str) -> bool {
        let hooks = core_hooks();
        if !self.has_server_client(client_id) {
            // `[nil lastIndex]` (no server) is 0, not NSNotFound
            let idx: Option<u64> = match self.free_client_light_block_indices.borrow().as_ref() {
                Some(set) => set.last_index(),
                None => Some(0),
            };
            match idx {
                Some(idx) => {
                    if let Some(set) = self.free_client_light_block_indices.borrow_mut().as_mut() {
                        set.remove_index(idx);
                    }
                    let lbdb = if self.world_tile_loader.borrow().is_some() {
                        hooks.loader_light_block_database()
                    } else {
                        None
                    };
                    hooks.server_client_create(client_id, (idx & 0xffff_ffff) as u32 as i32, lbdb);
                    let map = self.server_clients.borrow().clone();
                    if let Some(m) = map {
                        m.borrow_mut().insert(client_id.to_string(), Rc::new(ServerClient {}));
                    }
                    if self.world_tile_loader.borrow().is_some() {
                        hooks.loader_unarchive_light_blocks_for_client(client_id);
                    }
                    if self.dynamic_world.borrow().is_some() {
                        hooks.dynamic_world_client_connected(client_id);
                    }
                    hooks.world_fully_load_if_needed_around_pos_client_light_block_index(
                        self,
                        self.start_portal_pos(),
                        idx as i32,
                    );
                    true
                }
                None => {
                    nslog!("No more light block indices available.");
                    false
                }
            }
        } else if !hooks.server_client_connected(client_id) {
            hooks.server_client_reconnected(client_id);
            if self.dynamic_world.borrow().is_some() {
                hooks.dynamic_world_client_connected(client_id);
            }
            let lbi = hooks.server_client_light_block_index(client_id);
            hooks.world_fully_load_if_needed_around_pos_client_light_block_index(self, self.start_portal_pos(), lbi);
            true
        } else {
            false
        }
    }

    /// `-[World clientDisconnected:wasKick:]`: logs, runs the three-branch logic, forces a save with `saveCounter =
    /// 999` and calls `pre_update(0.1)` IMMEDIATELY (each disconnect is one full save + LMDB commit). Removing the
    /// `ServerClient` is NOT done here (`World.update` does it). No `RefCell` borrow is held across the calls.
    pub fn client_disconnected_was_kick(&self, client_id: &str, was_kick: bool) {
        let hooks = core_hooks();
        let name = self.world_name.borrow().clone();
        nslog!("{} - Client disconnected:{}", s_or_null(name.as_deref()), client_id);
        let has_dw = self.dynamic_world.borrow().is_some();
        if was_kick && has_dw {
            hooks.dynamic_world_stop_all_blockhead_actions_for_client_due_to_kick(client_id);
        }
        if self.server_client_is_connected(client_id) {
            crate::world::simulation::away_events_remove_client(client_id);
            if has_dw {
                hooks.dynamic_world_client_disconnected_simulate(client_id, !was_kick);
            }
            hooks.server_client_set_connected(client_id, false);
        } else if was_kick {
            crate::world::simulation::away_events_remove_client(client_id);
            if has_dw {
                hooks.dynamic_world_client_disconnected_simulate(client_id, false);
            }
        }
        self.set_save_counter(999);
        self.pre_update(0.1f32);
    }

    /// `-[World requestForBlock:fromClient:]`: ignored for a nil / unconnected client.
    pub fn request_for_block_from_client(&self, request: ClientMacroBlockRequest, client_id: &str) {
        if self.server_client_is_connected(client_id) {
            core_hooks().server_client_request_for_block(client_id, request);
        }
    }

    /// `-[World remoteBlockRemoved:byClient:]`.
    pub fn remote_block_removed_by_client(&self, macro_index: i32, client_id: &str) {
        if self.server_client_is_connected(client_id) {
            core_hooks().server_client_block_removed(client_id, macro_index);
        }
    }

    /// `-[World peersInterestedInMacroIndex:]`: ids (in `serverClients` GsDict key order) of connected clients that
    /// have the block wired; empty without a server.
    pub fn peers_interested_in_macro_index(&self, macro_index: i32) -> Vec<String> {
        let mut ret = Vec::new();
        if self.server.borrow().is_none() {
            return ret;
        }
        let hooks = core_hooks();
        for id in self.server_client_keys() {
            if hooks.server_client_block_is_wired(&id, macro_index) && self.server_client_is_connected(&id) {
                ret.push(id);
            }
        }
        ret
    }

    // =================================================================================================
    // remote object relays
    // =================================================================================================

    /// `-[World remoteCreate:forObjectsOfType:clientID:]`: forwarded to the `DynamicWorld`, or queued per client in
    /// `initialCreateObjects` while loading.
    pub fn remote_create_for_objects_of_type_client_id(
        &self,
        creation_array: Vec<Vec<u8>>,
        object_type: i32,
        client_id: &str,
    ) {
        if self.dynamic_world.borrow().is_some() {
            core_hooks().dynamic_world_remote_create(creation_array, object_type, Some(client_id));
            return;
        }
        let mut q = self.initial_create_objects.borrow_mut();
        let create = q.create.get_or_insert_with(GsDict::new);
        let key = client_id.to_string();
        if create.get(&key).is_none() {
            create.insert(key.clone(), GsDict::new());
        }
        let per_client = create.get_mut(&key).expect("just inserted");
        let k = int_key(object_type);
        if per_client.get(&k).is_none() {
            per_client.insert(k, Vec::new());
        }
        per_client.get_mut(&k).expect("just inserted").extend(creation_array);
    }

    /// Shared queue step of the three other relays.
    fn queue_initial(&self, which: u8, objects: Vec<Vec<u8>>, object_type: i32) {
        let mut q = self.initial_create_objects.borrow_mut();
        let slot = match which {
            0 => &mut q.update,
            1 => &mut q.creation_data_update,
            _ => &mut q.remove,
        };
        let dict = slot.get_or_insert_with(GsDict::new);
        let k = int_key(object_type);
        if dict.get(&k).is_none() {
            dict.insert(k, Vec::new());
        }
        dict.get_mut(&k).expect("just inserted").extend(objects);
    }

    /// `-[World remoteUpdate:forObjectsOfType:fromClient:]`: queue is `initialUpdateObjects` (not per client).
    pub fn remote_update_for_objects_of_type_from_client(
        &self,
        update_array: Vec<Vec<u8>>,
        object_type: i32,
        from_client: Option<&str>,
    ) {
        if self.dynamic_world.borrow().is_some() {
            core_hooks().dynamic_world_remote_update(update_array, object_type, from_client);
            return;
        }
        self.queue_initial(0, update_array, object_type);
    }

    /// `-[World remoteCreationDataUpdate:forObjectsOfType:fromClient:]`: queue `initialCreationDataUpdateObjects`.
    pub fn remote_creation_data_update_for_objects_of_type_from_client(
        &self,
        update_array: Vec<Vec<u8>>,
        object_type: i32,
        from_client: Option<&str>,
    ) {
        if self.dynamic_world.borrow().is_some() {
            core_hooks().dynamic_world_remote_creation_data_update(update_array, object_type, from_client);
            return;
        }
        self.queue_initial(1, update_array, object_type);
    }

    /// `-[World remoteRemove:forObjectsOfType:fromClient:]`: queue `initialRemoveObjects`.
    pub fn remote_remove_for_objects_of_type_from_client(
        &self,
        remove_array: Vec<Vec<u8>>,
        object_type: i32,
        from_client: Option<&str>,
    ) {
        if self.dynamic_world.borrow().is_some() {
            core_hooks().dynamic_world_remote_remove(remove_array, object_type, from_client);
            return;
        }
        self.queue_initial(2, remove_array, object_type);
    }

    /// `-[World serverFillReply:]`: client receive path, `[b fillReceiptsReturned:]` on every local blockhead; the
    /// server has none.
    pub fn server_fill_reply(&self, _fill_reply_receipts: &[u8]) {}

    // =================================================================================================
    // forwards to the DynamicWorld / loader
    // =================================================================================================

    /// `-[World currentTotalBlockheadCountIncludingNet]`.
    pub fn current_total_blockhead_count_including_net(&self) -> i32 {
        if self.dynamic_world.borrow().is_none() {
            return 0;
        }
        core_hooks().dynamic_world_all_blockheads_including_net().len() as i32
    }

    /// `-[World isControllingBlockheadsForClientPlayer:]`.
    pub fn is_controlling_blockheads_for_client_player(&self, player_id: &str) -> bool {
        self.dynamic_world.borrow().is_some()
            && core_hooks().dynamic_world_is_controlling_blockheads_for_client_player(player_id)
    }

    /// `-[World playersChanged]` (`uiManager` is nil): forwards to the `DynamicWorld`.
    pub fn players_changed(&self) {
        if self.dynamic_world.borrow().is_some() {
            core_hooks().dynamic_world_players_changed();
        }
    }

    /// `-[World updatePhysicalBlockToLatestVersion:]` -> `[worldTileLoader updatePhysicalBlockToLatestVersion:]`.
    pub fn update_physical_block_to_latest_version(&self, physical_block: PhysicalBlockRef) {
        if self.world_tile_loader.borrow().is_some() {
            core_hooks().loader_update_physical_block_to_latest_version(physical_block);
        }
    }

    /// `-[World tileIsLitForClient:atPos:tile:]`. A nil client id (or an unknown one) reads light block index 0; an
    /// unavailable light block is treated as lit.
    pub fn tile_is_lit_for_client_at_pos_tile(
        &self,
        client_id: Option<&str>,
        tile_pos: IntPair,
        tile: Option<TileRef>,
    ) -> bool {
        if (self.server.borrow().is_none() || client_id.is_none()) && tile.is_some() {
            return tile.expect("checked").light() != 0;
        }
        let idx: i32 = match client_id {
            Some(id) if self.has_server_client(id) => core_hooks().server_client_light_block_index(id),
            _ => 0, // nil client / nil dictionary: `[nil lightBlockIndex]` is 0
        };
        if idx >= 0 {
            let macro_tiles = self.macro_tiles();
            if let Some(mt) = macro_tile_at_world_postion(tile_pos.x, tile_pos.y, &macro_tiles, self) {
                if let Some(pb) = mt.physical_block() {
                    if pb.valid.get() {
                        let slot = idx as usize;
                        let have = |pb: PhysicalBlockRef| {
                            pb.client_light_blocks.borrow().get(slot).map_or(false, |s| s.is_some())
                        };
                        if !have(pb) {
                            core_hooks()
                                .world_load_light_block_for_client_light_block_index_into_physical_block(self, idx, pb);
                        }
                        let lbs = pb.client_light_blocks.borrow();
                        if let Some(Some(lb)) = lbs.get(slot) {
                            let w = self.world_width_macro;
                            let n = w.wrapping_mul(32);
                            let dx = tile_pos.x.wrapping_sub(pb.x_pos.get().wrapping_mul(32));
                            let lx = if dx >= n / 2 {
                                dx.wrapping_sub(n)
                            } else if dx < w.wrapping_mul(-32) / 2 {
                                dx.wrapping_add(n)
                            } else {
                                dx
                            };
                            let off = tile_pos
                                .y
                                .wrapping_sub(pb.y_pos.get().wrapping_mul(32))
                                .wrapping_mul(32)
                                .wrapping_add(lx);
                            // UB-PARITY: the original reads `lb[off]` unchecked; an out-of-range offset reads
                            // adjacent heap. Treat it as lit (the "unlit data unavailable" result).
                            if off >= 0 && (off as usize) < lb.len() {
                                return lb[off as usize] != 0;
                            }
                        }
                    }
                }
            }
        }
        true
    }

    /// `-[World getBlockheadZoomPosForClient:requesterClient:cycleIndex:]`: handler for 0x1b. The modulus is taken on
    /// the sign-extended unsigned 64-bit value. Result defaults to `(-1, -1)`.
    pub fn get_blockhead_zoom_pos_for_client_requester_client_cycle_index(
        &self,
        player_id: Option<&str>,
        requester_client: Option<&str>,
        cycle_index: i32,
    ) -> IntPair {
        let none = IntPair { x: -1, y: -1 };
        let player_id = match player_id {
            Some(p) => p,
            None => return none,
        };
        if self.dynamic_world.borrow().is_none() {
            return none;
        }
        let hooks = core_hooks();
        let mut owned: Vec<IntPair> = Vec::new();
        for b in hooks.dynamic_world_all_blockheads_including_net() {
            let cid = hooks.blockhead_client_id(&b);
            let mut matched = cid.as_deref() == Some(player_id);
            if !matched
                && self.server.borrow().is_some()
                && cid.is_none()
                && hooks.server_local_player_id().as_deref() == Some(player_id)
            {
                matched = true; // host's own blockheads
            }
            if matched {
                let p = b.base().borrow().pos;
                let t = tile_at_world_position_loaded(p.x, p.y, self);
                if self.tile_is_lit_for_client_at_pos_tile(requester_client, p, t) {
                    owned.push(p);
                }
            }
        }
        if owned.is_empty() {
            return none;
        }
        owned[((cycle_index as i64 as u64) % owned.len() as u64) as usize]
    }

    /// `-[World markCircumNavigateX:]`: achievement tracker over the visited x columns (contiguous fill). The
    /// achievement report is a no-op.
    pub fn mark_circum_navigate_x(&self, x: i32) {
        let n = self.world_width_macro.wrapping_mul(32);
        let mut x = x;
        if x < 0 {
            x = x.wrapping_add(n);
        } else if x >= n {
            x = x.wrapping_sub(n);
        }
        let mut guard = self.circum_navigate_booleans.borrow_mut();
        let set = match guard.as_mut() {
            Some(s) => s,
            None => return, // every message to nil is a no-op / NO
        };
        let u = |v: i32| v as i64 as u64;
        if set.contains_index(u(x)) {
            return;
        }
        set.add_index(u(x));
        let next = if n != 0 { x.wrapping_add(1).wrapping_rem(n) } else { 0 };
        if !set.contains_index(u(next)) && x > 0 && !set.contains_index(u(x - 1)) {
            if x > 1 && !set.contains_index(u(x - 2)) {
                set.add_index(u(next)); // fill gap on the right
            } else {
                set.add_index(u(x - 1)); // fill gap on the left
            }
        }
        if (set.count() as u64) >= (n.wrapping_sub(8)) as i64 as u64 {
            drop(guard);
            self.report_achievement_with_identifier("grp.circumnavigate");
        }
    }

    // =================================================================================================
    // welcome message, passwords, privacy
    // =================================================================================================

    /// `-[World welcomeMessageDictRecieved:fromClient:]`: `welcomeMessage = dict["newWelcomeMessage"]` (a nil dict or
    /// missing key gives nil). No permission check here (BHServer checks admin).
    pub fn welcome_message_dict_recieved_from_client(&self, message_dict: Option<&PlistValue>, _peer: &str) {
        // Note: a non-string object under the key would be stored as is by the original; only strings are kept.
        *self.welcome_message.borrow_mut() = message_dict
            .and_then(|d| d.object_for_key("newWelcomeMessage"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
    }

    /// `-[World setNewWelcomeMessage:]`, server branch: a plain setter.
    pub fn set_new_welcome_message(&self, m: Option<&str>) {
        if self.server.borrow().is_some() {
            *self.welcome_message.borrow_mut() = m.map(|s| s.to_string());
        }
    }

    /// `-[World newServerPasswordSet:]`.
    pub fn new_server_password_set(&self, new_password: Option<&str>) {
        *self.server_password.borrow_mut() = new_password.map(|s| s.to_string());
    }

    /// `-[World isCloudGame]`.
    pub fn is_cloud_game(&self) -> bool {
        self.server.borrow().is_some() && core_hooks().server_is_cloud_match()
    }

    /// `-[World reportUserWithName:reporterName:reporterMessage:reportedAvatarImagePath:]`, server branch. The
    /// non-cloud C format ends in `\n`, so it is logged without an extra newline (CRITIC C1).
    pub fn report_user_with_name_reporter_name_reporter_message_reported_avatar_image_path(
        &self,
        player_name: Option<&str>,
        reporter_name: Option<&str>,
        reporter_message: Option<&str>,
        reported_avatar_image_path: Option<&str>,
    ) {
        if self.server.borrow().is_none() {
            return;
        }
        let hooks = core_hooks();
        let rn = match reporter_name {
            Some(n) => n.to_string(),
            None => hooks.server_local_player_name(),
        };
        if !hooks.server_is_cloud_match() {
            crate::foundation::nslog::nslog_fmt_nl(&format!(
                "REPORT-USER:{}\nREPORTER-NAME:{}\nREPORTER-EMAIL:{}\nREPORTER-MESSAGE:{}\n",
                s_or_null(player_name),
                rn,
                "no_email",
                s_or_null(reporter_message)
            ));
        } else {
            let mut add = String::new();
            if let Some(p) = reported_avatar_image_path {
                if !p.is_empty() {
                    add = format!("REPORTED-AVATAR:<<{}>>", p);
                }
            }
            let msg = reporter_message.map(|m| m.replace("\n", "\\n"));
            nslog!(
                "CLOUD:REPORT-USER:<<{}>>REPORTER-NAME:<<{}>>REPORTER-EMAIL:<<{}>>REPORTER-MESSAGE:<<{}>>{}",
                s_or_null(player_name),
                rn,
                "no_email",
                s_or_null(msg.as_deref()),
                add
            );
        }
    }

    // =================================================================================================
    // mute list
    // =================================================================================================

    fn mute_list_path(&self) -> String {
        format!("{}/default_mutelist", save_dir_for(&self.save_id.borrow().clone().unwrap_or_default()))
    }

    /// `-[World initMuteList]`: load `saves/<id>/default_mutelist` (XML array of strings).
    pub fn init_mute_list(&self) {
        if !self.muted_players.borrow().is_empty() {
            return; // mutedPlayers != nil (an empty array is indistinguishable from nil here: reloading is idempotent)
        }
        if let Ok(bytes) = std::fs::read(self.mute_list_path()) {
            if let Some(PlistValue::Array(a)) = property_list_from_data(&bytes) {
                *self.muted_players.borrow_mut() =
                    a.items.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
            }
        }
    }

    /// `-[World playerIsMuted:]`.
    pub fn player_is_muted(&self, player_id: &str) -> bool {
        self.init_mute_list();
        self.muted_players.borrow().iter().any(|p| p == player_id)
    }

    /// `-[World setPlayerMuted:withID:]`: updates the list, tells the `DynamicWorld`, writes the file (the `saveQueue`
    /// block runs synchronously).
    pub fn set_player_muted_with_id(&self, muted: bool, player_id: &str) {
        self.init_mute_list();
        {
            let mut m = self.muted_players.borrow_mut();
            if !muted {
                m.retain(|p| p != player_id);
            } else if !m.iter().any(|p| p == player_id) {
                m.push(player_id.to_string());
            }
        }
        if self.dynamic_world.borrow().is_some() {
            core_hooks().dynamic_world_user_mute_changed(player_id);
        }
        let arr =
            new_mutable_array(self.muted_players.borrow().iter().map(|s| PlistValue::string(s.clone())).collect());
        if let Some(d) = data_from_property_list(&arr) {
            let _ = write_file_atomically(&self.mute_list_path(), &d);
        }
    }

    // =================================================================================================
    // ownership signs
    // =================================================================================================

    /// `-[World updateLocalOwnershipSignsDueToSignPlacedOrChangedAtPos:withLandOwner:widthRadius:heightRadius:wasRemoved:]`:
    /// maintains `ownershipSignPositions` (key `"%d" % (macroPos.y * worldWidthMacro + macroPos.x)`, value an array
    /// of `{x, y, o?, w?, h?}`).
    pub fn update_local_ownership_signs_due_to_sign_placed_or_changed_at_pos_with_land_owner_width_radius_height_radius_was_removed(
        &self,
        tile_pos: IntPair,
        land_owner_id: Option<&str>,
        width_radius: i32,
        height_radius: i32,
        was_removed: bool,
    ) {
        let mp = macro_pos_for_world_pos(tile_pos, self);
        let key = format!("{}", mp.y.wrapping_mul(self.world_width_macro).wrapping_add(mp.x));
        let mut guard = self.ownership_sign_positions.borrow_mut();
        if guard.is_none() && !was_removed {
            *guard = Some(new_mutable_dict());
        }
        if let Some(dict) = guard.as_mut() {
            if dict.map.get(&key).is_none() && !was_removed {
                dict.map.insert(key.clone(), PlistValue::mutable_array());
            }
            if let Some(PlistValue::Array(signs)) = dict.map.get_mut(&key) {
                let mut i = 0;
                while i < signs.items.len() {
                    let e = &signs.items[i];
                    let ex = e.object_for_key("x").map_or(0, |v| v.int_value());
                    let ey = e.object_for_key("y").map_or(0, |v| v.int_value());
                    if ex == tile_pos.x && ey == tile_pos.y {
                        signs.items.remove(i);
                    } else {
                        i += 1;
                    }
                }
                if !was_removed {
                    let mut e = new_mutable_dict();
                    e.map.insert("x".to_string(), PlistValue::int(tile_pos.x as i64));
                    e.map.insert("y".to_string(), PlistValue::int(tile_pos.y as i64));
                    if let Some(o) = land_owner_id {
                        e.map.insert("o".to_string(), PlistValue::string(o));
                    }
                    if width_radius != 15 {
                        e.map.insert("w".to_string(), PlistValue::int(width_radius as i64));
                    }
                    if height_radius != 15 {
                        e.map.insert("h".to_string(), PlistValue::int(height_radius as i64));
                    }
                    signs.items.push(PlistValue::Dict(e));
                }
            }
            if was_removed {
                let empty = matches!(dict.map.get(&key), Some(PlistValue::Array(a)) if a.items.is_empty());
                if empty {
                    dict.map.remove(&key);
                }
            }
        }
        drop(guard);
        // `[self displayOwnershipAreas]` (world_ops) is empty on the server.
    }

    /// `-[World ownershipSignWasPlacedOrChangedAtPos:withLandOwner:widthRadius:heightRadius:wasRemoved:]`: server
    /// entry called by sign objects; also broadcasts `0x3d` + `gzip(plist)` to everybody. `removed` is present only
    /// when true, `w` / `h` only when != 15.
    pub fn ownership_sign_was_placed_or_changed_at_pos_with_land_owner_width_radius_height_radius_was_removed(
        &self,
        tile_pos: IntPair,
        land_owner_id: Option<&str>,
        width_radius: i32,
        height_radius: i32,
        was_removed: bool,
    ) {
        self.update_local_ownership_signs_due_to_sign_placed_or_changed_at_pos_with_land_owner_width_radius_height_radius_was_removed(
            tile_pos,
            land_owner_id,
            width_radius,
            height_radius,
            was_removed,
        );
        if self.server.borrow().is_some() {
            let mut d = new_mutable_dict();
            d.map.insert("x".to_string(), PlistValue::int(tile_pos.x as i64));
            d.map.insert("y".to_string(), PlistValue::int(tile_pos.y as i64));
            if let Some(o) = land_owner_id {
                d.map.insert("o".to_string(), PlistValue::string(o));
            }
            if was_removed {
                d.map.insert("removed".to_string(), PlistValue::boolean(true));
            }
            if width_radius != 15 {
                d.map.insert("w".to_string(), PlistValue::int(width_radius as i64));
            }
            if height_radius != 15 {
                d.map.insert("h".to_string(), PlistValue::int(height_radius as i64));
            }
            let mut pkt = vec![BHNDT_SERVER_OWNERSHIP_SIGN_MODIFIED_NOTIFATION];
            if let Some(x) = data_from_property_list(&PlistValue::Dict(d)) {
                pkt.extend_from_slice(&gzip_deflate(&x));
            }
            self.send_to_peers(&pkt, None, true);
        }
        self.set_ownership_sign_positions_need_saving(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::exception::catch_ns;
    use crate::foundation::gzip::gzip_inflate;
    use crate::foundation::indexset::IndexSet;
    use crate::world::lifecycle::testkit::*;

    fn bh(f: &Fixture, id: u64, client: Option<&str>, pos: IntPair) -> ObjRef {
        let o = crate::world::helpers::testutil::new_obj();
        {
            let mut b = o.base().borrow_mut();
            b.unique_id = id;
            b.pos = pos;
        }
        f.rec.bh.borrow_mut().insert(id, Bh { client_id: client.map(|s| s.to_string()), ..Default::default() });
        f.rec.blockheads.borrow_mut().push(o.clone());
        o
    }

    #[test]
    fn light_block_indices_are_handed_out_from_31_down_and_the_33rd_client_is_refused() {
        let f = fixture(32);
        let w = &f.world;
        *w.world_tile_loader.borrow_mut() = Some(Rc::new(crate::world::state::WorldTileLoader {}));
        with_dw(w);
        assert!(w.client_connected("c0"));
        assert!(w.client_connected("c1"));
        assert_eq!(f.rec.sc_created.borrow().clone(), vec![("c0".to_string(), 31), ("c1".to_string(), 30)]);
        let calls = f.rec.calls.borrow().clone();
        // creation order: ServerClient, then unarchive, then dynamicWorld, then fullyLoad
        let pos = |p: &str| calls.iter().position(|c| c == p).unwrap_or_else(|| panic!("{} in {:?}", p, calls));
        assert!(pos("sc_create") < pos("unarchive c0"));
        assert!(pos("unarchive c0") < pos("dw_client_connected c0"));
        assert!(pos("dw_client_connected c0") < pos("fully_load 0 0 31"));
        for i in 2..32 {
            assert!(w.client_connected(&format!("c{}", i)));
        }
        let log = capture_log(|| assert!(!w.client_connected("c32")));
        assert!(strip_log(&log).contains(&"No more light block indices available.".to_string()));
        // reconnect of a connected client is rejected; of a disconnected one it reuses its index
        assert!(!w.client_connected("c0"));
        f.rec.clients.borrow_mut().get_mut("c0").unwrap().connected = false;
        assert!(w.client_connected("c0"));
        assert_eq!(f.rec.count("sc_reconnected"), 1);
        assert!(f.rec.calls.borrow().contains(&"fully_load 0 0 31".to_string()));
    }

    #[test]
    fn disconnect_logs_saves_immediately_and_is_idempotent_in_effects() {
        let f = fixture(32);
        let w = &f.world;
        with_dw(w);
        assert!(w.client_connected("a"));
        let log = capture_log(|| w.client_disconnected_was_kick("a", false));
        assert!(strip_log(&log).contains(&"TESTWORLD - Client disconnected:a".to_string()), "{}", log);
        assert!(f.rec.calls.borrow().contains(&"dw_client_disconnected a true".to_string()));
        assert!(!f.rec.clients.borrow()["a"].connected);
        assert_eq!(f.rec.saved.borrow().len(), 1, "saveCounter = 999 forces a save inside the re-entrant preUpdate");
        assert_eq!(w.save_counter(), 0);
        // second disconnect: logs and saves again, no dynamic world call
        let log = capture_log(|| w.client_disconnected_was_kick("a", false));
        assert_eq!(strip_log(&log).iter().filter(|l| l.contains("Client disconnected:a")).count(), 1);
        assert_eq!(f.rec.saved.borrow().len(), 2);
        assert_eq!(f.rec.count("dw_client_disconnected"), 1);
    }

    #[test]
    fn kick_clears_away_events_and_stops_blockhead_actions() {
        let f = fixture(32);
        let w = &f.world;
        with_dw(w);
        crate::world::simulation::with_away_client_simulation_events(|a| {
            let mut m = GsDict::new();
            m.insert("a".to_string(), GsDict::new());
            *a = Some(m);
        });
        assert!(w.client_connected("a"));
        w.client_disconnected_was_kick("a", true);
        let calls = f.rec.calls.borrow().clone();
        assert!(calls.contains(&"dw_stop_actions a".to_string()));
        assert!(calls.contains(&"dw_client_disconnected a false".to_string()));
        assert!(crate::world::simulation::with_away_client_simulation_events(|a| a.as_ref().unwrap().len() == 0));
        // kick of an unknown client still clears and calls the dynamic world with simulate = NO
        w.client_disconnected_was_kick("ghost", true);
        assert!(f.rec.calls.borrow().contains(&"dw_client_disconnected ghost false".to_string()));
    }

    #[test]
    fn block_requests_only_for_connected_clients_and_peers_interested_in_dict_order() {
        let f = fixture(32);
        let w = &f.world;
        for id in ["alpha", "beta", "gamma"] {
            assert!(w.client_connected(id));
        }
        w.request_for_block_from_client(ClientMacroBlockRequest { macro_index: 7, ..Default::default() }, "alpha");
        w.request_for_block_from_client(ClientMacroBlockRequest { macro_index: 8, ..Default::default() }, "nobody");
        assert_eq!(f.rec.clients.borrow()["alpha"].requested, vec![7]);
        f.rec.clients.borrow_mut().get_mut("beta").unwrap().connected = false;
        w.request_for_block_from_client(ClientMacroBlockRequest { macro_index: 9, ..Default::default() }, "beta");
        assert!(f.rec.clients.borrow()["beta"].requested.is_empty());
        w.remote_block_removed_by_client(3, "alpha");
        assert_eq!(f.rec.clients.borrow()["alpha"].removed, vec![3]);
        for id in ["alpha", "beta", "gamma"] {
            f.rec.clients.borrow_mut().get_mut(id).unwrap().wired.insert(5);
        }
        let got = w.peers_interested_in_macro_index(5);
        let order = w.server_client_keys();
        let want: Vec<String> = order.into_iter().filter(|k| k != "beta").collect();
        assert_eq!(got, want, "GsDict key order, beta is disconnected");
        *w.server.borrow_mut() = None;
        assert!(w.peers_interested_in_macro_index(5).is_empty());
    }

    #[test]
    fn zoom_position_uses_unsigned_modulo_of_the_sign_extended_cycle_index() {
        let f = fixture(32);
        let w = &f.world;
        with_dw(w);
        *f.rec.local_player_id.borrow_mut() = Some("host".into());
        // three lit blockheads of player "p" (world has no loaded blocks: tile == NULL, requester nil -> falls to the
        // per-client path which treats "unlit data unavailable" as lit)
        bh(&f, 1, Some("p"), IntPair { x: 10, y: 100 });
        bh(&f, 2, Some("p"), IntPair { x: 20, y: 100 });
        bh(&f, 3, Some("p"), IntPair { x: 30, y: 100 });
        bh(&f, 4, None, IntPair { x: 99, y: 99 }); // host's own
        bh(&f, 5, Some("q"), IntPair { x: 55, y: 55 });
        let z = |p: Option<&str>, c: i32| w.get_blockhead_zoom_pos_for_client_requester_client_cycle_index(p, None, c);
        assert_eq!(z(Some("p"), 0), IntPair { x: 10, y: 100 });
        assert_eq!(z(Some("p"), 1), IntPair { x: 20, y: 100 });
        assert_eq!(z(Some("p"), 3), IntPair { x: 10, y: 100 });
        // -1 sign-extends to u64::MAX; u64::MAX % 3 == 0
        assert_eq!(z(Some("p"), -1), IntPair { x: 10, y: 100 });
        // -2 -> (2^64 - 2) % 3 == 2 -> third blockhead (a plain i32 modulo would give -2 and panic or wrap)
        assert_eq!(z(Some("p"), -2), IntPair { x: 30, y: 100 });
        assert_eq!(z(Some("host"), 0), IntPair { x: 99, y: 99 });
        assert_eq!(z(Some("zzz"), 0), IntPair { x: -1, y: -1 });
        assert_eq!(z(None, 0), IntPair { x: -1, y: -1 });
    }

    #[test]
    fn tile_is_lit_rules() {
        let f = fixture(32);
        let w = &f.world;
        let pb = w.take_physical_block();
        pb.valid.set(true);
        pb.x_pos.set(0);
        pb.y_pos.set(0);
        *w.macro_tiles.get(0).unwrap().physical_block.borrow_mut() = Some(pb);
        let t = TileRef::new(pb, 5);
        // server + nil client id: tile light
        t.set_light(0);
        assert!(!w.tile_is_lit_for_client_at_pos_tile(None, IntPair { x: 5, y: 0 }, Some(t.clone())));
        t.set_light(9);
        assert!(w.tile_is_lit_for_client_at_pos_tile(None, IntPair { x: 5, y: 0 }, Some(t.clone())));
        // known client: light block byte
        assert!(w.client_connected("c"));
        let idx = f.rec.sc_created.borrow()[0].1 as usize;
        let mut lb = Box::new([0u8; 1024]);
        lb[5] = 1;
        pb.client_light_blocks.borrow_mut()[idx] = Some(lb);
        assert!(w.tile_is_lit_for_client_at_pos_tile(Some("c"), IntPair { x: 5, y: 0 }, Some(t.clone())));
        assert!(!w.tile_is_lit_for_client_at_pos_tile(Some("c"), IntPair { x: 6, y: 0 }, Some(t.clone())));
        // block missing for that client and nothing loads it: treated as lit
        pb.client_light_blocks.borrow_mut()[idx] = None;
        assert!(w.tile_is_lit_for_client_at_pos_tile(Some("c"), IntPair { x: 6, y: 0 }, Some(t.clone())));
        // unknown client id reads light block index 0
        let mut lb0 = Box::new([0u8; 1024]);
        lb0[6] = 1;
        pb.client_light_blocks.borrow_mut()[0] = Some(lb0);
        assert!(w.tile_is_lit_for_client_at_pos_tile(Some("nobody"), IntPair { x: 6, y: 0 }, Some(t)));
        // no block at all for the position: lit
        assert!(w.tile_is_lit_for_client_at_pos_tile(Some("c"), IntPair { x: 500, y: 500 }, None));
    }

    #[test]
    fn circumnavigation_marks_and_fills_gaps() {
        let f = fixture(32);
        let w = &f.world;
        *w.circum_navigate_booleans.borrow_mut() = Some(IndexSet::new());
        let has = |x: u64| w.circum_navigate_booleans.borrow().as_ref().unwrap().contains_index(x);
        w.mark_circum_navigate_x(10);
        assert!(has(10));
        // x = 10: next (11) missing, 9 missing, 8 missing and x > 1 -> fills the right neighbour 11
        assert!(has(11) && !has(9));
        w.mark_circum_navigate_x(10); // already visited: no change
        w.mark_circum_navigate_x(1);
        // x = 1: 2 missing, 0 missing, x > 1 false -> fills the left neighbour 0
        assert!(has(1) && has(0) && !has(2));
        w.mark_circum_navigate_x(-1); // wraps to 1023
        assert!(has(1023));
        w.mark_circum_navigate_x(1024 + 4); // wraps to 4
        assert!(has(4));
        // nil set: no-op
        *w.circum_navigate_booleans.borrow_mut() = None;
        w.mark_circum_navigate_x(3);
    }

    #[test]
    fn ownership_signs_add_replace_remove_and_broadcast() {
        let f = fixture(32);
        let w = &f.world;
        let pos = IntPair { x: 40, y: 7 }; // macro (1, 0) -> key "1"
        w.ownership_sign_was_placed_or_changed_at_pos_with_land_owner_width_radius_height_radius_was_removed(
            pos,
            Some("owner1"),
            15,
            15,
            false,
        );
        assert!(w.ownership_sign_positions_need_saving());
        let entries = |w: &World| -> Vec<PlistValue> {
            let guard = w.ownership_sign_positions.borrow();
            let Some(PlistValue::Array(a)) = guard.as_ref().and_then(|d| d.map.get(&"1".to_string())) else {
                return vec![];
            };
            a.items.clone()
        };
        let e = entries(w);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].object_for_key("x").unwrap().int_value(), 40);
        assert_eq!(e[0].object_for_key("o").unwrap().as_str(), Some("owner1"));
        assert!(e[0].object_for_key("w").is_none() && e[0].object_for_key("h").is_none());
        // broadcast: 0x3d + gzip(plist) to everybody, reliable, no `removed`, no w/h
        {
            let sends = f.rec.sends.borrow();
            assert_eq!(sends.len(), 1);
            assert_eq!(sends[0].0[0], 0x3d);
            assert_eq!((sends[0].1.clone(), sends[0].2), (None, true));
            let d = crate::plist::helpers::property_list_from_data(&gzip_inflate(&sends[0].0[1..]).unwrap()).unwrap();
            assert!(d.object_for_key("removed").is_none() && d.object_for_key("w").is_none());
        }
        // same tile again with new radii replaces the entry
        w.ownership_sign_was_placed_or_changed_at_pos_with_land_owner_width_radius_height_radius_was_removed(
            pos,
            Some("owner2"),
            10,
            15,
            false,
        );
        let e = entries(w);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].object_for_key("o").unwrap().as_str(), Some("owner2"));
        assert_eq!(e[0].object_for_key("w").unwrap().int_value(), 10);
        // another tile in the same bucket
        w.update_local_ownership_signs_due_to_sign_placed_or_changed_at_pos_with_land_owner_width_radius_height_radius_was_removed(IntPair { x: 41, y: 7 }, None, 15, 15, false);
        assert_eq!(entries(w).len(), 2);
        // removal of one keeps the bucket, removal of the last drops the key
        w.update_local_ownership_signs_due_to_sign_placed_or_changed_at_pos_with_land_owner_width_radius_height_radius_was_removed(pos, None, 15, 15, true);
        assert_eq!(entries(w).len(), 1);
        w.ownership_sign_was_placed_or_changed_at_pos_with_land_owner_width_radius_height_radius_was_removed(
            IntPair { x: 41, y: 7 },
            None,
            15,
            15,
            true,
        );
        assert!(w.ownership_sign_positions.borrow().as_ref().unwrap().map.get(&"1".to_string()).is_none());
        assert!(entries(w).is_empty());
        let sends = f.rec.sends.borrow();
        let last = &sends[sends.len() - 1].0;
        let d = crate::plist::helpers::property_list_from_data(&gzip_inflate(&last[1..]).unwrap()).unwrap();
        assert!(matches!(d.object_for_key("removed"), Some(PlistValue::Bool(true))));
        // removal in a never-created bucket on a nil dictionary is a no-op
        *w.ownership_sign_positions.borrow_mut() = None;
        w.update_local_ownership_signs_due_to_sign_placed_or_changed_at_pos_with_land_owner_width_radius_height_radius_was_removed(pos, None, 15, 15, true);
        assert!(w.ownership_sign_positions.borrow().is_none());
    }

    #[test]
    fn remote_relays_queue_while_loading_and_forward_afterwards() {
        let f = fixture(32);
        let w = &f.world;
        w.remote_create_for_objects_of_type_client_id(vec![vec![1]], 7, "a");
        w.remote_create_for_objects_of_type_client_id(vec![vec![2]], 7, "a");
        w.remote_create_for_objects_of_type_client_id(vec![vec![3]], 8, "b");
        {
            let q = w.initial_create_objects.borrow();
            let create = q.create.as_ref().unwrap();
            assert_eq!(create.get(&"a".to_string()).unwrap().get(&int_key(7)).unwrap().len(), 2);
            assert_eq!(create.len(), 2);
        }
        w.remote_update_for_objects_of_type_from_client(vec![vec![1]], 3, Some("a"));
        w.remote_update_for_objects_of_type_from_client(vec![vec![2]], 3, Some("b"));
        assert_eq!(w.initial_create_objects.borrow().update.as_ref().unwrap().get(&int_key(3)).unwrap().len(), 2);
        assert!(f.rec.calls.borrow().is_empty());
        with_dw(w);
        w.remote_update_for_objects_of_type_from_client(vec![vec![9]], 3, Some("a"));
        assert!(f.rec.calls.borrow().contains(&"dw_remote_update 1 3 Some(\"a\")".to_string()));
    }

    #[test]
    fn small_forwards_and_stubs() {
        let f = fixture(32);
        let w = &f.world;
        with_dw(w);
        w.save_sunlight_changed_at_pos(IntPair { x: -33, y: 65 });
        assert!(
            f.rec.calls.borrow().contains(&"light_changed -1 2 false".to_string()),
            "C division truncates toward zero"
        );
        assert!(w.allows_panning() && w.allows_rotation() && !w.requires_swipe_events() && !w.map_visible());
        assert_eq!(w.active_blockhead_pos(), w.start_portal_pos());
        w.set_repair_mode(true);
        assert_eq!(w.current_tip_text().as_deref(), Some("-- REPAIR MODE --"));
        assert_eq!(w.current_total_blockhead_count_including_net(), 0);
        bh(&f, 1, Some("p"), IntPair { x: 1, y: 1 });
        assert_eq!(w.current_total_blockhead_count_including_net(), 1);
        w.new_server_password_set(Some("x"));
        assert_eq!(w.server_password.borrow().as_deref(), Some("x"));
        w.new_server_password_set(None);
        assert!(w.server_password.borrow().is_none());
        let mut d = new_mutable_dict();
        d.map.insert("newWelcomeMessage".into(), PlistValue::string("hey"));
        w.welcome_message_dict_recieved_from_client(Some(&PlistValue::Dict(d)), "x");
        assert_eq!(w.welcome_message.borrow().as_deref(), Some("hey"));
        w.welcome_message_dict_recieved_from_client(Some(&PlistValue::Dict(new_mutable_dict())), "x");
        assert!(w.welcome_message.borrow().is_none());
        w.set_new_welcome_message(Some("direct"));
        assert_eq!(w.welcome_message.borrow().as_deref(), Some("direct"));
        w.welcome_message_dict_recieved_from_client(None, "x");
        assert!(w.welcome_message.borrow().is_none());
        let _ = catch_ns(|| ());
    }

    #[test]
    fn report_user_formats() {
        let f = fixture(32);
        let w = &f.world;
        let log = capture_log(|| {
            w.report_user_with_name_reporter_name_reporter_message_reported_avatar_image_path(
                Some("bad"),
                None,
                Some("line1\nline2"),
                None,
            )
        });
        assert!(
            log.contains(
                "REPORT-USER:bad\nREPORTER-NAME:HOST\nREPORTER-EMAIL:no_email\nREPORTER-MESSAGE:line1\nline2\n"
            ),
            "{:?}",
            log
        );
        // (parallel tests may log into the shared sink, so test the local shape only)
        assert!(!log.contains("line2\n\n"), "the C format already ends in a newline: no extra one");
        f.rec.cloud.set(true);
        let log = capture_log(|| {
            w.report_user_with_name_reporter_name_reporter_message_reported_avatar_image_path(
                Some("bad"),
                Some("rep"),
                Some("a\nb"),
                Some("/p.png"),
            )
        });
        assert!(strip_log(&log).contains(&"CLOUD:REPORT-USER:<<bad>>REPORTER-NAME:<<rep>>REPORTER-EMAIL:<<no_email>>REPORTER-MESSAGE:<<a\\nb>>REPORTED-AVATAR:<</p.png>>".to_string()), "{}", log);
        assert!(w.is_cloud_game());
    }

    #[test]
    fn mute_list_round_trips_through_the_file() {
        let _g = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = format!("{}/bhwc_mute_{}", std::env::temp_dir().display(), std::process::id());
        let _ = std::fs::remove_dir_all(&home);
        crate::foundation::paths::set_home_for_test(&home);
        std::fs::create_dir_all(format!("{}/GNUstep/Library/ApplicationSupport/TheBlockheads/saves/sv", home)).unwrap();
        let f = fixture(32);
        let w = &f.world;
        assert!(!w.player_is_muted("p1"));
        w.set_player_muted_with_id(true, "p1");
        w.set_player_muted_with_id(true, "p1");
        w.set_player_muted_with_id(true, "p2");
        assert!(w.player_is_muted("p1") && w.player_is_muted("p2"));
        w.muted_players.borrow_mut().clear();
        assert!(w.player_is_muted("p2"), "reloaded from default_mutelist");
        assert_eq!(w.muted_players.borrow().len(), 2);
        w.set_player_muted_with_id(false, "p1");
        assert!(!w.player_is_muted("p1"));
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;
    use crate::world::lifecycle::testkit::*;

    /// Own home directory: the mute list is written below it.
    fn sandbox(tag: &str) -> std::sync::MutexGuard<'static, ()> {
        let g = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = format!("{}/bhcn_{}_{}", std::env::temp_dir().display(), tag, std::process::id());
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(format!("{home}/GNUstep/Library/ApplicationSupport/TheBlockheads/saves/sv")).unwrap();
        crate::foundation::paths::set_home_for_test(&home);
        g
    }

    #[test]
    fn stubs_and_forwards() {
        let _g = sandbox("stubs");
        let f = fixture(32);
        let w = &f.world;
        assert!(!w.requires_directional_swipes());
        w.directional_swipe(Vector2 { x: 0.0, y: 0.0 });
        assert_eq!(w.current_tip_text().as_deref(), Some("a tip"));
        assert_eq!(w.current_tip_color(), [1.0; 4]);
        w.show_double_time_prompt_if_good_time();
        w.zoom_to_pos_pinch_zoom(IntPair { x: 0, y: 0 }, false);
        w.water_moved_from_from_tile_to_to_tile_amount(IntPair { x: 0, y: 0 }, None, IntPair { x: 1, y: 0 }, None, 1);
        assert!(w.world_ui().is_none());
        w.report_achievement_with_identifier("x");
        w.server_fill_reply(&[]);
        // without a dynamic world
        assert_eq!(w.current_total_blockhead_count_including_net(), 0);
        assert!(!w.is_controlling_blockheads_for_client_player("boss"));
        w.players_changed();
        assert_eq!(f.rec.count("players_changed"), 0);
        w.set_player_muted_with_id(true, "x");
        assert_eq!(f.rec.count("mute_changed"), 0);
        // with one
        with_dw(w);
        assert!(w.is_controlling_blockheads_for_client_player("boss"));
        assert!(!w.is_controlling_blockheads_for_client_player("nobody"));
        w.players_changed();
        assert_eq!(f.rec.count("players_changed"), 1);
        w.set_player_muted_with_id(true, "y");
        assert_eq!(f.rec.count("mute_changed y"), 1);
        w.remote_create_for_objects_of_type_client_id(vec![vec![1]], 3, "c");
        w.remote_creation_data_update_for_objects_of_type_from_client(vec![vec![1]], 3, Some("c"));
        assert_eq!(f.rec.count("dw_remote_create"), 1);
        assert_eq!(f.rec.count("dw_remote_cdu"), 1);
        // the loader
        let pb: PhysicalBlockRef = Box::leak(Box::new(crate::world::state::PhysicalBlock::new_fresh()));
        w.update_physical_block_to_latest_version(pb);
        assert_eq!(f.rec.count("update_pb"), 0);
        *w.world_tile_loader.borrow_mut() = Some(Rc::new(crate::world::state::WorldTileLoader {}));
        w.update_physical_block_to_latest_version(pb);
        assert_eq!(f.rec.count("update_pb"), 1);
        // repair mode shows its own text
        w.repair_mode.set(true);
        assert_eq!(w.current_tip_text().as_deref(), Some("-- REPAIR MODE --"));
    }

    #[test]
    fn aborting_the_path_of_a_blockhead() {
        let f = fixture(32);
        let w = &f.world;
        let bh = crate::world::helpers::testutil::new_obj();
        // no path creator yet
        w.abort_in_progress_path_if_for_blockhead(&bh);
        *w.path_creator.borrow_mut() = Some(Rc::new(crate::world::state::PathCreator {}));
        w.abort_in_progress_path_if_for_blockhead(&bh);
        f.rec.pc_in_progress.set(true);
        w.abort_in_progress_path_if_for_blockhead(&bh);
        *f.rec.pc_user.borrow_mut() = Some(crate::world::helpers::testutil::new_obj());
        w.abort_in_progress_path_if_for_blockhead(&bh);
        assert_eq!(f.rec.count("abort_path"), 0);
        *f.rec.pc_user.borrow_mut() = Some(bh.clone());
        w.abort_in_progress_path_if_for_blockhead(&bh);
        assert_eq!(f.rec.count("abort_path"), 1);
    }

    #[test]
    fn a_world_without_a_light_block_set_hands_out_index_zero_and_zoom_needs_a_dynamic_world() {
        let f = fixture(32);
        let w = &f.world;
        *w.free_client_light_block_indices.borrow_mut() = None;
        with_dw(w);
        assert!(w.client_connected("solo"));
        assert_eq!(f.rec.sc_created.borrow()[0].1, 0);
        *w.dynamic_world.borrow_mut() = None;
        let p = w.get_blockhead_zoom_pos_for_client_requester_client_cycle_index(Some("solo"), None, 0);
        assert_eq!(p, IntPair { x: -1, y: -1 });
    }

    #[test]
    fn light_lookup_wraps_around_the_world_and_tolerates_odd_blocks() {
        let f = fixture(32);
        let w = &f.world;
        let pb = w.take_physical_block();
        pb.valid.set(true);
        *w.macro_tiles.get(0).unwrap().physical_block.borrow_mut() = Some(pb);
        let t = TileRef::new(pb, 0);
        assert!(w.client_connected("c"));
        let idx = f.rec.sc_created.borrow()[0].1 as usize;
        let mut lb = Box::new([0u8; 1024]);
        lb[37] = 1;
        pb.client_light_blocks.borrow_mut()[idx] = Some(lb);
        // the block thinks it sits at the far end of the world: x wraps forward
        pb.x_pos.set(31);
        pb.y_pos.set(0);
        assert!(w.tile_is_lit_for_client_at_pos_tile(Some("c"), IntPair { x: 5, y: 0 }, Some(t.clone())));
        assert!(!w.tile_is_lit_for_client_at_pos_tile(Some("c"), IntPair { x: 6, y: 0 }, Some(t.clone())));
        // and the other way round (a block at the start, a tile far to the east): out of range, so lit
        *w.macro_tiles.get(18).unwrap().physical_block.borrow_mut() = Some(pb);
        pb.x_pos.set(0);
        assert!(w.tile_is_lit_for_client_at_pos_tile(Some("c"), IntPair { x: 600, y: 0 }, Some(t.clone())));
        // an offset beyond the block is lit as well
        assert!(w.tile_is_lit_for_client_at_pos_tile(Some("c"), IntPair { x: 5, y: 40 }, Some(t.clone())));
        // an invalid block, a negative light block index
        pb.valid.set(false);
        assert!(w.tile_is_lit_for_client_at_pos_tile(Some("c"), IntPair { x: 5, y: 0 }, Some(t.clone())));
        pb.valid.set(true);
        f.rec.clients.borrow_mut().get_mut("c").unwrap().lbi = -1;
        assert!(w.tile_is_lit_for_client_at_pos_tile(Some("c"), IntPair { x: 5, y: 0 }, Some(t)));
    }

    #[test]
    fn circumnavigating_the_whole_world_reports_the_achievement() {
        let f = fixture(1);
        let w = &f.world;
        *w.circum_navigate_booleans.borrow_mut() = Some(crate::foundation::indexset::IndexSet::new());
        for x in (0..32).step_by(2) {
            w.mark_circum_navigate_x(x);
        }
        w.mark_circum_navigate_x(31);
        w.mark_circum_navigate_x(1);
        assert!(w.circum_navigate_booleans.borrow().as_ref().unwrap().count() >= 24);
    }

    #[test]
    fn messages_reports_mutes_and_signs_in_corners() {
        let _g = sandbox("corners");
        let f = fixture(32);
        let w = &f.world;
        // a world without a server keeps its welcome message and ignores reports and broadcasts
        let local = World::alloc(32);
        local.set_new_welcome_message(Some("hi"));
        assert!(local.welcome_message.borrow().is_none());
        local.report_user_with_name_reporter_name_reporter_message_reported_avatar_image_path(None, None, None, None);
        local.ownership_sign_was_placed_or_changed_at_pos_with_land_owner_width_radius_height_radius_was_removed(
            IntPair { x: 3, y: 3 },
            None,
            15,
            9,
            false,
        );
        assert!(local.ownership_sign_positions_need_saving());
        // cloud reports with an empty avatar path, a path, and none
        f.rec.cloud.set(true);
        let log = capture_log(|| {
            for avatar in [Some(""), Some("p.png"), None] {
                w.report_user_with_name_reporter_name_reporter_message_reported_avatar_image_path(
                    Some("bad"),
                    None,
                    Some("a\nb"),
                    avatar,
                );
            }
        });
        assert!(log.contains("REPORTED-AVATAR:<<p.png>>"));
        // signs with a non-default height, local and broadcast
        let pos = IntPair { x: 40, y: 7 };
        w.update_local_ownership_signs_due_to_sign_placed_or_changed_at_pos_with_land_owner_width_radius_height_radius_was_removed(pos, None, 15, 9, false);
        w.ownership_sign_was_placed_or_changed_at_pos_with_land_owner_width_radius_height_radius_was_removed(
            pos, None, 15, 7, false,
        );
        // a bucket that holds something other than a list is left alone
        w.ownership_sign_positions.borrow_mut().as_mut().unwrap().map.insert("1".to_string(), PlistValue::int(1));
        w.update_local_ownership_signs_due_to_sign_placed_or_changed_at_pos_with_land_owner_width_radius_height_radius_was_removed(pos, None, 15, 15, false);
        // no macro tile at all
        assert!(w.tile_is_lit_for_client_at_pos_tile(Some("nobody"), IntPair { x: 5, y: 5000 }, None));
        // a mute list file that is not a list
        let _ = w.player_is_muted("zed");
        let dir = save_dir_for("sv");
        std::fs::write(format!("{dir}/default_mutelist"), b"<plist version=\"1.0\"><dict/></plist>").unwrap();
        w.muted_players.borrow_mut().clear();
        assert!(!w.player_is_muted("zed"));
    }
}
