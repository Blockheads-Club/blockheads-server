//! Replays of the protocol vectors of the original. They need the private fixtures and skip without them, so they live in
//! this file, which the coverage gate leaves out (`*_golden.rs`).

#[cfg(test)]
mod golden_tests {
    use super::super::*;
    use crate::protocol::block_wire::*;
    use crate::protocol::object_batches::*;

    fn vec_file(dir: &str, prefix: &str) -> Option<Vec<u8>> {
        let d = crate::testenv::modules(&format!("protocol_vectors/{dir}"));
        for e in std::fs::read_dir(d).ok()? {
            let e = e.ok()?;
            if e.file_name().to_string_lossy().starts_with(prefix) {
                return std::fs::read(e.path()).ok();
            }
        }
        None
    }

    #[test]
    fn vectors_static() {
        let Some(v) = vec_file("solo", "000_t23") else { return };
        assert_eq!(initial_contact(None, false, "e98c5bee38be74e00ad7748a3dd42119"), v);
        let v = vec_file("solo", "009_t27").unwrap();
        assert_eq!(block_not_loaded(1), v);
        let v = vec_file("solo", "012_t27").unwrap();
        assert_eq!(block_not_loaded(100000), v);
        let v = vec_file("solo", "006_t17").unwrap();
        let hb: WorldHeartbeat = decode_or0(&v[1..]);
        assert_eq!(heartbeat(&hb), v);
        let v = vec_file("solo", "004_t1e").unwrap();
        assert_eq!(player_list(&[]), v);
        let v = vec_file("solo", "010_t36").unwrap();
        assert_eq!(welcome_message(None, None), v);
        let v = vec_file("solo", "005_t0f").unwrap();
        let ServerMsg::UniqueId(id) = decode_server_packet(&v).unwrap() else { panic!() };
        assert_eq!(unique_id_reply(id), v);
        let v = vec_file("two_clients", "013_t13").unwrap();
        let ServerMsg::PickupReply { accepted, requester, ids } = decode_server_packet(&v).unwrap() else { panic!() };
        let mine = pickup_reply(accepted, requester, &ids);
        assert_eq!(mine[..2], v[..2]);
        assert_eq!(mine[8..], v[8..]);
        // 0x3c: 1 entry
        let v = vec_file("solo", "011_t3c").unwrap();
        let ServerMsg::OwnershipSignPlayerList(Some(l)) = decode_server_packet(&v).unwrap() else { panic!() };
        assert_eq!(ownership_sign_player_list(&l), v);
        // 0x1e with entry
        let v = vec_file("two_clients", "001_t1e").unwrap();
        let ServerMsg::PlayerList(Some(l)) = decode_server_packet(&v).unwrap() else { panic!() };
        assert_eq!(xml_packet(t::SERVER_PLAYER_LIST_CHANGED_NOTIFICATION, &l), v);
        // 0x24 relay
        let v = vec_file("two_clients", "B_003").unwrap();
        let ServerMsg::Chat(Some(l)) = decode_server_packet(&v).unwrap() else { panic!() };
        assert_eq!(chat_message(&l), v);
    }

    #[test]
    fn vectors_gz() {
        let Some(v) = vec_file("solo", "001_t01") else { return };
        let ServerMsg::Summary(Some(d)) = decode_server_packet(&v).unwrap() else { panic!() };
        let s = WorldSummary {
            random_seed: 1792191220,
            world_time: 2700.0,
            no_rain_timer: 0.0,
            credit: 0.0,
            start_portal_x: 6837,
            start_portal_y: 533,
            highest_point_x: 14428,
            highest_point_y: 782,
            portal_level: 0,
            world_width_macro: 512,
            save_id: "e98c5bee38be74e00ad7748a3dd42119".into(),
            world_name: "PROTO".into(),
            expert_mode: false,
            ..Default::default()
        };
        assert_eq!(plist_xml(&d), plist_xml(&s.to_plist()));
        assert_eq!(world_summary(&s), v);
        // 0x28
        let v = vec_file("solo", "002_t28").unwrap();
        let m = decode_light_block(&v[1..]).unwrap();
        let light: [u8; LIGHT_LEN] = m.light.clone().try_into().unwrap();
        assert_eq!(light_block(m.macro_index as i32, &light, m.update_index), v);
        // 0x2f empty
        let d = PV::dict(GsDict::new());
        let p = price_offsets(&d);
        assert_eq!(p.len(), 163); // observed 163 bytes
                                  // 0x04 captures
        for (dir, pre) in
            [("solo", "007_t04"), ("solo", "008_t04"), ("two_clients", "015_t04"), ("two_clients", "004_t04")]
        {
            let v = vec_file(dir, pre).unwrap();
            let b = decode_server_block(&v[1..]).unwrap();
            let tiles: [u8; TILES_LEN] = b.tiles.clone().try_into().unwrap();
            let light: [u8; LIGHT_LEN] = b.light.clone().try_into().unwrap();
            let extra = b.extra.as_ref().unwrap();
            let receipts = match extra.object_for_key("receipts") {
                Some(PV::Array(a)) => Some(a.items.iter().map(|x| x.unsigned_int_value()).collect()),
                _ => None,
            };
            let ex = ExtraData { hash: extra.object_for_key("hash").unwrap().as_str().unwrap().to_string(), receipts };
            let mine =
                server_block_from_parts(b.macro_index, &tiles, b.version, b.update_index, Some((&light, &ex))).unwrap();
            assert_eq!(mine, v, "{pre}");
        }
        // 0x07 batches
        for (pre, ty) in [("006_t07", 14u8), ("005_t07", 45), ("008_t07", 10)] {
            let v = vec_file("two_clients", pre).unwrap();
            let b = decode_batch(&v).unwrap().unwrap();
            assert_eq!(b.object_type, ty);
            assert_eq!(encode_batch(BatchKind::Create, ty, &b.items).unwrap(), v, "{pre}");
        }
        // 0x01 for second client, 0x2f roundtrip
        let v = vec_file("two_clients", "B_001_t01").unwrap();
        let ServerMsg::Summary(Some(d)) = decode_server_packet(&v).unwrap() else { panic!() };
        assert_eq!(gz_packet(t::WORLD_SUMMARY, &d), v);
    }
}
