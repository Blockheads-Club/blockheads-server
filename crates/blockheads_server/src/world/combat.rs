//! Projectiles (0x29), blockhead damage (0x2a), electricity paths (0x2c), tips and PVP (0x2d).
//!
//! Only the `server != nil` branches are
//! ported; every `client != nil` branch is dead on the dedicated server.

use crate::fmath::cconv::f32_to_i32;
use crate::foundation::cfmt::fmt_f;
use crate::foundation::exception::ns_raise;
use crate::foundation::gzip::{gzip_deflate, gzip_inflate};
use crate::nslog;
use crate::objects::handle::ObjRef;
use crate::types::netdata::{BlockheadDamageNetRequest, ElectricityParticleHeader, ProjectileNetRequest};
use crate::types::structs::{ElectrictyParticlePathIndex, Vector2, WireStruct};
use crate::world::lifecycle::core_hooks;
use crate::world::state::World;
use crate::world::tiles::macro_index_at_world_index;

/// `BHNDT_CLIENT_PROJECTILE_REQUEST`
const BHNDT_PROJECTILE: u8 = 0x29;
/// `BHNDT_CLIENT_BLOCKHEAD_DAMAGE_REQUEST`
const BHNDT_DAMAGE: u8 = 0x2a;
/// `BHNDT_ELECTRICITY_PATH`
const BHNDT_ELECTRICITY_PATH: u8 = 0x2c;
/// `BHNDT_TIP_NOTIFICATION`
const BHNDT_TIP_NOTIFICATION: u8 = 0x2d;

/// `-[NSData getBytes:length:]` never raises: it clamps to the data length. The rest of the
/// destination is uninitialised stack in the original; we zero-fill.
fn padded(data: &[u8], len: usize) -> Vec<u8> {
    let mut v = vec![0u8; len];
    let n = data.len().min(len);
    v[..n].copy_from_slice(&data[..n]);
    v
}

impl World {
    /// `-[World fireProjectileFrom:to:at:fireItemType:firer:]`: fire locally, then broadcast `0x29 +
    /// ProjectileNetRequest` (21 bytes) reliably to ALL connected clients including the originator. `at` and
    /// `firer` are not sent.
    pub fn fire_projectile_from_to_at_fire_item_type_firer(
        &self,
        from_pos: Vector2,
        to_pos: Vector2,
        at_object: Option<&ObjRef>,
        item_type: i32,
        firer: Option<&ObjRef>,
    ) {
        if self.projectile_manager.borrow().is_some() {
            core_hooks().projectile_manager_fire(from_pos, to_pos, at_object, item_type, firer);
        }
        if self.server.borrow().is_some() {
            let req = ProjectileNetRequest {
                from_pos_x: from_pos.x,
                from_pos_y: from_pos.y,
                to_pos_x: to_pos.x,
                to_pos_y: to_pos.y,
                item_type: item_type as u16, // low 16 bits
                gap_12: [0; 2],
            };
            let mut pkt = vec![BHNDT_PROJECTILE];
            req.append_to(&mut pkt);
            self.send_to_peers(&pkt, None, true);
        }
    }

    /// `-[World remoteProjectileRequest:]`: handler for 0x29 (payload without the type byte). Needs 20 bytes
    /// (`NSRangeException` otherwise); fires with `at = nil, firer = nil`; no rebroadcast here.
    pub fn remote_projectile_request(&self, remote_data: &[u8]) {
        let req = ProjectileNetRequest::read_le(&padded(remote_data, ProjectileNetRequest::SIZE));
        if self.projectile_manager.borrow().is_some() {
            core_hooks().projectile_manager_fire(
                Vector2 { x: req.from_pos_x, y: req.from_pos_y },
                Vector2 { x: req.to_pos_x, y: req.to_pos_y },
                None,
                req.item_type as i32,
                None,
            );
        }
    }

    /// `-[World sendDamage:forNetBlockhead:recoil:]`: `0x2a` + 16 bytes to the owning client only; nothing for a
    /// negative damage. A nil client id raises (`arrayWithObject:nil`).
    pub fn send_damage_for_net_blockhead_recoil(&self, damage: f32, blockhead: &ObjRef, recoil: bool) {
        if damage >= 0.0 && self.server.borrow().is_some() {
            let req = BlockheadDamageNetRequest {
                blockhead_id: blockhead.base().borrow().unique_id,
                damage,
                recoil: recoil as u8,
                gap_d: [0; 3],
            };
            let mut pkt = vec![BHNDT_DAMAGE];
            req.append_to(&mut pkt);
            let cid = match core_hooks().blockhead_client_id(blockhead) {
                Some(c) => c,
                // `+arrayWithObject:nil` guard
                None => ns_raise("NSInvalidArgumentException", "Attempt to add nil to an array"),
            };
            self.send_to_peers(&pkt, Some(&[cid]), true);
        }
    }

    /// `-[World remoteBlockheadDamageRequest:requestedByClientName:]`: handler for 0x2a. Needs 16 bytes. Proceeds only
    /// if `server == nil || pvpEnabled`; damages the FIRST blockhead with that unique id, always with `recoil = YES`
    /// (the request's recoil byte is ignored).
    pub fn remote_blockhead_damage_request_requested_by_client_name(
        &self,
        remote_data: &[u8],
        requested_by_client_name: Option<&str>,
    ) {
        let req = BlockheadDamageNetRequest::read_le(&padded(remote_data, BlockheadDamageNetRequest::SIZE));
        if !(self.server.borrow().is_none() || self.pvp_enabled()) {
            return;
        }
        let hooks = core_hooks();
        let found = if self.dynamic_world.borrow().is_some() {
            hooks
                .dynamic_world_all_blockheads_including_net()
                .into_iter()
                .find(|b| b.base().borrow().unique_id == req.blockhead_id)
        } else {
            None
        };
        if let Some(b) = found {
            let is_sim = self.is_simulating() || hooks.blockhead_is_client_blockhead_being_controlled_by_server(&b);
            hooks.blockhead_suffer_damage_is_simulation_recoil(&b, req.damage, is_sim, true);
            if let Some(by) = requested_by_client_name {
                if self.server.borrow().is_some() {
                    let cid = hooks.blockhead_client_id(&b);
                    let owner = hooks.server_player_name_for_player_with_id_including_old_players(cid.as_deref());
                    nslog!(
                        "Blockhead named {} owned by player {} was harmed by player {}.",
                        hooks.blockhead_name(&b).unwrap_or("(null)".to_string()),
                        owner.unwrap_or("(null)".to_string()),
                        by
                    );
                }
            }
        }
    }

    /// `-[World sendNetDataForElectricityParticlePathIfRequired:size:ignoreClient:]`: runs only for a path of more
    /// than two elements. Interested clients are `serverClients` in key order, `!= ignoreClient`, connected, with a
    /// wired block at either end. Packet `0x2c | u16 size | u16 pathCount | 4 pad | gzip(path * 8)`, sent with
    /// `reliable = false`.
    pub fn send_net_data_for_electricity_particle_path_if_required_size_ignore_client(
        &self,
        path: Vec<ElectrictyParticlePathIndex>,
        size: f32,
        ignore_client: Option<&str>,
    ) {
        if !(self.server.borrow().is_some() && path.len() > 2) {
            return;
        }
        let hooks = core_hooks();
        let a = path.first().expect("len > 2").index as i32;
        let b = path.last().expect("len > 2").index as i32;
        let mut interested: Vec<String> = Vec::new();
        for id in self.server_client_keys() {
            if Some(id.as_str()) != ignore_client && self.server_client_is_connected(&id) {
                if hooks.server_client_block_is_wired(&id, macro_index_at_world_index(a, self))
                    || hooks.server_client_block_is_wired(&id, macro_index_at_world_index(b, self))
                {
                    interested.push(id);
                }
            }
        }
        if interested.is_empty() {
            return; // server with no recipient
        }
        let hdr =
            ElectricityParticleHeader { size: f32_to_i32(size) as u16, path_count: path.len() as u16, padding: [0; 4] };
        let mut pkt = vec![BHNDT_ELECTRICITY_PATH];
        hdr.append_to(&mut pkt);
        let mut raw = Vec::with_capacity(path.len() * 8);
        for e in &path {
            e.append_to(&mut raw);
        }
        pkt.extend_from_slice(&gzip_deflate(&raw));
        self.send_to_peers(&pkt, Some(&interested), false);
    }

    /// `-[World electricityPathDataRecieved:fromClient:]`: handler for 0x2c. Decodes the path, shows the (empty)
    /// particle effect and re-sends to the other interested clients.
    pub fn electricity_path_data_recieved_from_client(&self, data: &[u8], from_client: &str) {
        if data.len() < ElectricityParticleHeader::SIZE {
            // `subdataWithRange:{8, len-8}` with unsigned wrap (backstop; dispatcher rejects first)
            ns_raise("NSRangeException", "Range location + length too great");
        }
        let hdr = ElectricityParticleHeader::read_le(data);
        let body = gzip_inflate(&data[ElectricityParticleHeader::SIZE..]);
        if let Some(body) = body {
            let need = hdr.path_count as usize * 8;
            let body = padded(&body, need); // `getBytes:length:` clamps, never raises (Q7b)
            let path: Vec<ElectrictyParticlePathIndex> =
                (0..hdr.path_count as usize).map(|i| ElectrictyParticlePathIndex::read_le(&body[i * 8..])).collect();
            core_hooks().particle_emitter_do_add_electricity_particle_with_path_size(&path, hdr.size as f32);
            if self.server.borrow().is_some() {
                self.send_net_data_for_electricity_particle_path_if_required_size_ignore_client(
                    path,
                    hdr.size as f32,
                    Some(from_client),
                );
            }
        }
    }

    /// `-[World sendAndDisplayAirTimeTipWithDistance:]`: local tip (no-op display) plus `0x2d` + UTF-8 text sent
    /// UNRELIABLY to everybody.
    pub fn send_and_display_air_time_tip_with_distance(&self, distance: f32) {
        let hooks = core_hooks();
        // `%.1f` with glibc semantics (also for nan/inf), foundation cfmt
        let local = format!("YOU GOT {} BLOCKS OF AIR ON A {}!", fmt_f(distance as f64, 1), "HANDCAR");
        hooks.tip_manager_display_tip(&local);
        if self.server.borrow().is_some() {
            let name = hooks.server_local_player_name();
            let msg = format!("{} GOT {} BLOCKS OF AIR ON A {}!", name, fmt_f(distance as f64, 1), "HANDCAR");
            let mut pkt = vec![BHNDT_TIP_NOTIFICATION];
            pkt.extend_from_slice(msg.as_bytes());
            self.send_to_peers(&pkt, None, false);
        }
    }

    /// `-[World setPVPEnabled:displayNotifciation:]`: only acts on a change; with `display` it shows a tip and
    /// broadcasts `0x2d` + text reliably.
    pub fn set_pvp_enabled_display_notifciation(&self, pvp_enabled: bool, display_notifciation: bool) {
        if self.pvp_enabled() != pvp_enabled {
            self.set_pvp_enabled(pvp_enabled);
            if display_notifciation {
                let text = if pvp_enabled { "PVP IS NOW ENABLED" } else { "PVP IS NOW DISABLED" };
                core_hooks().tip_manager_display_tip(text);
                if self.server.borrow().is_some() {
                    let mut pkt = vec![BHNDT_TIP_NOTIFICATION];
                    pkt.extend_from_slice(text.as_bytes());
                    self.send_to_peers(&pkt, None, true);
                }
            }
        }
    }

    /// `-[World sendSetPVPEnabledToServer:]`: on the server `setPVPEnabled:displayNotifciation:YES`.
    pub fn send_set_pvp_enabled_to_server(&self, new_enabled: bool) {
        if self.server.borrow().is_some() {
            self.set_pvp_enabled_display_notifciation(new_enabled, true);
        }
    }

    /// `-[World remoteTipNotification:]`: handler for 0x2d; the whole payload is UTF-8 text, only a HUD tip.
    pub fn remote_tip_notification(&self, remote_tip_data: &[u8]) {
        if let Ok(s) = std::str::from_utf8(remote_tip_data) {
            core_hooks().tip_manager_display_tip(s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::exception::catch_ns;
    use crate::foundation::gzip::gzip_inflate;
    use crate::types::structs::IntPair;
    use crate::world::lifecycle::testkit::*;
    use std::rc::Rc;

    fn bh(f: &Fixture, id: u64, client: Option<&str>) -> ObjRef {
        let o = crate::world::helpers::testutil::new_obj();
        o.base().borrow_mut().unique_id = id;
        f.rec.bh.borrow_mut().insert(
            id,
            Bh { client_id: client.map(|s| s.to_string()), name: Some(format!("BH{}", id)), ..Default::default() },
        );
        f.rec.blockheads.borrow_mut().push(o.clone());
        o
    }

    #[test]
    fn fire_projectile_fires_locally_and_broadcasts_21_bytes_reliably() {
        let f = fixture(32);
        let w = &f.world;
        *w.projectile_manager.borrow_mut() = Some(Rc::new(crate::world::state::ProjectileManager {}));
        w.fire_projectile_from_to_at_fire_item_type_firer(
            Vector2 { x: 1.0, y: 2.0 },
            Vector2 { x: 3.0, y: 4.0 },
            None,
            0x1_0007,
            None,
        );
        assert_eq!(f.rec.fired.borrow().len(), 1);
        let sends = f.rec.sends.borrow();
        let (pkt, peers, rel) = &sends[0];
        assert_eq!((pkt.len(), pkt[0], peers.is_none(), *rel), (21, 0x29, true, true));
        assert_eq!(&pkt[1..5], &1.0f32.to_le_bytes());
        assert_eq!(&pkt[13..17], &4.0f32.to_le_bytes());
        assert_eq!(&pkt[17..19], &7u16.to_le_bytes(), "low 16 bits of the item type");
        assert_eq!(&pkt[19..21], &[0, 0]);
    }

    #[test]
    fn remote_projectile_request_fires_without_rebroadcast_and_short_payload_clamps() {
        let f = fixture(32);
        let w = &f.world;
        *w.projectile_manager.borrow_mut() = Some(Rc::new(crate::world::state::ProjectileManager {}));
        let mut d = Vec::new();
        ProjectileNetRequest {
            from_pos_x: 1.0,
            from_pos_y: 2.0,
            to_pos_x: 3.0,
            to_pos_y: 4.0,
            item_type: 9,
            gap_12: [0; 2],
        }
        .append_to(&mut d);
        w.remote_projectile_request(&d);
        assert_eq!(f.rec.fired.borrow()[0].2, 9);
        assert!(f.rec.sends.borrow().is_empty());
        // short payload: `getBytes:length:` clamps and never raises; the tail is zero-filled
        w.remote_projectile_request(&d[..19]);
        assert_eq!(f.rec.fired.borrow().len(), 2);
    }

    #[test]
    fn send_damage_goes_to_the_owning_client_only() {
        let f = fixture(32);
        let w = &f.world;
        let b = bh(&f, 0x1122, Some("owner"));
        w.send_damage_for_net_blockhead_recoil(2.5, &b, true);
        w.send_damage_for_net_blockhead_recoil(-1.0, &b, true); // negative: nothing
        let sends = f.rec.sends.borrow();
        assert_eq!(sends.len(), 1);
        let (pkt, peers, rel) = &sends[0];
        assert_eq!((pkt.len(), pkt[0]), (17, 0x2a));
        assert_eq!(&pkt[1..9], &0x1122u64.to_le_bytes());
        assert_eq!(&pkt[9..13], &2.5f32.to_le_bytes());
        assert_eq!(pkt[13], 1);
        assert_eq!(peers.clone().unwrap(), vec!["owner".to_string()]);
        assert!(*rel);
        drop(sends);
        let nil = bh(&f, 7, None);
        assert_eq!(
            catch_ns(|| w.send_damage_for_net_blockhead_recoil(1.0, &nil, false)).unwrap_err().name,
            "NSInvalidArgumentException"
        );
    }

    fn damage_payload(id: u64, dmg: f32) -> Vec<u8> {
        BlockheadDamageNetRequest { blockhead_id: id, damage: dmg, recoil: 0, gap_d: [0; 3] }.to_vec()
    }

    #[test]
    fn remote_damage_respects_pvp_logs_and_ignores_the_recoil_byte() {
        let f = fixture(32);
        let w = &f.world;
        with_dw(w);
        let _a = bh(&f, 1, Some("c1"));
        let _b = bh(&f, 2, Some("c2"));
        let _dup = bh(&f, 2, Some("c3")); // duplicate id: first match wins
        w.set_pvp_enabled(false);
        let log = capture_log(|| {
            w.remote_blockhead_damage_request_requested_by_client_name(&damage_payload(2, 3.0), Some("attacker"))
        });
        assert!(!log.contains("Blockhead named"));
        assert!(f.rec.bh.borrow()[&2].damage_log.is_empty());
        w.set_pvp_enabled(true);
        let log = capture_log(|| {
            w.remote_blockhead_damage_request_requested_by_client_name(&damage_payload(2, 3.0), Some("attacker"))
        });
        assert_eq!(f.rec.bh.borrow()[&2].damage_log, vec![(3.0, false, true)]);
        assert!(
            strip_log(&log)
                .contains(&"Blockhead named BH2 owned by player N-c3 was harmed by player attacker.".to_string()),
            "{}",
            log
        );
        // unknown id: nothing; requested_by nil: no log
        let log =
            capture_log(|| w.remote_blockhead_damage_request_requested_by_client_name(&damage_payload(99, 3.0), None));
        assert!(!log.contains("Blockhead named"));
        let log =
            capture_log(|| w.remote_blockhead_damage_request_requested_by_client_name(&damage_payload(1, -2.0), None));
        assert!(!log.contains("Blockhead named"));
        assert_eq!(f.rec.bh.borrow()[&1].damage_log, vec![(-2.0, false, true)]);
        w.remote_blockhead_damage_request_requested_by_client_name(&[0u8; 15], None);
        // clamps, no raise
    }

    fn path(n: usize) -> Vec<ElectrictyParticlePathIndex> {
        (0..n)
            .map(|i| ElectrictyParticlePathIndex { index: 100 + i as u32 * 3, solid: (i % 2) as u8, padding: [0; 3] })
            .collect()
    }

    #[test]
    fn electricity_fan_out_goes_to_wired_connected_clients_unreliably() {
        let f = fixture(32);
        let w = &f.world;
        for id in ["a", "b", "c"] {
            assert!(w.client_connected(id));
        }
        let p = path(5);
        let first = macro_index_at_world_index(p[0].index as i32, w);
        let last = macro_index_at_world_index(p[4].index as i32, w);
        f.rec.clients.borrow_mut().get_mut("a").unwrap().wired.insert(first);
        f.rec.clients.borrow_mut().get_mut("b").unwrap().wired.insert(last);
        f.rec.clients.borrow_mut().get_mut("c").unwrap().wired.insert(first);
        f.rec.clients.borrow_mut().get_mut("c").unwrap().connected = false;
        // too short a path: nothing
        w.send_net_data_for_electricity_particle_path_if_required_size_ignore_client(path(2), 3.7, None);
        assert!(f.rec.sends.borrow().is_empty());
        w.send_net_data_for_electricity_particle_path_if_required_size_ignore_client(p.clone(), 3.7, Some("a"));
        {
            let sends = f.rec.sends.borrow();
            assert_eq!(sends.len(), 1);
            let (pkt, peers, rel) = &sends[0];
            assert_eq!(peers.clone().unwrap(), vec!["b".to_string()]);
            assert!(!*rel);
            assert_eq!(pkt[0], 0x2c);
            assert_eq!(&pkt[1..3], &3u16.to_le_bytes(), "size = (uint16)(int)3.7");
            assert_eq!(&pkt[3..5], &5u16.to_le_bytes());
            assert_eq!(&pkt[5..9], &[0; 4]);
            let body = gzip_inflate(&pkt[9..]).unwrap();
            assert_eq!(body.len(), 40);
            assert_eq!(&body[0..4], &100u32.to_le_bytes());
            assert_eq!(body[12], 1);
        }
        // no interested client at all: nothing sent
        w.send_net_data_for_electricity_particle_path_if_required_size_ignore_client(p, 3.7, Some("zz"));
        assert_eq!(f.rec.sends.borrow().len(), 2, "a and b are interested when nobody is ignored");
    }

    #[test]
    fn electricity_receive_re_sends_ignoring_the_sender_and_validates_lengths() {
        let f = fixture(32);
        let w = &f.world;
        for id in ["a", "b"] {
            assert!(w.client_connected(id));
        }
        let p = path(4);
        let m = macro_index_at_world_index(p[0].index as i32, w);
        for id in ["a", "b"] {
            f.rec.clients.borrow_mut().get_mut(id).unwrap().wired.insert(m);
        }
        let mut raw = Vec::new();
        for e in &p {
            e.append_to(&mut raw);
        }
        let mut data = ElectricityParticleHeader { size: 5, path_count: 4, padding: [0; 4] }.to_vec();
        data.extend_from_slice(&gzip_deflate(&raw));
        w.electricity_path_data_recieved_from_client(&data, "a");
        let sends = f.rec.sends.borrow();
        assert_eq!(sends.len(), 1);
        assert_eq!(sends[0].1.clone().unwrap(), vec!["b".to_string()]);
        drop(sends);
        let e = catch_ns(|| w.electricity_path_data_recieved_from_client(&data[..7], "a")).unwrap_err();
        assert_eq!(
            (e.name.as_str(), e.reason.as_deref()),
            ("NSRangeException", Some("Range location + length too great"))
        );
        // inflated body shorter than pathCount * 8
        let mut short = ElectricityParticleHeader { size: 5, path_count: 9, padding: [0; 4] }.to_vec();
        short.extend_from_slice(&gzip_deflate(&raw));
        let n = f.rec.sends.borrow().len();
        w.electricity_path_data_recieved_from_client(&short, "a"); // zero-padded path, no raise
        assert_eq!(f.rec.sends.borrow().len(), n + 1);
        // garbage body: gzipInflate gives nil -> nothing
        let mut junk = ElectricityParticleHeader { size: 5, path_count: 4, padding: [0; 4] }.to_vec();
        junk.extend_from_slice(b"not gzip");
        w.electricity_path_data_recieved_from_client(&junk, "a");
        let _ = IntPair { x: 0, y: 0 };
    }

    #[test]
    fn tips_and_pvp() {
        let f = fixture(32);
        let w = &f.world;
        w.send_and_display_air_time_tip_with_distance(12.345);
        assert_eq!(f.rec.tips.borrow()[0], "YOU GOT 12.3 BLOCKS OF AIR ON A HANDCAR!");
        {
            let sends = f.rec.sends.borrow();
            assert_eq!(sends[0].0[0], 0x2d);
            assert_eq!(&sends[0].0[1..], b"HOST GOT 12.3 BLOCKS OF AIR ON A HANDCAR!");
            assert!(!sends[0].2, "air time tip is unreliable");
        }
        w.set_pvp_enabled(false);
        w.set_pvp_enabled_display_notifciation(false, true); // unchanged: nothing
        assert_eq!(f.rec.sends.borrow().len(), 1);
        w.set_pvp_enabled_display_notifciation(true, true);
        assert!(w.pvp_enabled());
        {
            let sends = f.rec.sends.borrow();
            assert_eq!(&sends[1].0[1..], b"PVP IS NOW ENABLED");
            assert!(sends[1].2 && sends[1].1.is_none());
        }
        w.send_set_pvp_enabled_to_server(false);
        assert!(!w.pvp_enabled());
        assert_eq!(&f.rec.sends.borrow()[2].0[1..], b"PVP IS NOW DISABLED");
        w.set_pvp_enabled_display_notifciation(true, false); // changes silently
        assert!(w.pvp_enabled());
        assert_eq!(f.rec.sends.borrow().len(), 3);
        w.remote_tip_notification(b"hello");
        assert_eq!(f.rec.tips.borrow().last().unwrap(), "hello");
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;
    use crate::world::lifecycle::testkit::*;

    fn damage(id: u64) -> Vec<u8> {
        let mut v = id.to_le_bytes().to_vec();
        v.extend_from_slice(&3.0f32.to_le_bytes());
        v.extend_from_slice(&[0, 0, 0, 0]);
        v
    }

    #[test]
    fn damage_needs_a_dynamic_world_and_logs_only_on_servers() {
        let f = fixture(32);
        let w = &f.world;
        // no dynamic world: nobody to hurt
        w.set_pvp_enabled(true);
        w.remote_blockhead_damage_request_requested_by_client_name(&damage(1), Some("x"));
        // a world without a server hurts silently
        with_dw(w);
        let o = crate::world::helpers::testutil::new_obj();
        o.base().borrow_mut().unique_id = 1;
        f.rec.bh.borrow_mut().insert(1, Bh { name: Some("B".into()), ..Default::default() });
        f.rec.blockheads.borrow_mut().push(o);
        *w.server.borrow_mut() = None;
        let log = capture_log(|| w.remote_blockhead_damage_request_requested_by_client_name(&damage(1), Some("x")));
        assert!(!log.contains("harmed"));
        assert_eq!(f.rec.bh.borrow()[&1].damage_log.len(), 1);
    }

    #[test]
    fn electricity_goes_to_clients_wired_at_either_end_only() {
        let f = fixture(32);
        let w = &f.world;
        assert!(w.client_connected("a"));
        let path: Vec<ElectrictyParticlePathIndex> = [0u32, 1, 160]
            .iter()
            .map(|&index| ElectrictyParticlePathIndex { index, solid: 0, padding: [0; 3] })
            .collect();
        // wired at neither end
        w.send_net_data_for_electricity_particle_path_if_required_size_ignore_client(path.clone(), 1.0, None);
        assert!(f.rec.sends.borrow().is_empty());
        // wired at the far end only
        f.rec.clients.borrow_mut().get_mut("a").unwrap().wired.insert(5);
        w.send_net_data_for_electricity_particle_path_if_required_size_ignore_client(path, 1.0, None);
        assert_eq!(f.rec.sends.borrow().len(), 1);
    }
}
