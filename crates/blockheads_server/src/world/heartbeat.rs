//! The 0x01 world summary packet and the 0x17 heartbeat packet.
//!
//! `sendHeartbeatData`, `heartbeatDataRecieved:fromPeer:` (+ `## Synthesis resolutions` 3, 4, 5, 11, 13);

use crate::foundation::gzip::gzip_deflate;
use crate::plist::helpers::data_from_property_list;
use crate::plist::value::PlistValue;
use crate::types::netdata::WorldHeartbeat;
use crate::types::structs::WireStruct;
use crate::world::lifecycle::{core_hooks, new_mutable_dict};
use crate::world::state::World;

/// `BHNDT_WORLD_SUMMARY`
const BHNDT_WORLD_SUMMARY: u8 = 0x01;
/// `BHNDT_SERVER_WORLD_HEARTBEAT`
const BHNDT_SERVER_WORLD_HEARTBEAT: u8 = 0x17;

impl World {
    /// `-[World summaryNetDataForAdmin:mod:owner:cloudMode:]`: `0x01` + `gzipDeflate(XML plist)`.
    /// The conditions `owner` / `admin` / `mod` / `supermod` read the four METHOD PARAMETERS, not the `World` ivars
    /// of the same name (R16). The base dictionary is built with `dictionaryWithObjectsAndKeys:`, so a nil `saveID` /
    /// `worldName` truncates it at that point (never happens on the server). Built per call.
    pub fn summary_net_data_for_admin_mod_owner_cloud_mode(
        &self,
        is_admin: bool,
        is_mod: bool,
        is_owner: bool,
        cloud_mode: i32,
    ) -> Vec<u8> {
        let hooks = core_hooks();
        let mut d = new_mutable_dict();
        let has_server = self.server.borrow().is_some();
        let credit: f32 = if has_server { hooks.server_credit() } else { 0.0 }; // NIL-FLOAT when server == nil
        d.map.insert("randomSeed".to_string(), PlistValue::int(self.random_seed() as i64));
        d.map.insert("worldTime".to_string(), PlistValue::real_f64(self.world_time()));
        d.map.insert("noRainTimer".to_string(), PlistValue::real_f32(self.no_rain_timer()));
        d.map.insert("credit".to_string(), PlistValue::real_f32(credit));
        let sp = self.start_portal_pos();
        d.map.insert("startPortalPos.x".to_string(), PlistValue::int(sp.x as i64));
        d.map.insert("startPortalPos.y".to_string(), PlistValue::int(sp.y as i64));
        let hp = self.highest_point();
        d.map.insert("highestPoint.x".to_string(), PlistValue::int(hp.x as i64));
        d.map.insert("highestPoint.y".to_string(), PlistValue::int(hp.y as i64));
        d.map.insert("portalLevel".to_string(), PlistValue::int(self.portal_level() as i64));
        // dictionaryWithObjectsAndKeys: stops at the first nil value
        if let Some(id) = self.save_id.borrow().clone() {
            d.map.insert("saveID".to_string(), PlistValue::string(id));
            d.map.insert("worldWidthMacro".to_string(), PlistValue::int(self.world_width_macro as i64));
            d.map.insert("expertMode".to_string(), PlistValue::boolean(self.expert_mode()));
            if let Some(n) = self.world_name.borrow().clone() {
                d.map.insert("worldName".to_string(), PlistValue::string(n));
                d.map.insert("minorVersion".to_string(), PlistValue::int(3));
            }
        }
        if is_owner {
            d.map.insert("owner".to_string(), PlistValue::boolean(true));
            if has_server {
                // `[[server match] isKindOfClass:[BHNetServerMatch class]]` is always true on the dedicated server
                // `numberWithInt:` truncates the 8-byte BHNetPrivacy to a C int
                d.map.insert("privacy".to_string(), PlistValue::int(hooks.server_match_privacy() as i32 as i64));
                if let Some(p) = self.server_password.borrow().clone() {
                    d.map.insert("serverPassword".to_string(), PlistValue::string(p));
                }
            }
        }
        if is_admin || is_owner {
            d.map.insert("admin".to_string(), PlistValue::boolean(true));
        }
        if is_mod || is_admin || is_owner {
            d.map.insert("mod".to_string(), PlistValue::boolean(true));
        }
        if cloud_mode != 0 {
            d.map.insert("supermod".to_string(), PlistValue::int(cloud_mode as i64));
        }
        if !self.pvp_enabled() {
            d.map.insert("disablePvp".to_string(), PlistValue::boolean(true));
        }
        if let Some(p) = self.world_price_multipliers.borrow().as_ref() {
            d.map.insert("worldPriceMultipliers".to_string(), PlistValue::Dict(p.clone()));
        }
        if let Some(m) = self.welcome_message.borrow().clone() {
            d.map.insert("welcomeMessage".to_string(), PlistValue::string(m));
        }
        if let Some(s) = self.ownership_sign_positions.borrow().as_ref() {
            if s.map.len() != 0 {
                d.map.insert("ownershipSignPositions".to_string(), PlistValue::Dict(s.clone()));
            }
        }
        if let Some(c) = self.custom_rules_dict.borrow().as_ref() {
            // the summary uses the key `customRules`, NOT `customRulesDict`
            d.map.insert("customRules".to_string(), PlistValue::Dict(c.clone()));
        }
        let mut out = vec![BHNDT_WORLD_SUMMARY];
        if let Some(xml) = data_from_property_list(&PlistValue::Dict(d)) {
            out.extend_from_slice(&gzip_deflate(&xml));
        }
        out
    }

    /// `-[World heartbeatData]`: `0x17` + 16 bytes of `WorldHeartbeat` (17 bytes). `local_paused` and `all_paused` are
    /// always 0 on the server because `uiManager` is nil.
    pub fn heartbeat_data(&self) -> Vec<u8> {
        let has_server = self.server.borrow().is_some();
        let credit: f32 = if has_server { core_hooks().server_credit() } else { 0.0 }; // NIL-FLOAT
                                                                                       // `[uiManager gameBlockingUIDisplayed]`: uiManager is nil, so NO
        let local_paused = false;
        // the server asks every client only when the local player is paused, which is never the case here
        let all_paused = local_paused;
        let hb = WorldHeartbeat {
            world_time: self.world_time() as f32,
            no_rain_timer: self.no_rain_timer(),
            fast_forward: self.fast_forward() as u8,
            local_paused: local_paused as u8,
            all_paused: all_paused as u8,
            pvp_disabled: (!self.pvp_enabled()) as u8,
            credit,
        };
        let mut out = vec![BHNDT_SERVER_WORLD_HEARTBEAT];
        hb.append_to(&mut out);
        out
    }

    /// `-[World sendHeartbeatData]`: with a server and at least one `serverClients` entry the packet goes to ALL
    /// `connectedClients` reliably (R3); with none nothing is sent.
    pub fn send_heartbeat_data(&self) {
        let data = self.heartbeat_data();
        let n = self.server_clients.borrow().as_ref().map_or(0, |m| m.borrow().len());
        if self.server.borrow().is_some() && n != 0 {
            core_hooks().server_send_network_data_to_peers_reliable(&data, None, true);
            return;
        }
        // client part: nil message; `foundItemsListNeedsToBeSentToServer` is never set on the server
    }

    /// `-[World heartbeatDataRecieved:fromPeer:]`, server branch (the client branch is dead). Only
    /// `min(len, 16)` bytes are copied into a zeroed local (R4, UB-PARITY: missing bytes are 0). A client whose
    /// `localPaused` differs flips its pause flag and the server re-broadcasts.
    pub fn heartbeat_data_recieved_from_peer(&self, hb: &[u8], peer_id: &str) {
        let mut raw = [0u8; 16]; // UB-PARITY: uninitialised stack in the original
        let n = hb.len().min(16);
        raw[..n].copy_from_slice(&hb[..n]);
        let mut wh = WorldHeartbeat::read_le(&raw);
        if hb.len() < 16 {
            wh.pvp_disabled = 0;
        }
        let hooks = core_hooks();
        let local_paused = wh.local_paused != 0;
        let known = self.has_server_client(peer_id);
        let paused = known && hooks.server_client_paused(peer_id);
        if paused != local_paused && known && hooks.server_client_connected(peer_id) {
            hooks.server_client_set_paused(peer_id, local_paused);
            self.send_heartbeat_data();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::gzip::gzip_inflate;
    use crate::plist::helpers::property_list_from_data;
    use crate::types::structs::IntPair;
    use crate::world::lifecycle::testkit::*;

    fn decode(pkt: &[u8]) -> PlistValue {
        assert_eq!(pkt[0], 0x01);
        assert_eq!(&pkt[1..11], &[0x1f, 0x8b, 0x08, 0, 0, 0, 0, 0, 0, 0x03], "gzip framing");
        property_list_from_data(&gzip_inflate(&pkt[1..]).unwrap()).unwrap()
    }
    fn keys(v: &PlistValue) -> Vec<String> {
        let PlistValue::Dict(d) = v else { panic!() };
        let mut k = d.map.keys_in_order();
        k.sort();
        k
    }
    const BASE: [&str; 14] = [
        "credit",
        "expertMode",
        "highestPoint.x",
        "highestPoint.y",
        "minorVersion",
        "noRainTimer",
        "portalLevel",
        "randomSeed",
        "saveID",
        "startPortalPos.x",
        "startPortalPos.y",
        "worldName",
        "worldTime",
        "worldWidthMacro",
    ];

    fn base_world() -> Fixture {
        let f = fixture(32);
        let w = &f.world;
        w.set_pvp_enabled(true);
        w.set_random_seed(0x9000_0000);
        w.set_world_time(2704.5);
        w.set_no_rain_timer(4.5);
        w.set_start_portal_pos(IntPair { x: 3, y: 4 });
        w.set_highest_point(IntPair { x: 5, y: 6 });
        f
    }

    #[test]
    fn plain_player_gets_only_the_base_keys_with_the_right_types() {
        let f = base_world();
        f.rec.credit.set(1.5);
        let v = decode(&f.world.summary_net_data_for_admin_mod_owner_cloud_mode(false, false, false, 0));
        assert_eq!(keys(&v), BASE.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(v.object_for_key("randomSeed").unwrap().unsigned_int_value(), 0x9000_0000);
        assert_eq!(v.object_for_key("minorVersion").unwrap().int_value(), 3);
        assert!(matches!(v.object_for_key("expertMode"), Some(PlistValue::Bool(false))));
        assert_eq!(v.object_for_key("credit").unwrap().float_value(), 1.5);
        assert_eq!(v.object_for_key("worldTime").unwrap().double_value(), 2704.5);
        assert_eq!(v.object_for_key("worldName").unwrap().as_str(), Some("TESTWORLD"));
        assert_eq!(v.object_for_key("saveID").unwrap().as_str(), Some("sv"));
    }

    #[test]
    fn conditions_follow_the_method_parameters_not_the_world_ivars() {
        let f = base_world();
        let w = &f.world;
        assert!(w.is_owner() && w.is_admin() && w.is_mod()); // ivars are YES but must not matter
        *w.server_password.borrow_mut() = Some("secret".into());
        f.rec.privacy.set(3);
        let extra = |a, m, o, c| {
            let v = decode(&w.summary_net_data_for_admin_mod_owner_cloud_mode(a, m, o, c));
            keys(&v).into_iter().filter(|k| !BASE.contains(&k.as_str())).collect::<Vec<_>>()
        };
        assert_eq!(extra(false, false, false, 0), Vec::<String>::new());
        assert_eq!(extra(false, true, false, 0), vec!["mod"]);
        assert_eq!(extra(true, false, false, 0), vec!["admin", "mod"]);
        assert_eq!(extra(false, false, true, 0), vec!["admin", "mod", "owner", "privacy", "serverPassword"]);
        assert_eq!(extra(false, false, false, 2), vec!["supermod"]);
        let v = decode(&w.summary_net_data_for_admin_mod_owner_cloud_mode(false, false, true, 2));
        assert_eq!(v.object_for_key("privacy").unwrap().int_value(), 3);
        assert_eq!(v.object_for_key("supermod").unwrap().int_value(), 2);
        assert_eq!(v.object_for_key("serverPassword").unwrap().as_str(), Some("secret"));
        assert!(matches!(v.object_for_key("owner"), Some(PlistValue::Bool(true))));
    }

    #[test]
    fn an_owner_summary_without_a_server_has_no_privacy_and_no_password() {
        let f = base_world();
        let w = &f.world;
        *w.server.borrow_mut() = None;
        let v = decode(&w.summary_net_data_for_admin_mod_owner_cloud_mode(false, false, true, 0));
        assert!(v.object_for_key("privacy").is_none() && v.object_for_key("serverPassword").is_none());
        assert!(matches!(v.object_for_key("owner"), Some(PlistValue::Bool(true))));
    }

    #[test]
    fn unconditional_optional_keys() {
        let f = base_world();
        let w = &f.world;
        w.set_pvp_enabled(false);
        *w.welcome_message.borrow_mut() = Some("hello".into());
        let mut prices = new_mutable_dict();
        prices.map.insert("298".into(), PlistValue::real_f64(0.5));
        *w.world_price_multipliers.borrow_mut() = Some(prices);
        *w.custom_rules_dict.borrow_mut() = Some(new_mutable_dict());
        // an empty ownershipSignPositions dictionary is omitted (count == 0)
        *w.ownership_sign_positions.borrow_mut() = Some(new_mutable_dict());
        let v = decode(&w.summary_net_data_for_admin_mod_owner_cloud_mode(false, false, false, 0));
        let extra: Vec<String> = keys(&v).into_iter().filter(|k| !BASE.contains(&k.as_str())).collect();
        assert_eq!(extra, vec!["customRules", "disablePvp", "welcomeMessage", "worldPriceMultipliers"]);
        let mut signs = new_mutable_dict();
        signs.map.insert("5".into(), PlistValue::array(vec![]));
        *w.ownership_sign_positions.borrow_mut() = Some(signs);
        let v = decode(&w.summary_net_data_for_admin_mod_owner_cloud_mode(false, false, false, 0));
        assert!(keys(&v).contains(&"ownershipSignPositions".to_string()));
        // dictionaryWithObjectsAndKeys: stops at a nil saveID
        *w.save_id.borrow_mut() = None;
        let v = decode(&w.summary_net_data_for_admin_mod_owner_cloud_mode(false, false, false, 0));
        assert!(!keys(&v).contains(&"saveID".to_string()) && !keys(&v).contains(&"worldName".to_string()));
        assert!(keys(&v).contains(&"portalLevel".to_string()));
    }

    #[test]
    fn heartbeat_bytes() {
        let f = base_world();
        let w = &f.world;
        f.rec.credit.set(2.0);
        w.set_fast_forward(true);
        let hb = w.heartbeat_data();
        assert_eq!(hb.len(), 17);
        assert_eq!(hb[0], 0x17);
        assert_eq!(&hb[1..5], &(2704.5f64 as f32).to_le_bytes());
        assert_eq!(&hb[5..9], &4.5f32.to_le_bytes());
        assert_eq!(&hb[9..13], &[1, 0, 0, 0]); // fastForward, localPaused, allPaused, pvpDisabled
        assert_eq!(&hb[13..17], &2.0f32.to_le_bytes());
        w.set_pvp_enabled(false);
        assert_eq!(w.heartbeat_data()[12], 1);
    }

    #[test]
    fn send_heartbeat_needs_a_server_client_and_goes_to_all_connected() {
        let f = base_world();
        let w = &f.world;
        w.send_heartbeat_data();
        assert!(f.rec.sends.borrow().is_empty());
        assert!(connect(&f, "a"));
        w.send_heartbeat_data();
        let sends = f.rec.sends.borrow();
        assert_eq!(sends.len(), 1);
        assert_eq!(sends[0].0.len(), 17);
        assert_eq!(sends[0].1, None);
        assert!(sends[0].2);
    }

    #[test]
    fn heartbeat_received_flips_pause_and_rebroadcasts_only_for_connected_known_peers() {
        let f = base_world();
        let w = &f.world;
        assert!(connect(&f, "a"));
        let mut pkt = vec![0u8; 16];
        pkt[9] = 1; // localPaused
                    // unknown peer: nothing
        w.heartbeat_data_recieved_from_peer(&pkt, "zz");
        assert!(f.rec.sends.borrow().is_empty());
        w.heartbeat_data_recieved_from_peer(&pkt, "a");
        assert!(f.rec.clients.borrow()["a"].paused);
        assert_eq!(f.rec.sends.borrow().len(), 1);
        // same state again: no flip, no broadcast
        w.heartbeat_data_recieved_from_peer(&pkt, "a");
        assert_eq!(f.rec.sends.borrow().len(), 1);
        // payload of 0..9 bytes is localPaused = 0 (UB-PARITY), which un-pauses
        w.heartbeat_data_recieved_from_peer(&[1, 2, 3], "a");
        assert!(!f.rec.clients.borrow()["a"].paused);
        assert_eq!(f.rec.sends.borrow().len(), 2);
        // disconnected client: ignored
        f.rec.clients.borrow_mut().get_mut("a").unwrap().connected = false;
        w.heartbeat_data_recieved_from_peer(&pkt, "a");
        assert!(!f.rec.clients.borrow()["a"].paused);
    }
}
