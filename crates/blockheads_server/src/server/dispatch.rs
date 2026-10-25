//! The packet dispatcher of `BHServer` (`match:didReceiveData:fromPlayer:`): entry gate, handshake, chat receive,
//! per-type routing, and the portal-chest acknowledgement.
//!
//! `match:didReceiveData:fromPlayer:`; `sendPortalChestAcknowledgementIfNeededForClient:`.

use std::rc::Rc;

use crate::foundation::clock::reference_time;
use crate::foundation::exception::ns_raise;
use crate::foundation::gsdict::GsDict;
use crate::foundation::gzip::{gzip_deflate, gzip_inflate};
use crate::foundation::indexset::IndexSet;
use crate::foundation::md5::string_from_md5;
use crate::foundation::nsstring::{self, safe_case_insensitive_compare as safe_ci};
use crate::net::bhmatch::PlayerInfo;
use crate::plist::helpers::{data_from_property_list, property_list_from_data};
use crate::plist::keyed_archiver::unarchive_index_set;
use crate::plist::value::{PlistDict, PlistValue};
use crate::protocol::codec::{self, reject, Malformed};
use crate::protocol::messages::{self, decode_client_packet, ClientMsg};
use crate::server::bhserver::{
    dict_get, dict_str, new_mutable_dict, raise_unrecognized, write_data_atomic, BHServer, NetBlob, ServerWorld,
};

/// `-match:didReceiveData:fromPlayer:`.
pub fn match_did_receive_data_from_player(srv: &Rc<BHServer>, data: &[u8], peer: &str) {
    srv.update_players();
    // Mods (not in the original): a vanilla peer in the readable-rejection limbo is dropped by any packet.
    if crate::mods::hooks().is_some_and(|h| h.limbo_packet(peer)) {
        return;
    }
    let ty = data.first().copied().unwrap_or(0);
    let connected = srv.connected_clients.borrow().iter().any(|e| e == peer);
    // Mods (not in the original): 0xBC goes to the mod host, only when one is installed.
    if ty == crate::mods::MOD_PACKET {
        if let Some(h) = crate::mods::hooks() {
            if connected || srv.unapproved_clients.borrow().iter().any(|e| e == peer) {
                h.mod_packet(peer, data);
            }
            return;
        }
    }
    if !connected {
        if ty != 0x1f {
            return;
        }
        if !srv.unapproved_clients.borrow().iter().any(|e| e == peer) {
            return;
        }
    }
    // Unknown types decode to nothing, and the decoder's only errors are the original's exceptions.
    let msg = match decode_client_packet(data) {
        Ok(Some(m)) => m,
        Err(Malformed::Exception { name, reason }) => ns_raise(name, &reason),
        _ => return,
    };
    route(srv, msg, data, peer);
}

/// `[NSArray of NSData]` -> blobs; `None` for nil or a non-array (the handlers treat nil as nothing to do).
fn blobs_of(items: &Option<PlistValue>) -> Option<Vec<NetBlob>> {
    match items {
        Some(PlistValue::Array(a)) => Some(
            a.items
                .iter()
                .filter_map(|v| match v {
                    PlistValue::Data(d) => Some(d.bytes.clone()),
                    _ => None,
                })
                .collect(),
        ),
        _ => None,
    }
}

fn route(srv: &Rc<BHServer>, msg: ClientMsg, original_packet: &[u8], peer: &str) {
    let w = srv.world();
    let dw = w.as_ref().and_then(|w| w.dynamic_world());
    let to_peer = |pkt: Vec<u8>| srv.send_network_data_to_peers_reliable(pkt, &[peer.to_string()], true);
    let forward = |orig: &[u8]| {
        let mut others = srv.connected_clients.borrow().clone();
        others.retain(|e| e != peer);
        if !others.is_empty() {
            srv.send_network_data_to_peers_reliable(orig, others, true);
        }
    };
    let alias = srv.player_alias_for_peer_id(Some(peer));
    match msg {
        ClientMsg::PlayerInformation { payload } => handshake(srv, &payload, peer),
        ClientMsg::BlockRequest(req) => {
            if let Some(w) = &w {
                w.request_for_block_from_client(req, peer);
            }
        }
        ClientMsg::InitialDataRequest => {
            let history = srv.chat_history.borrow().clone();
            if !history.is_empty() {
                let mut photos: GsDict<String, PlistValue> = GsDict::new();
                for chat in &history {
                    if let Some(pid) = dict_str(chat, "playerID") {
                        if photos.get(&pid).is_none() {
                            let ph =
                                srv.player_info_for_peer_id(&pid).and_then(|i| match dict_get(&i.borrow(), "photo") {
                                    Some(PlistValue::Data(d)) => Some(PlistValue::data(d.bytes.clone())),
                                    _ => None,
                                });
                            if let Some(ph) = ph {
                                photos.insert(pid, ph);
                            }
                        }
                    }
                }
                let msgs: Vec<PlistValue> = history.into_iter().map(PlistValue::Dict).collect();
                to_peer(messages::chat_history(&PlistValue::dict(photos), &msgs));
            }
            let mut food = [0i32; 31];
            if let Some(w) = &w {
                for (i, v) in w.distance_ordered_food_types().into_iter().take(31).enumerate() {
                    food[i] = v;
                }
            }
            let blockheads = dw.as_ref().and_then(|d| d.load_client_blockheads_data_for_player_id(peer));
            to_peer(messages::initial_data_reply(&food, blockheads.as_deref()));
        }
        ClientMsg::ObjectCreate { ty, items } => {
            if let (Some(w), Some(b)) = (&w, blobs_of(&items)) {
                w.remote_create_for_objects_of_type_client_id(b, ty as i32, peer);
            }
        }
        ClientMsg::ObjectUpdate { ty, items } => {
            if let (Some(w), Some(b)) = (&w, blobs_of(&items)) {
                w.remote_update_for_objects_of_type_from_client(b, ty as i32, peer);
            }
        }
        ClientMsg::ObjectRemove { ty, items } => {
            if let (Some(w), Some(b)) = (&w, blobs_of(&items)) {
                w.remote_remove_for_objects_of_type_from_client(b, ty as i32, peer);
            }
        }
        ClientMsg::ObjectCreationDataUpdate { ty, items } => {
            if let (Some(w), Some(b)) = (&w, blobs_of(&items)) {
                w.remote_creation_data_update_for_objects_of_type_from_client(b, ty as i32, peer);
            }
        }
        ClientMsg::BlockRemovalNotification { macro_index } => {
            if let Some(w) = &w {
                w.remote_block_removed_by_client(macro_index as i32, peer);
            }
        }
        ClientMsg::UniqueIdRequest => {
            let id = dw.as_ref().map_or(0, |d| d.get_next_dynamic_object_id());
            to_peer(messages::unique_id_reply(id));
        }
        ClientMsg::FillRequest { payload } => {
            if let Some(w) = &w {
                let idx = w.remote_fill_request_placed_by_client(&payload, peer);
                if let Some(sc) = w.server_client(peer) {
                    sc.add_fill_request(idx);
                }
            }
        }
        ClientMsg::RemoveRequest { payload } => {
            if let Some(w) = &w {
                let idx = w.remote_remove_request_from_client(&payload, peer);
                if let Some(sc) = w.server_client(peer) {
                    sc.add_fill_request(idx);
                }
            }
        }
        ClientMsg::RemoveBackWallRequest { payload } => {
            if let Some(w) = &w {
                let idx = w.remote_remove_back_wall_request_from_client(&payload, peer);
                if let Some(sc) = w.server_client(peer) {
                    sc.add_fill_request(idx);
                }
            }
        }
        ClientMsg::PickupRequest { requester, ids } => {
            if let Some(d) = &dw {
                d.client_pickup_request_count_client_id_blockhead_requester_unique_id(&ids, peer, requester);
            }
        }
        ClientMsg::FreeblockCreation { payload } => {
            if let Some(d) = &dw {
                d.create_client_freeblocks_with_data(&payload);
            }
            send_portal_chest_acknowledgement_if_needed_for_client(srv, peer);
        }
        ClientMsg::PlaceWorkbench { payload } => {
            if let Some(w) = &w {
                w.remote_place_workbench_request_from_peer(&payload, peer);
            }
        }
        ClientMsg::PlaceInteractionObject { payload } => {
            if let Some(w) = &w {
                w.remote_place_interaction_object_request_from_peer(&payload, peer);
            }
        }
        ClientMsg::Paint { payload } => {
            if let Some(w) = &w {
                w.remote_paint_request_from_client(&payload, peer);
            }
        }
        ClientMsg::Heartbeat { payload } => {
            if let Some(w) = &w {
                w.heartbeat_data_recieved_from_peer(&payload, peer);
            }
        }
        ClientMsg::HeartbeatRequestOld => {
            let enabled = w.as_ref().is_some_and(|w| w.custom_rules_enabled());
            if enabled {
                crate::nslog!("Kicking player {} due to likely cheating.", peer);
                srv.boot_player_was_ban(peer, false);
                return;
            }
            if let Some(sc) = w.as_ref().and_then(|w| w.server_client(peer)) {
                sc.set_requests_heart_beat(true);
            }
        }
        ClientMsg::HeartbeatRequestWithRules { payload } => {
            let now = reference_time();
            if now - srv.rules_changed_time.get() > 30.0
                && !srv.player_is_admin_with_alias(alias.as_deref())
                && !w.as_ref().is_some_and(|w| w.verify_client_custom_rules_data(&payload))
            {
                crate::nslog!("Kicking player {} due to likely cheating.", peer);
                srv.boot_player_was_ban(peer, false);
                return;
            }
            if let Some(sc) = w.as_ref().and_then(|w| w.server_client(peer)) {
                sc.set_requests_heart_beat(true);
            }
        }
        ClientMsg::Gather { payload, original } => {
            if let Some(w) = &w {
                w.remote_gather_request(&payload);
            }
            forward(&original);
        }
        ClientMsg::Projectile { payload, original } => {
            if let Some(w) = &w {
                w.remote_projectile_request(&payload);
            }
            forward(&original);
        }
        ClientMsg::Tip { payload, original } => {
            if let Some(w) = &w {
                w.remote_tip_notification(&payload);
            }
            forward(&original);
        }
        ClientMsg::BlockheadDamage { payload } => {
            if let Some(w) = &w {
                // Mods: the damage event names the attacking player (no effect without mods).
                let _actor = crate::mods::hooks().map(|_| crate::mods::act_as(peer));
                w.remote_blockhead_damage_request_requested_by_client_name(&payload, alias.as_deref());
            }
        }
        ClientMsg::ElectricityPath { payload } => {
            if let Some(w) = &w {
                w.electricity_path_data_recieved_from_client(&payload, peer);
            }
        }
        ClientMsg::ZoomRequest { player_id, cycle_index } => {
            let pos = w.as_ref().map(|w| {
                w.get_blockhead_zoom_pos_for_client_requester_client_cycle_index(
                    player_id.as_deref(),
                    Some(peer),
                    cycle_index,
                )
            });
            let reply = match pos {
                Some(t) if t.y > 0 => Some((t.x, t.y)),
                _ => None,
            };
            to_peer(messages::zoom_response(reply));
        }
        ClientMsg::ClientBlockheads { payload } => {
            if let Some(d) = &dw {
                d.client_blockheads_recieved_for_player_id_data(peer, &payload);
            }
        }
        ClientMsg::ClientBlockheadInventory { blockhead_id, data } => {
            if let Some(d) = &dw {
                d.client_blockhead_inventory_recieved_for_player_id_blockhead_id_data(peer, blockhead_id, &data);
            }
            send_portal_chest_acknowledgement_if_needed_for_client(srv, peer);
        }
        ClientMsg::TradeTransaction { dict } => {
            if let Some(w) = &w {
                w.transaction_data_recieved_from_client(dict.as_ref(), peer);
            }
        }
        ClientMsg::PaintingDataRequest { id } => {
            if let Some(d) = &dw {
                d.send_painting_data_for_painting_with_id_to_client(id, peer);
            }
        }
        ClientMsg::WelcomeMessageSet { dict } => {
            if srv.player_is_admin_with_alias(alias.as_deref()) {
                if let Some(w) = &w {
                    w.welcome_message_dict_recieved_from_client(dict.as_ref(), peer);
                }
            }
        }
        ClientMsg::WelcomeMessageGet => {
            let welcome = w.as_ref().and_then(|w| w.welcome_message()).map(PlistValue::string);
            let rules = match &w {
                Some(w) if w.custom_rules_enabled() => w.custom_rules_dict(),
                _ => None,
            };
            to_peer(messages::welcome_message(welcome.as_ref(), rules.as_ref()));
        }
        ClientMsg::ReportUser { dict } => report_user(srv, dict, peer, alias),
        ClientMsg::PasswordChange { new_password } => {
            if srv.player_is_owner_with_alias(alias.as_deref()) {
                if let Some(w) = &w {
                    w.new_server_password_set(new_password.as_deref());
                }
                if srv.is_cloud_match() {
                    if new_password.is_some() {
                        crate::nslog!("CLOUD:SETTING:PASSWORD:ON");
                    } else {
                        crate::nslog!("CLOUD:SETTING:PASSWORD:OFF");
                    }
                }
            }
        }
        ClientMsg::PrivacyChange { privacy } => privacy_change(srv, privacy, peer, alias.as_deref()),
        ClientMsg::OwnershipSignPlayerListRequest => {
            let list = srv.get_recent_player_names_for_ownership_sign();
            to_peer(messages::ownership_sign_player_list(&list));
        }
        ClientMsg::PortalChestTransaction { id, .. } => {
            srv.trade_portal_transactions
                .borrow_mut()
                .get_or_insert_with(Default::default)
                .insert(peer.to_string(), id as u32);
        }
        ClientMsg::PortalChestTransactionResend { raw8, id } => {
            let saved = srv
                .server_database
                .as_ref()
                .and_then(|db| db.data_for_key(Some(&format!("{}_lastPortalChestAck", peer))))
                .and_then(|d| property_list_from_data(&d));
            let matches = saved
                .as_ref()
                .and_then(|s| s.object_for_key("transaction"))
                .is_some_and(|v| v.unsigned_int_value() == id as u32);
            if matches {
                to_peer(messages::portal_chest_ack_resend(&raw8));
            } else {
                to_peer(messages::portal_chest_not_saved(&raw8));
            }
        }
        ClientMsg::UpdateCustomRule { dict, original } => {
            if srv.player_is_owner_with_alias(alias.as_deref()) || srv.player_is_admin_with_alias(alias.as_deref()) {
                srv.rules_changed_time.set(reference_time());
                if let Some(d) = &dict {
                    if let Some(w) = &w {
                        w.custom_rule_dict_recieved_from_net(d);
                    }
                    if let PlistValue::Dict(pd) = d {
                        for key in pd.map.keys_in_order() {
                            crate::nslog!(
                                "Custom rule named \"{}\" changed by {}",
                                key,
                                alias.as_deref().unwrap_or("(null)")
                            );
                        }
                    }
                }
                forward(&original);
            }
        }
        ClientMsg::FoundItemsOld { payload } => {
            if let Some(set) = unarchive_index_set(&payload, "foundItems") {
                if let Some(d) = &dw {
                    d.new_found_list_recieved_from_client_list(peer, &set);
                }
            }
        }
        ClientMsg::FoundItemsV2 { ranges } => {
            let mut set = IndexSet::new();
            for (loc, len) in ranges {
                set.add_indexes_in_range(loc as u64, len as u64);
            }
            if let Some(d) = &dw {
                d.new_found_list_recieved_from_client_list(peer, &set);
            }
        }
        ClientMsg::DynamicObjectRemovalRequest { blockhead_id, object_id } => {
            if let Some(d) = &dw {
                d.client_blockhead_with_id_from_client_requests_dyamic_object_removal_with_id(
                    blockhead_id,
                    peer,
                    object_id,
                );
            }
        }
        ClientMsg::RepairTile { world_index } => {
            let in_set = srv.repair_set.borrow().contains(peer);
            if in_set
                && (srv.player_is_owner_with_alias(alias.as_deref())
                    || srv.player_is_admin_with_alias(alias.as_deref()))
            {
                if let Some(w) = &w {
                    let pos = w.world_pos_at_world_index(world_index);
                    srv.do_repair_for_tile_at_pos(pos);
                }
            }
        }
        ClientMsg::Chat { dict } => chat(srv, dict, peer),
    }
    let _ = original_packet;
}

/// 0x37 CLIENT_REPORT_USER.
fn report_user(srv: &Rc<BHServer>, dict: Option<PlistValue>, peer: &str, reporter: Option<String>) {
    // `report[@"playerName"]` on a non-dictionary plist raises (`objectForKey:` unrecognized).
    if let Some(other) = dict.as_ref().filter(|d| !matches!(d, PlistValue::Dict(_))) {
        raise_unrecognized(other, "objectForKey:");
    }
    let name =
        dict.as_ref().and_then(|d| d.object_for_key("playerName")).and_then(|v| v.as_str()).map(|s| s.to_string());
    let (Some(name), Some(reporter)) = (name, reporter) else { return };
    let mut avatar_file: Option<String> = None;
    let recent = srv.recent_players.borrow().clone();
    for pd in &recent {
        if safe_ci(Some(&name), dict_str(pd, "alias").as_deref()) == 0 {
            let Some(rid) = dict_str(pd, "playerID") else { continue };
            let photo = srv.player_info_for_peer_id(&rid).and_then(|i| match dict_get(&i.borrow(), "photo") {
                Some(PlistValue::Data(d)) => Some(d.bytes.clone()),
                _ => None,
            });
            if let Some(photo) = photo {
                let dir = format!(
                    "{}/saves/{}/reportedAvatars/",
                    crate::foundation::paths::appsup(),
                    srv.save_id.as_deref().unwrap_or("(null)")
                );
                let _ = std::fs::create_dir_all(&dir);
                let fname = format!("{}.png", rid);
                if write_data_atomic(&format!("{}/{}", dir, fname), &photo) {
                    avatar_file = Some(fname);
                    break;
                }
            }
        }
    }
    let message =
        dict.as_ref().and_then(|d| d.object_for_key("reporterMessage")).and_then(|v| v.as_str()).map(|s| s.to_string());
    if let Some(w) = srv.world() {
        w.report_user_with_name_reporter_name_reporter_message_reported_avatar_image_path(
            Some(&name),
            Some(&reporter),
            message.as_deref(),
            avatar_file.as_deref(),
        );
    }
    let _ = peer;
}

/// 0x39 CLIENT_PRIVACY_CHANGE_REQUEST.
fn privacy_change(srv: &Rc<BHServer>, privacy: Option<String>, peer: &str, alias: Option<&str>) {
    if !srv.player_is_owner_with_alias(alias) {
        return;
    }
    let Some(privacy) = privacy else { return };
    if nsstring::length(&privacy) == 0 {
        return;
    }
    let Some(m) = srv.match_() else { return };
    let Some(sm) = m.as_server_match() else { return };
    let white_nonempty = srv.white_list.borrow().as_ref().is_some_and(|l| !l.is_empty());
    if srv.is_cloud_match() && white_nonempty && safe_ci(Some(&privacy), Some("public")) == 0 {
        let cur = srv.privacy_string().map(|s| nsstring::uppercase_string(&s)).unwrap_or("(null)".to_string());
        let msg = format!(
            "Error: This world is whitelisted, and whitelisted worlds can only be serachable or private. Privacy setting has been left unchanged as {}. To make this world public, please clear the whitelist first with /CLEAR-WHITELIST",
            cur
        );
        srv.send_chat_message_send_to_clients(msg.as_str(), Some(&[peer.to_string()]));
        return;
    }
    if safe_ci(Some(&privacy), Some("public")) != 0
        && safe_ci(Some(&privacy), Some("searchable")) != 0
        && safe_ci(Some(&privacy), Some("private")) != 0
    {
        return;
    }
    sm.set_privacy_by_string(Some(&privacy));
    if srv.is_cloud_match() {
        crate::nslog!("CLOUD:SETTING:PRIVACY:{}", nsstring::uppercase_string(&privacy));
    }
    if safe_ci(Some(&privacy), Some("public")) == 0 {
        if let Some(w) = srv.world() {
            if w.server_password().is_some() {
                w.new_server_password_set(None);
                crate::nslog!("CLOUD:SETTING:PASSWORD:OFF");
            }
        }
    }
}

/// 0x24 CHAT_MESSAGE from a connected client.
fn chat(srv: &Rc<BHServer>, dict: Option<PlistValue>, peer: &str) {
    // `msg[@"playerID"] = peer` on a non-dictionary plist (array, string,...) raises before anything else.
    // Note: the selector in the exception reason (`setObject:forKey:` vs `setObject:forKeyedSubscript:`)
    // is not given by;
    let mut msg: Option<PlistDict> = match dict {
        None => None,
        Some(PlistValue::Dict(d)) => Some(d),
        Some(other) => raise_unrecognized(&other, "setObject:forKey:"),
    };
    if let Some(d) = msg.as_mut() {
        d.map.insert("playerID".to_string(), PlistValue::string(peer));
    }
    let alias = srv.player_alias_for_peer_id(Some(peer));
    let a = alias.as_deref();
    if let Some(d) = msg.as_mut() {
        if srv.player_is_owner_with_alias(a) {
            d.map.insert("mod".to_string(), PlistValue::string("owner"));
        }
        if srv.player_is_admin_with_alias(a) {
            d.map.insert("mod".to_string(), PlistValue::string("admin"));
        } else if srv.player_is_mod_with_alias(a) {
            d.map.insert("mod".to_string(), PlistValue::string("mod"));
        } else {
            d.map.remove(&"mod".to_string());
        }
        if srv.player_is_cloud_wide_admin_with_alias(a) {
            let v = if srv.player_is_cloud_wide_invisible_admin_with_alias(a) { "mode2" } else { "mode1" };
            d.map.insert("supermod".to_string(), PlistValue::string(v));
        } else {
            d.map.remove(&"supermod".to_string());
        }
    }
    let raw_text = msg.as_ref().and_then(|d| dict_get(d, "message")).cloned();
    let mut text: Option<String> = match &raw_text {
        Some(PlistValue::String(s)) => Some(s.s.clone()),
        Some(other) => raise_unrecognized(other, "rangeOfString:"),
        None => None,
    };
    let loc_zero = match &text {
        Some(t) => nsstring::range_of_string(t, "/").is_some_and(|(l, _)| l == 0),
        None => true,
    };
    let is_command = (loc_zero || srv.is_waiting_for_multi_part_command_response()) && srv.player_is_mod_with_alias(a);
    if !is_command && srv.is_cloud_match() {
        let privacy = srv.match_().and_then(|m| m.as_server_match().map(|s| s.privacy())).unwrap_or(0);
        if privacy == 1 || privacy == 2 {
            text = srv.replace_curse_words_for_message_client(text.as_deref(), Some(peer));
            if let Some(d) = msg.as_mut() {
                match &text {
                    Some(t) => {
                        d.map.insert("message".to_string(), PlistValue::string(t.clone()));
                    }
                    None => {
                        d.map.remove(&"message".to_string());
                    }
                }
            }
        }
    }
    let Some(mut msg) = msg else {
        let nil = messages::chat_nil_exception();
        let Malformed::Exception { name, reason } = nil else { unreachable!() };
        ns_raise(name, &reason)
    };
    // Mods (not in the original; `None` without mods): mod commands work for every player, chat can be cancelled
    // or rewritten before it is shown or broadcast.
    if let (Some(h), Some(t)) = (crate::mods::hooks(), text.clone()) {
        if loc_zero && h.owns_command(&t) {
            if let Some(d) = srv.delegate() {
                d.chat_message_recieved_display_notification(msg.clone(), true);
            }
            h.command(Some(peer), &t);
            return;
        }
        if !is_command {
            match h.chat(peer, &t) {
                crate::mods::ChatVerdict::Keep => {}
                crate::mods::ChatVerdict::Cancel => return,
                crate::mods::ChatVerdict::Replace(n) => {
                    msg.map.insert("message".to_string(), PlistValue::string(n.clone()));
                    text = Some(n);
                }
            }
        }
    }
    if let Some(d) = srv.delegate() {
        d.chat_message_recieved_display_notification(msg.clone(), true);
    }
    if is_command {
        srv.handle_command_issue_client(text.as_deref(), Some(peer));
        return;
    }
    let mut others = srv.connected_clients.borrow().clone();
    others.retain(|e| e != peer);
    if !others.is_empty() {
        srv.send_network_data_to_peers_reliable(messages::chat_message(&PlistValue::Dict(msg.clone())), others, true);
    }
    let mut h = srv.chat_history.borrow_mut();
    h.push(msg);
    if h.len() > 20 {
        h.remove(0);
    }
}

/// UTF-16 length of `raw["alias"]` as `[raw[@"alias"] length]` (NSData: byte count; other classes raise).
fn alias_length(raw: &PlistValue) -> usize {
    match raw.object_for_key("alias") {
        None => 0,
        Some(PlistValue::String(s)) => nsstring::length(&s.s),
        Some(PlistValue::Data(d)) => d.bytes.len(),
        Some(other) => raise_unrecognized(other, "length"),
    }
}

/// Steps 6 to 10 of the handshake: the reason this player may not join (cloud key, client version, password,
/// blacklist, whitelist), none when it may.
fn admission_reject(
    srv: &Rc<BHServer>,
    m: &dyn crate::net::bhmatch::BhMatch,
    world: &dyn ServerWorld,
    info: &PlayerInfo,
) -> Option<u8> {
    let info_alias = dict_str(&info.borrow(), "alias");
    // 6.
    if let Some(salt) = m.cloud_salt() {
        let server_key = string_from_md5(&format!("{}{}", salt, info_alias.clone().unwrap_or_default()));
        let cloud_key = dict_str(&info.borrow(), "cloudKey");
        if cloud_key.is_none() || safe_ci(cloud_key.as_deref(), Some(&server_key)) != 0 {
            return Some(reject::INCORRECT_CLOUD_KEY);
        }
    }
    // 7.
    if world.custom_rules_enabled() || world.world_width_macro() < 512 {
        let minor = dict_get(&info.borrow(), "minorVersion").map(|v| v.int_value());
        if minor.is_none_or(|v| v < 2) {
            return Some(reject::CLIENT_TOO_OLD);
        }
    }
    // 8.
    if let Some(pw) = world.server_password() {
        if !srv.player_is_owner_with_alias(info_alias.as_deref())
            && !srv.player_is_cloud_wide_admin_with_alias(info_alias.as_deref())
        {
            let cp = dict_str(&info.borrow(), "clientPassword");
            if cp.as_deref() != Some(pw.as_str()) {
                return Some(reject::INCORRECT_PASSWORD);
            }
        }
    }
    let info_snapshot = info.borrow().clone();
    // 9.
    if srv.player_is_black_listed_with_info(&info_snapshot) {
        return Some(reject::BLACKLISTED);
    }
    // 10.
    if !srv.player_is_white_listed_with_info(&info_snapshot) {
        return Some(reject::NOT_WHITELISTED);
    }
    None
}

/// Handshake (type 0x1f).
fn handshake(srv: &Rc<BHServer>, payload: &[u8], peer: &str) {
    let send_to =
        |target: &str, pkt: Vec<u8>| srv.send_network_data_to_peers_reliable(pkt, &[target.to_string()], true);
    let remove_unapproved = || srv.unapproved_clients.borrow_mut().retain(|e| e != peer);
    // reject(r, target)
    let reject_to = |r: u8, target: &str| {
        send_to(target, messages::rejection(r));
        remove_unapproved();
    };
    // rejectP(r): the rejection goes to the persistent id `p`, which is then removed.
    let reject_p = |r: u8, p: &str| {
        reject_to(r, p);
        if let Some(m) = srv.match_() {
            m.remove_client(p);
        }
        srv.client_finished_away_simulation(p);
    };

    // 1.
    let world = match srv.world() {
        Some(w) if w.load_complete() => w,
        _ => {
            reject_to(reject::STILL_LOADING, peer);
            return;
        }
    };
    // 2.
    if srv.is_cloud_match() {
        let credit = srv.credit();
        if (credit as f64) <= 0.001 || credit.is_nan() {
            let wid = srv.save_id.clone().or(world.save_id()).unwrap_or_default();
            send_to(peer, messages::rejection_no_credit(&wid, &world.world_name().unwrap_or_default()));
            remove_unapproved();
            return;
        }
    }
    // 3.
    if srv.full() {
        reject_to(reject::SERVER_FULL, peer);
        return;
    }
    // 4.
    let raw = property_list_from_data(payload);
    let raw_dict: Option<PlistDict> = match &raw {
        None => None,
        Some(PlistValue::Dict(d)) => Some(d.clone()),
        Some(other) => raise_unrecognized(other, "objectForKey:"),
    };
    if let Some(r) = &raw {
        if alias_length(r) > 16 {
            reject_to(reject::INVALID_USERNAME, peer);
            if let Some(m) = srv.match_() {
                m.remove_client(peer);
            }
            return;
        }
    }
    // 5.
    let Some(m) = srv.match_() else { return };
    // With the "same name from another device" setting another connection that holds the name is replaced by
    // this one, but only once this join would be admitted (a join that fails must not log anyone out).
    if crate::server::shared_names::enabled() {
        if let Some(prov) = m.name_holder_elsewhere(raw_dict.as_ref(), peer) {
            if let Some(r) = admission_reject(srv, &*m, &*world, &prov) {
                reject_to(r, peer);
                return;
            }
        }
    }
    let info: Option<PlayerInfo> = m.client_player_information_recieved_from_peer(raw_dict.as_ref(), peer);
    let p_id: Option<String> = m.persistent_id_for_net_id(peer);
    let already = p_id.as_ref().is_some_and(|p| srv.connected_clients.borrow().iter().any(|e| e == p));
    let (Some(info), Some(p_id), false) = (info.clone(), p_id.clone(), already) else {
        reject_to(reject::USER_LOGGED_IN, peer);
        m.remove_client(peer);
        return;
    };
    let p = p_id.as_str();
    let info_alias = dict_str(&info.borrow(), "alias");
    // 6-10.
    if let Some(r) = admission_reject(srv, &*m, &*world, &info) {
        reject_p(r, p);
        return;
    }
    let info_snapshot = info.borrow().clone();
    // 11.
    if m.requires_player_authentication() {
        if !srv.is_cloud_match() {
            // Every reset entry naming this alias is spent by the join.
            let mut is_reset = false;
            if let Some(rl) = srv.reset_list.borrow_mut().as_mut() {
                let before = rl.len();
                rl.retain(|name| safe_ci(Some(name), info_alias.as_deref()) != 0);
                is_reset = rl.len() != before;
            }
            if is_reset {
                srv.save_reset_list();
            } else if let Some(old) = srv
                .saved_player_info_data_for_player(p)
                .and_then(|d| property_list_from_data(&d))
                .filter(|_| !crate::server::shared_names::enabled())
            {
                let old_d = match &old {
                    PlistValue::Dict(d) => d.clone(),
                    other => raise_unrecognized(other, "objectForKey:"),
                };
                let new_udid = dict_str(&info_snapshot, "udidNew");
                let mut auth = new_udid.is_some() && new_udid == dict_str(&old_d, "udidNew");
                if !auth && dict_get(&old_d, "iCloudID").is_some() {
                    let a = dict_str(&info_snapshot, "iCloudID");
                    auth = a.is_some() && a == dict_str(&old_d, "iCloudID");
                }
                if !auth && dict_get(&old_d, "gameCenterID").is_some() {
                    let a = dict_str(&info_snapshot, "gameCenterID");
                    auth = a.is_some() && a == dict_str(&old_d, "gameCenterID");
                }
                if !auth {
                    reject_p(reject::USERNAME_TAKEN, p);
                    return;
                }
            }
        }
        let new_udid = dict_str(&info_snapshot, "udidNew");
        for d in srv.node.live_player_infos() {
            let db = d.borrow();
            let du = dict_str(&db, "udidNew");
            let same_udid = du.is_some() && du == new_udid;
            if same_udid && dict_str(&db, "playerID").as_deref() != Some(p) {
                drop(db);
                reject_p(reject::DEVICE_ALREADY_IN_USE_WITH_DIFFERENT_NAME, p);
                return;
            }
        }
    }
    // Mods (not in the original; `None` without mods): the `bhcModLoader` check and `player-pre-login` may reject
    // with a vanilla reason.
    if let Some(h) = crate::mods::hooks() {
        let loader = crate::mods::LoaderKey::from_player_info(raw_dict.as_ref());
        // A vanilla client on a server with client-required mods is not approved: it gets a world summary whose
        // welcome message explains the situation, and the host drops it a little later.
        if h.limbo_join(peer, p, loader.as_ref()) {
            send_to(p, limbo_summary(&*world, crate::mods::LIMBO_TEXT));
            remove_unapproved();
            return;
        }
        if let Some(r) = h.pre_login(peer, p, info_alias.as_deref().unwrap_or(""), loader.as_ref()) {
            reject_p(r, p);
            return;
        }
    }
    // 12. approved
    srv.connected_clients.borrow_mut().push(p.to_string());
    remove_unapproved();
    m.client_approved_with_info(&info);
    srv.node.load_info_for_player(p);
    if let Some(d) = srv.player_info_dict_for_peer_id(p) {
        if !d.map.is_empty() {
            {
                let mut rp = srv.recent_players.borrow_mut();
                if let Some(i) = rp.iter().position(|x| dict_str(x, "playerID").as_deref() == Some(p)) {
                    rp.remove(i);
                }
            }
            srv.add_player_dict_to_all_players_ever(&d);
            srv.save_all_players_array();
        }
    }
    // The world of step 1 (it cannot go away within one packet).
    if !world.client_connected(p) {
        send_to(p, messages::rejection(reject::SERVER_FULL));
        remove_unapproved();
        srv.connected_clients.borrow_mut().retain(|e| e != p);
        m.remove_client(peer);
        srv.client_finished_away_simulation(peer);
    } else {
        let alias = srv.player_alias_for_peer_id(Some(p));
        let a = alias.as_deref();
        let cloud_mode = srv.cloud_mode_for_alias(a);
        let admin = srv.player_is_admin_with_alias(a);
        let moderator = srv.player_is_mod_with_alias(a);
        let owner = srv.player_is_owner_with_alias(a);
        let summary = world.summary_net_data_for_admin_mod_owner_cloud_mode(admin, moderator, owner, cloud_mode);
        srv.send_network_data_to_peers_reliable(summary, &[p.to_string()], true);
    }
    srv.players_changed();
    srv.send_updated_player_list_to_clients();
    if let Some(h) = crate::mods::hooks() {
        if srv.connected_clients.borrow().iter().any(|e| e == p) {
            h.player_join(p);
        }
    }
}

/// The 0x01 summary of the readable-rejection limbo: the world's own summary for a plain player (so the client
/// loads without trouble) with `welcomeMessage` replaced by `text`; a minimal one when that cannot be patched.
fn limbo_summary(world: &dyn ServerWorld, text: &str) -> Vec<u8> {
    let real = world.summary_net_data_for_admin_mod_owner_cloud_mode(false, false, false, 0);
    let patched =
        real.get(1..).and_then(gzip_inflate).and_then(|x| property_list_from_data(&x)).and_then(|v| match v {
            PlistValue::Dict(mut d) => {
                d.map.insert("welcomeMessage".to_string(), PlistValue::string(text.to_string()));
                data_from_property_list(&PlistValue::Dict(d))
            }
            _ => None,
        });
    let Some(xml) = patched else {
        let minimal = messages::WorldSummary {
            save_id: world.save_id().unwrap_or_default(),
            world_name: world.world_name().unwrap_or_default(),
            world_width_macro: world.world_width_macro(),
            welcome_message: Some(text.to_string()),
            ..Default::default()
        };
        return messages::world_summary(&minimal);
    };
    let mut out = vec![codec::t::WORLD_SUMMARY];
    out.extend_from_slice(&gzip_deflate(&xml));
    out
}

/// `-sendPortalChestAcknowledgementIfNeededForClient:`.
pub fn send_portal_chest_acknowledgement_if_needed_for_client(srv: &BHServer, client_id: &str) {
    let tx = srv.trade_portal_transactions.borrow().as_ref().and_then(|t| t.get(client_id).copied());
    let Some(tx) = tx else { return };
    srv.send_network_data_to_peers_reliable(messages::portal_chest_ack(tx as u16), &[client_id.to_string()], true);
    let mut d = new_mutable_dict();
    d.map.insert("transaction".to_string(), PlistValue::int(tx as i64));
    let blob = data_from_property_list(&PlistValue::Dict(d));
    if let Some(db) = &srv.server_database {
        db.set_data(blob.as_deref(), Some(&format!("{}_lastPortalChestAck", client_id)));
    }
    if let Some(t) = srv.trade_portal_transactions.borrow_mut().as_mut() {
        t.remove(client_id);
    }
    let _ = codec::t::CHAT_MESSAGE;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::md5::string_from_md5;
    use crate::net::bhmatch::MatchDelegate;
    use crate::server::bhserver::test_support::*;

    fn rejection_of(h: &Harness, id: &str) -> Option<u8> {
        h.m.packets_to(id).iter().find(|p| p[0] == 0x26).map(|p| p[1])
    }

    fn try_join(h: &Harness, alias: &str, udid: &str, minor: i64) -> (String, Option<u8>) {
        let pid = string_from_md5(&nsstring::lowercase_string(alias));
        h.m.persistent.borrow_mut().insert(pid.clone(), pid.clone());
        h.connect(&pid);
        h.m.clear();
        h.send(&pid, &Harness::handshake_packet(alias, udid, minor));
        let r = rejection_of(h, &pid);
        (pid, r)
    }

    #[test]
    fn contact_packet_and_gate() {
        let h = Harness::new("d_contact");
        h.connect("1:0");
        let pk = h.m.packets_to("1:0");
        assert_eq!(&pk[0][..2], &[0x23, 0x26]);
        h.m.clear();
        // non-handshake packet from an unapproved peer is ignored
        h.send("1:0", &[0x0e]);
        assert!(h.m.sent.borrow().is_empty());
        // handshake from unknown peer ignored
        h.send("9:9", &Harness::handshake_packet("A", "u", 5));
        assert!(h.m.sent.borrow().is_empty());
    }

    #[test]
    fn join_flow_sends_summary_and_connects() {
        let h = Harness::new("d_join");
        let pid = h.join("Alice");
        assert!(h.srv.connected_client_ids().contains(&pid));
        assert!(h.w.calls.borrow().contains(&format!("client_connected {}", pid)));
        let pk = h.m.packets_to(&pid);
        assert!(pk.iter().any(|p| p[0] == 0x01), "summary expected");
        assert_eq!(h.srv.unapproved_clients.borrow().len(), 0);
        assert!(h.d.changed.get() > 0);
        // duplicate handshake from an approved client -> 2
        h.m.clear();
        h.send(&pid, &[0x1f]);
        assert_eq!(rejection_of(&h, &pid), Some(2));
    }

    #[test]
    fn rejection_matrix() {
        let h = Harness::new("d_reject");
        // 14: 17-char alias
        let (_p, r) = try_join(&h, "ABCDEFGHIJKLMNOPQ", "u1", 5);
        assert_eq!(r, Some(14));
        // 2: alias SERVER
        let (_p, r) = try_join(&h, "SERVER", "u2", 5);
        assert_eq!(r, Some(2));
        // 3: world narrower than 512 and minorVersion 1
        h.w.width.set(128);
        let (_p, r) = try_join(&h, "Carl", "u3", 1);
        assert_eq!(r, Some(3));
        h.w.width.set(512);
        // accepted with minorVersion 1 on a 512-wide world
        let (pid, r) = try_join(&h, "Carl2", "u3b", 1);
        assert_eq!(r, None);
        assert!(h.srv.connected_client_ids().contains(&pid));
        // 10: password
        *h.w.password.borrow_mut() = Some("abc".into());
        let (_p, r) = try_join(&h, "Dan", "u4", 5);
        assert_eq!(r, Some(10));
        *h.w.password.borrow_mut() = None;
        // 15: still loading
        h.w.loaded.set(false);
        let (_p, r) = try_join(&h, "Eve", "u5", 5);
        assert_eq!(r, Some(15));
        h.w.loaded.set(true);
        // 8: another alias on a connected player's udid
        let (_p, r) = try_join(&h, "Frank", "u3b", 5);
        assert_eq!(r, Some(8));
    }

    #[test]
    fn full_server_gives_5() {
        let h = Harness::new("d_full");
        h.srv.max_players.set(1);
        h.join("Alice");
        let (_p, r) = try_join(&h, "Bob", "ub", 5);
        assert_eq!(r, Some(5));
    }

    #[test]
    fn blacklist_whitelist_and_returning_player() {
        use crate::storage::lists::{BLACK_HEADER, WHITE_HEADER};
        let h = Harness::with_files("d_lists", |s| {
            std::fs::write(format!("{}/blacklist.txt", s), format!("{}\nbob", BLACK_HEADER)).unwrap();
            std::fs::write(format!("{}/whitelist.txt", s), WHITE_HEADER).unwrap();
        });
        let (_p, r) = try_join(&h, "Bob", "ub", 5);
        assert_eq!(r, Some(6));
        // returning player with another device: 1
        let pid = h.join("Zed");
        h.send(&pid, &[0x0e]); // keep alive
        h.srv.match_player_did_change_state(&*h.m, &pid, crate::net::bhmatch::PlayerConnectionState::Disconnected);
        let (_p, r) = try_join(&h, "Zed", "other-device", 5);
        assert_eq!(r, Some(1));
        // same device is accepted
        let (_p, r) = try_join(&h, "Zed", "udid-Zed", 5);
        assert_eq!(r, None);
    }

    #[test]
    fn same_name_from_another_device_follows_the_setting() {
        use crate::server::shared_names::set_enabled;
        use crate::storage::lists::BLACK_HEADER;
        let h = Harness::with_files("d_shared", |s| {
            std::fs::write(format!("{}/blacklist.txt", s), format!("{}\nbob", BLACK_HEADER)).unwrap();
        });
        // a returning player on another device: refused as in vanilla, welcome with the setting
        let pid = h.join("Zed");
        h.srv.match_player_did_change_state(&*h.m, &pid, crate::net::bhmatch::PlayerConnectionState::Disconnected);
        assert_eq!(try_join(&h, "Zed", "other-device", 5).1, Some(1));
        set_enabled(true);
        h.m.name_held.set(true);
        h.m.removed.borrow_mut().clear();
        // the name is held by another connection: a join that fails changes nothing for the holder
        let (_p, r) = try_join(&h, "Bob", "ub", 5);
        assert_eq!(r, Some(6));
        assert!(h.m.removed.borrow().is_empty(), "the holder is not touched");
        // an admitted join goes on to the match, which replaces the holder
        let (pid, r) = try_join(&h, "Zed", "other-device", 5);
        assert_eq!(r, None);
        assert!(h.srv.connected_client_ids().contains(&pid));
        // no holder: the same
        h.m.name_held.set(false);
        assert_eq!(try_join(&h, "Zed", "third-device", 5).1, Some(2));
        set_enabled(false);
    }

    #[test]
    fn whitelist_non_listed_gives_7() {
        use crate::storage::lists::WHITE_HEADER;
        let h = Harness::with_files("d_white", |s| {
            std::fs::write(format!("{}/whitelist.txt", s), format!("{}\nzed", WHITE_HEADER)).unwrap()
        });
        let (_p, r) = try_join(&h, "Nobody", "un", 5);
        assert_eq!(r, Some(7));
        let (_p, r) = try_join(&h, "Zed", "uz", 5);
        assert_eq!(r, None);
    }

    #[test]
    fn reset_owner_flow() {
        let h = Harness::new("d_reset");
        let pid = h.join("Alice");
        h.srv.match_player_did_change_state(&*h.m, &pid, crate::net::bhmatch::PlayerConnectionState::Disconnected);
        *h.srv.reset_list.borrow_mut() = Some(vec!["alice".into()]);
        let (_p, r) = try_join(&h, "Alice", "brand-new-device", 5);
        assert_eq!(r, None);
        assert_eq!(h.srv.reset_list.borrow().clone(), Some(vec![]));
    }

    #[test]
    fn cloud_key_and_no_credit() {
        let h = Harness::new("d_cloud");
        *h.m.salt.borrow_mut() = Some("SALT".into());
        let (_p, r) = try_join(&h, "Olga", "uo", 5);
        assert_eq!(r, Some(9));
    }

    #[test]
    fn chat_relay_and_history() {
        let h = Harness::new("d_chat");
        let a = h.join("Alice");
        let b = h.join("Bob");
        h.m.clear();
        let mut d = new_mutable_dict();
        d.insert("message".to_string(), PlistValue::string("/help"));
        d.insert("alias".to_string(), PlistValue::string("Spoof"));
        let body = data_from_property_list(&PlistValue::Dict(d)).unwrap();
        let mut pkt = vec![0x24];
        pkt.extend(body);
        h.send(&a, &pkt);
        // non-mod: relayed, sender gets nothing
        assert!(h.m.packets_to(&a).is_empty());
        let to_b = h.m.packets_to(&b);
        assert_eq!(to_b.len(), 1);
        let p = property_list_from_data(&to_b[0][1..]).unwrap();
        assert_eq!(p.object_for_key("playerID").and_then(|v| v.as_str()), Some(a.as_str()));
        assert_eq!(p.object_for_key("alias").and_then(|v| v.as_str()), Some("Spoof"));
        assert!(p.object_for_key("mod").is_none());
        assert_eq!(h.srv.chat_history.borrow().len(), 1);
        assert_eq!(h.d.messages(), vec!["/help".to_string()]);
        // 0x05 returns history (0x25) before 0x06
        h.m.clear();
        h.send(&b, &[0x05]);
        let kinds: Vec<u8> = h.m.packets_to(&b).iter().map(|p| p[0]).collect();
        assert_eq!(kinds, vec![0x25, 0x06]);
        let reply = &h.m.packets_to(&b)[1];
        assert_eq!(reply.len(), 1 + 124);
    }

    #[test]
    fn mod_command_is_not_relayed() {
        let h = Harness::new("d_cmd");
        let a = h.join("Alice");
        let b = h.join("Bob");
        h.srv.modify_list_for_player_or_ip_is_added_list_type("alice", true, 3);
        h.m.clear();
        let mut d = new_mutable_dict();
        d.insert("message".to_string(), PlistValue::string("/players"));
        let mut pkt = vec![0x24];
        pkt.extend(data_from_property_list(&PlistValue::Dict(d)).unwrap());
        h.send(&a, &pkt);
        assert!(h.m.packets_to(&b).is_empty());
        let to_a = h.m.packets_to(&a);
        assert_eq!(to_a.len(), 1);
        let p = property_list_from_data(&to_a[0][1..]).unwrap();
        let text = p.object_for_key("message").and_then(|v| v.as_str()).unwrap().to_string();
        assert!(text.starts_with("There are 2 players connected:"), "{}", text);
        assert_eq!(p.object_for_key("alias").and_then(|v| v.as_str()), Some("SERVER"));
        assert_eq!(p.object_for_key("mod").and_then(|v| v.as_str()), Some("owner"));
        assert_eq!(h.srv.chat_history.borrow().len(), 0);
    }

    #[test]
    fn simple_requests() {
        let h = Harness::new("d_misc");
        let a = h.join("Alice");
        h.m.clear();
        h.send(&a, &[0x0e]);
        let pk = h.m.packets_to(&a);
        let mut exp = vec![0x0f];
        exp.extend(0x94u64.to_le_bytes());
        assert_eq!(pk[0], exp);
        h.m.clear();
        // 0x1b for unknown player -> 1c
        let mut d = new_mutable_dict();
        d.insert("playerID".to_string(), PlistValue::string("nobody"));
        d.insert("cycleIndex".to_string(), PlistValue::int(0));
        let mut pkt = vec![0x1b];
        pkt.extend(data_from_property_list(&PlistValue::Dict(d)).unwrap());
        h.send(&a, &pkt);
        assert_eq!(h.m.packets_to(&a)[0], vec![0x1c]);
        // 0x40 resend for never-acked id -> 41 + same 8 bytes
        h.m.clear();
        let raw = [5, 0, 0, 0, 0, 0, 0, 0];
        let mut pkt = vec![0x40];
        pkt.extend(raw);
        h.send(&a, &pkt);
        let mut exp = vec![0x41];
        exp.extend(raw);
        assert_eq!(h.m.packets_to(&a)[0], exp);
        // 0x3e then a freeblock creation (0x14) acks and persists
        h.m.clear();
        let mut pkt = vec![0x3e];
        pkt.extend(raw);
        h.send(&a, &pkt);
        assert!(h.m.sent.borrow().is_empty());
        h.send(&a, &[0x14, 1, 2, 3]);
        let pk = h.m.packets_to(&a);
        assert_eq!(pk[0][0], 0x3f);
        assert_eq!(&pk[0][1..3], &[5, 0]);
        assert!(h.srv.server_database().unwrap().data_for_key(Some(&format!("{}_lastPortalChestAck", a))).is_some());
        h.m.clear();
        let mut pkt = vec![0x40];
        pkt.extend(raw);
        h.send(&a, &pkt);
        assert_eq!(h.m.packets_to(&a)[0][0], 0x3f);
        // fill request reaches the server client
        let c = crate::server::server_client::ServerClient::init_with_client_id_server_light_block_index_light_block_database(&a, &h.srv, 31, None);
        h.w.clients.borrow_mut().insert(a.clone(), c.clone());
        h.send(&a, &[0x10, 0, 0, 0, 0]);
        let r =
            c.get_and_remove_all_reciept_data_for_macro_pos_world(crate::types::structs::IntPair { x: 0, y: 0 }, &*h.w);
        let Some(PlistValue::Array(arr)) = r else { unreachable!() };
        assert_eq!(arr.items.len(), 1);
    }

    #[test]
    fn anti_cheat_timer_kicks_after_countdown() {
        let h = Harness::new("d_cheat");
        let a = h.join("Alice");
        h.srv.update_player(&a);
        h.srv.player_update_timer.set(0.0);
        h.m.clear();
        h.send(&a, &[0x0e]);
        assert!(h.srv.player_update().is_none());
        assert_eq!(h.m.disconnected.borrow().as_slice(), std::slice::from_ref(&a));
        let t = h.srv.player_update_timer.get();
        assert!((10.0..=60.0).contains(&t));
    }

    #[test]
    fn heartbeat_request_old_kicks_with_rules() {
        let h = Harness::new("d_hb");
        let a = h.join("Alice");
        h.w.rules.set(true);
        h.m.clear();
        h.send(&a, &[0x18]);
        assert!(h.m.packets_to(&a).iter().any(|p| p == &[0x26, 0x0b]));
    }

    #[test]
    fn disconnect_updates_recent_players() {
        let h = Harness::new("d_disc");
        let a = h.join("Alice");
        let _b = h.join("Bob");
        h.srv.match_player_did_change_state(&*h.m, &a, crate::net::bhmatch::PlayerConnectionState::Disconnected);
        assert!(!h.srv.connected_client_ids().contains(&a));
        // Alice re-inserted at index 0
        assert_eq!(dict_str(&h.srv.recent_players()[0], "alias").as_deref(), Some("Alice"));
        assert!(h.w.calls.borrow().contains(&format!("client_disconnected {} false", a)));
    }

    #[test]
    fn limbo_summary_patches_the_real_summary_or_falls_back() {
        let h = Harness::new("d_limbo_summary");
        let welcome = |p: &[u8]| {
            let xml = gzip_inflate(&p[1..]).unwrap();
            let PlistValue::Dict(d) = property_list_from_data(&xml).unwrap() else { panic!() };
            (p[0], d)
        };
        // The world's own summary keeps its keys; only the welcome message changes.
        let mut real = new_mutable_dict();
        real.map.insert("saveID".to_string(), PlistValue::string("sid"));
        real.map.insert("welcomeMessage".to_string(), PlistValue::string("old"));
        let mut packet = vec![0x01];
        packet.extend(gzip_deflate(&data_from_property_list(&PlistValue::Dict(real)).unwrap()));
        *h.w.summary.borrow_mut() = Some(packet);
        let (ty, d) = welcome(&limbo_summary(&*h.world_dyn, "hi"));
        assert_eq!(ty, 0x01);
        assert_eq!(dict_str(&d, "saveID").as_deref(), Some("sid"));
        assert_eq!(dict_str(&d, "welcomeMessage").as_deref(), Some("hi"));
        // Not a dictionary, not gzip, or empty: a minimal summary of the world.
        let mut array = vec![0x01];
        array.extend(gzip_deflate(&data_from_property_list(&PlistValue::string("x")).unwrap()));
        for bad in [array, vec![0x01, 0, 0, 0, 0], vec![]] {
            *h.w.summary.borrow_mut() = Some(bad);
            let (ty, d) = welcome(&limbo_summary(&*h.world_dyn, "hi"));
            assert_eq!(ty, 0x01);
            assert_eq!(dict_str(&d, "saveID").as_deref(), Some(h.w.id.as_str()));
            assert_eq!(dict_str(&d, "worldName").as_deref(), Some("TestWorld"));
            assert_eq!(dict_get(&d, "worldWidthMacro").map(|v| v.int_value()), Some(512));
            assert_eq!(dict_str(&d, "welcomeMessage").as_deref(), Some("hi"));
        }
    }
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod routing_tests;
