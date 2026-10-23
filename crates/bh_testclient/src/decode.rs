//! Human-readable and JSON decoders of server->client and client->server packets.
//!
//! Built on `protocol::messages::{decode_server_packet, decode_client_packet}`; never panics.

use blockheads_server::foundation::md5::data_string_from_md5;
use blockheads_server::plist::value::PlistValue as PV;
use blockheads_server::protocol::codec::{reject, Malformed};
use blockheads_server::protocol::messages::{decode_client_packet, decode_server_packet, ClientMsg, ServerMsg};
use blockheads_server::types::enums::DynamicObjectType;
use blockheads_server::types::net_types::NetDataType;

use crate::trace::{json_str, to_hex, Dir, TraceRecord};

/// Result of decoding one packet.
#[derive(Clone, Debug)]
pub struct Decoded {
    pub ty: u8,
    pub name: &'static str,
    pub dir: Dir,
    /// one line, human readable
    pub summary: String,
    /// compact JSON object, fields per type
    pub json: String,
}

/// Width (in macro columns) assumed when printing block x/y (no session context here).
const DEFAULT_WIDTH: u32 = 512;

/// `BHNDT_*` name minus the prefix, e.g. `"SERVER_INITIAL_CONTACT"`; `"UNKNOWN_xx"` otherwise.
/// Client->server types use the same table.
pub fn type_name(ty: u8) -> &'static str {
    match NetDataType(ty as u32).name() {
        Some(n) => n.strip_prefix("BHNDT_").unwrap_or(n),
        None => unknown_name(ty),
    }
}

fn unknown_name(ty: u8) -> &'static str {
    // 256 static strings generated once.
    use std::sync::OnceLock;
    static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
    let v =
        NAMES.get_or_init(|| (0..256).map(|i| &*Box::leak(format!("UNKNOWN_{:02x}", i).into_boxed_str())).collect());
    v[ty as usize]
}

/// Name of a rejection reason.
pub fn reject_name(r: u8) -> &'static str {
    match r {
        reject::USERNAME_TAKEN => "USERNAME_TAKEN",
        reject::USER_LOGGED_IN => "USER_LOGGED_IN",
        reject::CLIENT_TOO_OLD => "CLIENT_TOO_OLD",
        reject::CLIENT_TOO_NEW => "CLIENT_TOO_NEW",
        reject::SERVER_FULL => "SERVER_FULL",
        reject::BLACKLISTED => "BLACKLISTED",
        reject::NOT_WHITELISTED => "NOT_WHITELISTED",
        reject::DEVICE_ALREADY_IN_USE_WITH_DIFFERENT_NAME => "DEVICE_ALREADY_IN_USE_WITH_DIFFERENT_NAME",
        reject::INCORRECT_CLOUD_KEY => "INCORRECT_CLOUD_KEY",
        reject::INCORRECT_PASSWORD => "INCORRECT_PASSWORD",
        reject::KICKED => "KICKED",
        reject::BANNED => "BANNED",
        reject::NO_CREDIT => "NO_CREDIT",
        reject::INVALID_USERNAME => "INVALID_USERNAME",
        reject::STILL_LOADING => "STILL_LOADING",
        _ => "UNKNOWN",
    }
}

/// Dynamic object type name without the `DYNAMIC_OBJECT_TYPE_` prefix.
pub fn object_type_name(t: u8) -> String {
    match DynamicObjectType(t as u32).name() {
        Some(n) => n.strip_prefix("DYNAMIC_OBJECT_TYPE_").unwrap_or(n).to_string(),
        None => format!("UNKNOWN_{}", t),
    }
}

/// Compact JSON of a plist value (data as `"data:<hex>"`, dates as numbers).
pub fn pv_json(v: &PV) -> String {
    match v {
        PV::String(s) => json_str(s),
        PV::Int(i) => i.to_string(),
        PV::UInt(u) => u.to_string(),
        PV::Real { value, .. } => {
            if value.is_finite() {
                format!("{}", value)
            } else {
                "null".to_string()
            }
        }
        PV::Bool(b) => b.to_string(),
        PV::Date(d) => format!("{}", d),
        PV::Data(d) => {
            if d.bytes.len() <= 64 {
                json_str(&format!("data:{}", to_hex(&d.bytes)))
            } else {
                json_str(&format!("data({}):{}...", d.bytes.len(), to_hex(&d.bytes[..32])))
            }
        }
        PV::Array(a) => format!("[{}]", a.items.iter().map(pv_json).collect::<Vec<_>>().join(",")),
        PV::Dict(d) => {
            let mut kv: Vec<(&String, &PV)> = d.map.iter().collect();
            kv.sort_by(|a, b| a.0.cmp(b.0));
            format!(
                "{{{}}}",
                kv.iter().map(|(k, v)| format!("{}:{}", json_str(k), pv_json(v))).collect::<Vec<_>>().join(",")
            )
        }
        PV::Null => "null".to_string(),
    }
}

fn opt_json(v: &Option<PV>) -> String {
    match v {
        Some(p) => pv_json(p),
        None => "null".to_string(),
    }
}

fn is_scalar(v: &PV) -> bool {
    !matches!(v, PV::Array(_) | PV::Dict(_) | PV::Data(_))
}

fn hex_prefix(b: &[u8], n: usize) -> String {
    let m = b.len().min(n);
    format!("{}{}", to_hex(&b[..m]), if b.len() > m { "..." } else { "" })
}

fn malformed_text(m: &Malformed) -> String {
    match m {
        Malformed::Exception { name, reason } => format!("{name}: {reason}"),
        Malformed::Ub(s) => format!("UB {s}"),
        Malformed::Ignore => "ignored".to_string(),
    }
}

/// Decode a packet in either direction. Never panics; unknown types are `UNKNOWN_xx`.
pub fn decode_packet(dir: Dir, packet: &[u8]) -> Decoded {
    let ty = packet.first().copied().unwrap_or(0);
    let name = type_name(ty);
    let (summary, json) = if packet.is_empty() {
        ("EMPTY".to_string(), "{}".to_string())
    } else {
        guarded(
            || match dir {
                Dir::Rx => decode_rx(packet),
                Dir::Tx => decode_tx(packet),
            },
            packet,
        )
    };
    Decoded { ty, name, dir, summary, json }
}

/// Runs a decoder; a panic inside it becomes a `MALFORMED(decoder panic)` summary.
fn guarded(decode: impl FnOnce() -> (String, String) + std::panic::UnwindSafe, packet: &[u8]) -> (String, String) {
    std::panic::catch_unwind(decode)
        .unwrap_or_else(|_| (format!("MALFORMED(decoder panic) {}", hex_prefix(packet, 16)), "{}".to_string()))
}

fn malformed(m: &Malformed, packet: &[u8]) -> (String, String) {
    (
        format!("MALFORMED({}) {}", malformed_text(m), hex_prefix(packet, 24)),
        format!("{{\"malformed\":{},\"hex\":\"{}\"}}", json_str(&malformed_text(m)), hex_prefix(packet, 24)),
    )
}

fn decode_rx(packet: &[u8]) -> (String, String) {
    let ty = packet[0];
    let msg = match decode_server_packet(packet) {
        Ok(m) => m,
        Err(e) => return malformed(&e, packet),
    };
    match msg {
        ServerMsg::Contact { version, plist } => (
            format!("version={} plist={}", version, opt_json(&plist)),
            format!("{{\"version\":{},\"plist\":{}}}", version, opt_json(&plist)),
        ),
        ServerMsg::Rejection { reason, extra } => {
            (format!("reason={} ({}) extra={}", reason, reject_name(reason), opt_json(&extra)), format!("{{\"reason\":{},\"name\":\"{}\",\"extra\":{}}}", reason, reject_name(reason), opt_json(&extra)))
        }
        ServerMsg::Summary(p) => {
            let mut keys = Vec::new();
            if let Some(PV::Dict(d)) = &p {
                let mut kv: Vec<(&String, &PV)> = d.map.iter().collect();
                kv.sort_by(|a, b| a.0.cmp(b.0));
                for (k, v) in kv {
                    if is_scalar(v) {
                        keys.push(format!("{}={}", k, pv_json(v)));
                    } else {
                        keys.push(k.clone());
                    }
                }
            }
            let kj = keys.iter().map(|k| json_str(k)).collect::<Vec<_>>().join(",");
            (format!("keys=[{}]", keys.join(" ")), format!("{{\"keys\":[{}]}}", kj))
        }
        ServerMsg::PlayerList(p) | ServerMsg::Chat(p) | ServerMsg::ChatHistory(p) | ServerMsg::Welcome(p) | ServerMsg::OwnershipSignPlayerList(p) => {
            let key = if ty == 0x24 { "dict" } else { "plist" };
            (opt_json(&p), format!("{{\"{}\":{}}}", key, opt_json(&p)))
        }
        ServerMsg::OwnershipSignModified(p) | ServerMsg::PriceOffsets(p) => (opt_json(&p), format!("{{\"plist\":{}}}", opt_json(&p))),
        ServerMsg::Heartbeat(h) => (
            format!(
                "worldTime={} noRainTimer={} fastForward={} localPaused={} allPaused={} pvpDisabled={} credit={}",
                h.world_time, h.no_rain_timer, h.fast_forward, h.local_paused, h.all_paused, h.pvp_disabled, h.credit
            ),
            format!(
                "{{\"worldTime\":{},\"noRainTimer\":{},\"fastForward\":{},\"localPaused\":{},\"allPaused\":{},\"pvpDisabled\":{},\"credit\":{}}}",
                jf(h.world_time),
                jf(h.no_rain_timer),
                h.fast_forward,
                h.local_paused,
                h.all_paused,
                h.pvp_disabled,
                jf(h.credit)
            ),
        ),
        ServerMsg::InitialData { food_types, plist } => (
            format!("food={:?} plist={}", &food_types[..4], opt_json(&plist)),
            format!("{{\"food\":{:?},\"plist\":{}}}", food_types, opt_json(&plist)),
        ),
        ServerMsg::UniqueId(id) => (format!("id={:#x}", id), format!("{{\"id\":{}}}", id)),
        ServerMsg::PickupReply { accepted, requester, ids } => (
            format!("accepted={} requester={:#x} ids={:?}", accepted, requester, ids),
            format!("{{\"accepted\":{},\"requester\":{},\"ids\":{:?}}}", accepted, requester, ids),
        ),
        ServerMsg::ZoomResponse(p) => match p {
            Some((x, y)) => (format!("pos=({},{})", x, y), format!("{{\"x\":{},\"y\":{}}}", x, y)),
            None => ("none".to_string(), "{}".to_string()),
        },
        ServerMsg::Block(b) => {
            let w = DEFAULT_WIDTH;
            let (x, y) = (b.macro_index % w, b.macro_index / w);
            let tiles_md5 = &data_string_from_md5(&b.tiles)[..8];
            let light_md5 = &data_string_from_md5(&b.light)[..8];
            let (hash, receipts, extra_keys) = block_extra(&b.extra);
            (
                format!(
                    "idx={} x={} y={} ver={} upd={} tiles={} light={} hash={} receipts={:?} extra={:?}",
                    b.macro_index, x, y, b.version, b.update_index, tiles_md5, light_md5, hash, receipts, extra_keys
                ),
                format!(
                    "{{\"idx\":{},\"ver\":{},\"upd\":{},\"hash\":{},\"receipts\":{:?}}}",
                    b.macro_index,
                    b.version,
                    b.update_index,
                    json_str(&hash),
                    receipts
                ),
            )
        }
        ServerMsg::BlockNotLoaded { index, index32 } => (format!("index={} index32={}", index, index32), format!("{{\"index\":{},\"index32\":{}}}", index, index32)),
        ServerMsg::LightBlock(l) => (
            format!("idx={} upd={}", l.macro_index, l.update_index),
            format!("{{\"idx\":{},\"upd\":{}}}", l.macro_index, l.update_index),
        ),
        ServerMsg::Batch(b) => {
            let ids: Vec<u64> = b.items.iter().filter_map(|x| blockheads_server::protocol::object_batches::blob_unique_id(x)).collect();
            let first = b.items.first().map(|x| hex_prefix(x, 16)).unwrap_or_default();
            (
                format!("objType={}({}) count={} ids={:?} first16={}", b.object_type, object_type_name(b.object_type), b.items.len(), ids, first),
                format!("{{\"kind\":{},\"objType\":{},\"count\":{},\"ids\":{:?}}}", b.kind, b.object_type, b.items.len(), ids),
            )
        }
        ServerMsg::AdminOrModChanged { x, y } => (format!("({},{})", x, y), format!("{{\"x\":{},\"y\":{}}}", x, y)),
        ServerMsg::PaintingData { id, image } => (format!("id={} image={}B", id, image.len()), format!("{{\"id\":{},\"len\":{}}}", id, image.len())),
        ServerMsg::ChestInventory { id, inventory } => (format!("id={} inv={}B", id, inventory.len()), format!("{{\"id\":{},\"len\":{}}}", id, inventory.len())),
        ServerMsg::PortalChestAck { raw8, id } | ServerMsg::PortalChestNotSaved { raw8, id } => {
            (format!("id={} raw={}", id, to_hex(&raw8)), format!("{{\"id\":{},\"raw\":\"{}\"}}", id, to_hex(&raw8)))
        }
        ServerMsg::ClearChat => (String::new(), "{}".to_string()),
        ServerMsg::ToggleRepairMode(b) => (format!("enabled={}", b), format!("{{\"enabled\":{}}}", b)),
        ServerMsg::Gather(g) => (
            format!("pos=({},{}) progress={}", g.pos_x, g.pos_y, g.gather_progress),
            format!("{{\"x\":{},\"y\":{},\"progress\":{}}}", g.pos_x, g.pos_y, g.gather_progress),
        ),
        ServerMsg::Projectile(p) => (
            format!("from=({},{}) to=({},{}) item={}", p.from_pos_x, p.from_pos_y, p.to_pos_x, p.to_pos_y, p.item_type),
            format!("{{\"item\":{}}}", p.item_type),
        ),
        ServerMsg::Damage(_) => (hex_prefix(&packet[1..], 24), "{}".to_string()),
        ServerMsg::ElectricityPath { header: _, path } => (format!("path={} entries", path.len()), format!("{{\"entries\":{}}}", path.len())),
        ServerMsg::Tip(s) => (json_str(&s), format!("{{\"text\":{}}}", json_str(&s))),
        ServerMsg::Unknown { ty, payload } => {
            // Block-like types that failed to decode are flagged; everything else is shown as raw bytes.
            let bad = matches!(ty, 0x04 | 0x27 | 0x28);
            (
                format!("{}{}B {}", if bad { "MALFORMED " } else { "" }, payload.len(), hex_prefix(&payload, 24)),
                format!("{{\"len\":{},\"hex\":\"{}\"}}", payload.len(), hex_prefix(&payload, 24)),
            )
        }
    }
}

fn jf(f: f32) -> String {
    if f.is_finite() {
        format!("{}", f)
    } else {
        "null".to_string()
    }
}

fn block_extra(extra: &Option<PV>) -> (String, Vec<u32>, Vec<String>) {
    let mut hash = String::new();
    let mut receipts = Vec::new();
    let mut keys = Vec::new();
    if let Some(PV::Dict(d)) = extra {
        let mut ks: Vec<&String> = d.map.iter().map(|(k, _)| k).collect();
        ks.sort();
        for k in ks {
            match k.as_str() {
                "hash" => hash = d.map.get(k).and_then(|v| v.as_str().map(|s| s.to_string())).unwrap_or_default(),
                "receipts" => {
                    if let Some(PV::Array(a)) = d.map.get(k) {
                        receipts = a.items.iter().map(|v| v.unsigned_int_value()).collect();
                    }
                }
                other => keys.push(other.to_string()),
            }
        }
    }
    (hash, receipts, keys)
}

fn decode_tx(packet: &[u8]) -> (String, String) {
    let ty = packet[0];
    let body = &packet[1..];
    let msg = match decode_client_packet(packet) {
        Ok(m) => m,
        Err(e) => return malformed(&e, packet),
    };
    let raw = || {
        (
            format!("{}B {}", body.len(), hex_prefix(body, 24)),
            format!("{{\"len\":{},\"hex\":\"{}\"}}", body.len(), hex_prefix(body, 24)),
        )
    };
    match msg {
        Some(ClientMsg::BlockRequest(r)) => (
            format!("idx={} create={}", r.macro_index, r.create_if_not_created),
            format!("{{\"idx\":{},\"create\":{}}}", r.macro_index, r.create_if_not_created),
        ),
        Some(ClientMsg::BlockRemovalNotification { macro_index }) => {
            (format!("idx={}", macro_index), format!("{{\"idx\":{}}}", macro_index))
        }
        Some(ClientMsg::ObjectCreate { ty: ot, items })
        | Some(ClientMsg::ObjectUpdate { ty: ot, items })
        | Some(ClientMsg::ObjectRemove { ty: ot, items })
        | Some(ClientMsg::ObjectCreationDataUpdate { ty: ot, items }) => {
            let n = match &items {
                Some(PV::Array(a)) => a.items.len(),
                _ => 0,
            };
            (
                format!("objType={}({}) count={}", ot, object_type_name(ot), n),
                format!("{{\"objType\":{},\"count\":{}}}", ot, n),
            )
        }
        Some(ClientMsg::PickupRequest { requester, ids }) => (
            format!("requester={:#x} ids={:?}", requester, ids),
            format!("{{\"requester\":{},\"ids\":{:?}}}", requester, ids),
        ),
        Some(ClientMsg::Chat { dict }) => (opt_json(&dict), format!("{{\"dict\":{}}}", opt_json(&dict))),
        Some(ClientMsg::ZoomRequest { player_id, cycle_index }) => (
            format!("playerID={:?} cycle={}", player_id, cycle_index),
            format!(
                "{{\"playerID\":{},\"cycle\":{}}}",
                player_id.as_deref().map(json_str).unwrap_or("null".into()),
                cycle_index
            ),
        ),
        Some(ClientMsg::ClientBlockheadInventory { blockhead_id, data }) => (
            format!("blockhead={:#x} data={}B", blockhead_id, data.len()),
            format!("{{\"blockhead\":{},\"len\":{}}}", blockhead_id, data.len()),
        ),
        Some(ClientMsg::TradeTransaction { dict })
        | Some(ClientMsg::WelcomeMessageSet { dict })
        | Some(ClientMsg::ReportUser { dict }) => (opt_json(&dict), format!("{{\"dict\":{}}}", opt_json(&dict))),
        Some(ClientMsg::PortalChestTransaction { id, .. })
        | Some(ClientMsg::PortalChestTransactionResend { id, .. }) => {
            (format!("id={}", id), format!("{{\"id\":{}}}", id))
        }
        Some(ClientMsg::Tip { payload, .. }) => {
            let s = String::from_utf8_lossy(&payload).into_owned();
            (json_str(&s), format!("{{\"text\":{}}}", json_str(&s)))
        }
        Some(ClientMsg::PlayerInformation { payload }) => {
            let p = blockheads_server::protocol::codec::plist_from(&payload);
            (opt_json(&p), format!("{{\"plist\":{}}}", opt_json(&p)))
        }
        _ => {
            let _ = ty;
            raw()
        }
    }
}

/// One line for a trace record, e.g.
/// `" 1234ms RX ch0 R 0x04 SERVER_BLOCK 888B idx=8646 ver=0..."`.
pub fn pretty(r: &TraceRecord) -> String {
    let d = decode_packet(r.dir, &r.data);
    format!(
        "{:>6}ms {} ch{} {} 0x{:02x} {} {}B  {}",
        r.t_ms,
        if r.dir == Dir::Rx { "RX" } else { "TX" },
        r.channel,
        if r.reliable { "R" } else { "U" },
        r.ty,
        d.name,
        r.len,
        d.summary
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use blockheads_server::protocol::codec::t;

    #[test]
    fn names() {
        assert_eq!(type_name(0x04), "SERVER_BLOCK");
        assert_eq!(type_name(t::SERVER_INITIAL_CONTACT), "SERVER_INITIAL_CONTACT");
        assert_eq!(type_name(0xee), "UNKNOWN_ee");
        for ty in 0..=76u8 {
            let n = NetDataType(ty as u32).name().unwrap();
            assert_eq!(format!("BHNDT_{}", type_name(ty)), n);
        }
    }

    #[test]
    fn never_panics_on_garbage() {
        for ty in 0..=255u8 {
            for body in [&[][..], &[1u8][..], &[0xff; 9][..], &[0u8; 40][..]] {
                let mut p = vec![ty];
                p.extend_from_slice(body);
                let _ = decode_packet(Dir::Rx, &p);
                let _ = decode_packet(Dir::Tx, &p);
            }
        }
    }

    use blockheads_server::foundation::gsdict::GsDict;
    use blockheads_server::protocol::block_wire::{self, ExtraData, LIGHT_LEN, TILES_LEN};
    use blockheads_server::protocol::messages as m;
    use blockheads_server::protocol::object_batches::{encode_batch, encode_client_batch, BatchKind, ClientBatchKind};

    fn dict(pairs: Vec<(&str, PV)>) -> PV {
        let mut g = GsDict::new();
        for (k, v) in pairs {
            g.insert(k.to_string(), v);
        }
        PV::dict(g)
    }
    fn rx(p: &[u8]) -> Decoded {
        decode_packet(Dir::Rx, p)
    }
    fn tx(p: &[u8]) -> Decoded {
        decode_packet(Dir::Tx, p)
    }

    #[test]
    fn small_helpers() {
        assert_eq!(reject_name(reject::USER_LOGGED_IN), "USER_LOGGED_IN");
        for code in 0..=16u8 {
            assert!(!reject_name(code).is_empty());
        }
        let mut all: Vec<&str> = (0..=15u8).map(reject_name).collect();
        all.sort();
        all.dedup();
        assert!(all.len() >= 14, "{all:?}");
        assert_eq!(reject_name(200), "UNKNOWN");
        assert_eq!(
            object_type_name(0),
            DynamicObjectType(0).name().unwrap().strip_prefix("DYNAMIC_OBJECT_TYPE_").unwrap()
        );
        assert_eq!(object_type_name(250), "UNKNOWN_250");
        assert_eq!(jf(1.5), "1.5");
        assert_eq!(jf(f32::NAN), "null");
        assert_eq!(hex_prefix(&[1, 2, 3], 2), "0102...");
        assert_eq!(hex_prefix(&[1, 2], 5), "0102");
        assert_eq!(malformed_text(&Malformed::Ub("x")), "UB x");
        assert_eq!(malformed_text(&Malformed::Ignore), "ignored");
        assert_eq!(malformed_text(&Malformed::malloc()), "NSMallocException: Default zone has run out of memory");
        let (s, j) = guarded(|| panic!("decoder bug"), &[1, 2, 3]);
        assert_eq!((s.as_str(), j.as_str()), ("MALFORMED(decoder panic) 010203", "{}"));
        assert_eq!(rx(&[]).summary, "EMPTY");
        assert_eq!(tx(&[]).json, "{}");
    }

    #[test]
    fn plist_values_become_json() {
        let v = dict(vec![
            ("s", PV::string("a\"b")),
            ("i", PV::int(-1)),
            ("u", PV::UInt(u64::MAX)),
            ("f", PV::real_f64(1.5)),
            ("inf", PV::real_f64(f64::INFINITY)),
            ("b", PV::Bool(true)),
            ("d", PV::date(2.0)),
            ("small", PV::data(vec![1, 2])),
            ("big", PV::data(vec![7; 100])),
            ("a", PV::array(vec![PV::Null, PV::int(2)])),
        ]);
        let j = pv_json(&v);
        assert!(j.starts_with("{\"a\":[null,2],\"b\":true,\"big\":\"data(100):"), "{j}");
        assert!(j.contains("\"d\":2") && j.contains("\"f\":1.5") && j.contains("\"inf\":null"));
        assert!(
            j.contains("\"small\":\"data:0102\"")
                && j.contains("\"s\":\"a\\\"b\"")
                && j.contains("\"u\":18446744073709551615")
        );
        assert_eq!(opt_json(&None), "null");
        assert!(is_scalar(&PV::int(1)) && !is_scalar(&PV::array(vec![])) && !is_scalar(&PV::data(vec![])));
    }

    #[test]
    fn server_packets_are_summarised() {
        let tiles = [1u8; TILES_LEN];
        let light = [2u8; LIGHT_LEN];
        let extra = ExtraData { hash: "abc".into(), receipts: Some(vec![4, 5]) };
        let block = block_wire::server_block_from_parts(1030, &tiles, 3, 9, Some((&light, &extra))).unwrap();
        let d = rx(&block);
        assert!(d.summary.starts_with("idx=1030 x=6 y=2 ver=3 upd=9 tiles="), "{}", d.summary);
        assert!(d.summary.contains("hash=abc receipts=[4, 5] extra=[]") && d.json.contains("\"receipts\":[4, 5]"));
        let mut weird = GsDict::new();
        weird.insert("hash".to_string(), PV::string("h"));
        weird.insert("other".to_string(), PV::int(1));
        weird.insert("receipts".to_string(), PV::int(3));
        assert_eq!(block_extra(&Some(PV::dict(weird))), ("h".to_string(), vec![], vec!["other".to_string()]));
        assert_eq!(block_extra(&None), (String::new(), vec![], vec![]));
        assert!(rx(&[0x04, 1, 2]).summary.starts_with("MALFORMED 2B"));
        assert!(rx(&block_wire::block_not_loaded(7)).summary.contains("index=7 index32=7"));
        assert!(rx(&[0x27, 1]).summary.starts_with("MALFORMED 1B"));
        assert_eq!(rx(&block_wire::light_block(3, &light, 4)).summary, "idx=3 upd=4");
        let batch = rx(&encode_batch(BatchKind::Create, 14, &[vec![1, 0, 0, 0, 0, 0, 0, 0, 9], vec![2; 8]]).unwrap());
        assert!(batch.summary.contains("count=2 ids=[1, 144680345676153346]") && batch.json.contains("\"kind\":7"));
        assert!(rx(&encode_batch(BatchKind::Remove, 14, &[]).unwrap()).summary.contains("count=0 ids=[] first16="));
        let contact = rx(&m::initial_contact(Some("Own"), false, "w"));
        assert!(contact.summary.starts_with("version=38 plist={") && contact.json.starts_with("{\"version\":38"));
        let rej = rx(&m::rejection(reject::KICKED));
        assert!(rej.summary.contains("reason=11 (KICKED) extra=null"));
        let summary = rx(&m::world_summary(&m::WorldSummary {
            world_name: "N".into(),
            welcome_message: Some("w".into()),
            custom_rules: Some(dict(vec![])),
            ..Default::default()
        }));
        assert!(
            summary.summary.starts_with("keys=[")
                && summary.summary.contains("worldName=\"N\"")
                && summary.summary.contains(" customRules")
        );
        assert_eq!(rx(&[0x01]).summary, "keys=[]");
        assert!(rx(&m::player_list(&[])).summary.contains("[]"));
        let chat = rx(&m::chat_message(&m::server_chat_dict("hi", "A", "p", 1.0)));
        assert!(chat.json.starts_with("{\"dict\":") && chat.summary.contains("\"message\":\"hi\""));
        assert!(rx(&m::chat_history(&PV::array(vec![]), &[])).json.starts_with("{\"plist\":"));
        assert!(rx(&m::welcome_message(None, None)).json.starts_with("{\"plist\":"));
        assert!(rx(&m::ownership_sign_player_list(&PV::array(vec![]))).json.starts_with("{\"plist\":"));
        assert!(rx(&m::ownership_sign_modified(&dict(vec![]))).json.starts_with("{\"plist\":"));
        assert!(rx(&m::price_offsets(&dict(vec![]))).json.starts_with("{\"plist\":"));
        let hb = rx(&m::heartbeat(&m::WorldHeartbeat::default()));
        assert!(hb.summary.starts_with("worldTime=") && hb.json.contains("\"credit\":"));
        let init = rx(&m::initial_data_reply(&[0; 31], None));
        assert!(init.summary.starts_with("food=[0, 0, 0, 0] plist=null"));
        assert_eq!(rx(&m::unique_id_reply(255)).summary, "id=0xff");
        assert!(rx(&m::pickup_reply(true, 16, &[1])).summary.contains("accepted=true requester=0x10 ids=[1]"));
        assert_eq!(rx(&m::zoom_response(Some((1, 2)))).summary, "pos=(1,2)");
        assert_eq!(rx(&m::zoom_response(None)).summary, "none");
        assert_eq!(rx(&m::admin_or_mod_changed(1, 0)).summary, "(1,0)");
        assert!(rx(&m::painting_data(5, &[1, 2])).summary.contains("id=5 image=2B"));
        assert!(rx(&m::chest_inventory(5, None)).summary.contains("inv=0B"));
        assert!(rx(&m::portal_chest_ack(2)).summary.starts_with("id=2 raw=02"));
        assert!(rx(&m::portal_chest_not_saved(&[3, 0, 0, 0, 0, 0, 0, 0])).json.contains("\"id\":3"));
        assert_eq!(rx(&m::clear_chat()).json, "{}");
        assert_eq!(rx(&m::toggle_repair_mode(true)).summary, "enabled=true");
        assert!(rx(&m::gather_notification(&m::GatherNetRequest::default())).summary.starts_with("pos=("));
        assert!(rx(&m::projectile(&m::ProjectileNetRequest::default())).summary.starts_with("from=("));
        assert!(rx(&m::blockhead_damage(&m::BlockheadDamageNetRequest::default())).json == "{}");
        let el = rx(&m::electricity_path(1, &[m::ElectricityPathIndex::default()]).unwrap());
        assert_eq!(el.summary, "path=1 entries");
        assert_eq!(rx(&m::tip("hello")).json, "{\"text\":\"hello\"}");
        let unknown = rx(&[0xee, 1, 2]);
        assert_eq!((unknown.summary.as_str(), unknown.name), ("2B 0102", "UNKNOWN_ee"));
    }

    #[test]
    fn client_packets_are_summarised() {
        let d = dict(vec![("k", PV::int(1))]);
        assert_eq!(
            tx(&block_wire::block_request(m::ClientMacroBlockRequest {
                macro_index: 3,
                create_if_not_created: 1,
                ..Default::default()
            }))
            .summary,
            "idx=3 create=1"
        );
        assert_eq!(tx(&block_wire::block_removal_notification(8)).summary, "idx=8");
        for kind in [
            ClientBatchKind::Create,
            ClientBatchKind::Update,
            ClientBatchKind::Remove,
            ClientBatchKind::CreationDataUpdate,
        ] {
            let s = tx(&encode_client_batch(kind, 14, &[vec![0; 9]]).unwrap()).summary;
            assert!(s.ends_with("count=1"), "{s}");
        }
        assert!(tx(&[0x0a, 14]).summary.ends_with("count=0"));
        assert!(tx(&m::pickup_request(255, &[1])).summary.contains("requester=0xff ids=[1]"));
        assert!(tx(&m::client_chat(&d)).json.starts_with("{\"dict\":{"));
        assert_eq!(tx(&[0x24]).summary, "null");
        assert!(tx(&m::zoom_request("pid", 2)).summary.contains("playerID=Some(\"pid\") cycle=2"));
        assert!(tx(&[0x1b]).json.contains("\"playerID\":null"));
        let mut inv = vec![0x21];
        inv.extend_from_slice(&16u64.to_le_bytes());
        inv.extend_from_slice(b"zz");
        assert_eq!(tx(&inv).summary, "blockhead=0x10 data=2B");
        for ty in [t::CLIENT_TRADE_TRANSACTION, t::CLIENT_WELCOME_MESSAGE_SET_REQUEST, t::CLIENT_REPORT_USER] {
            assert!(tx(&m::client_gz_plist(ty, &d)).summary.starts_with('{'), "{ty:#x}");
        }
        let mut chest = vec![0x3e];
        chest.extend_from_slice(&[9, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(tx(&chest).summary, "id=9");
        chest[0] = 0x40;
        assert_eq!(tx(&chest).json, "{\"id\":9}");
        assert_eq!(tx(&[0x2d, b'h', b'i']).json, "{\"text\":\"hi\"}");
        assert!(tx(&m::client_player_information(&d)).json.starts_with("{\"plist\":"));
        assert!(tx(&[0x10, 1, 2]).summary.starts_with("2B 0102"));
        assert!(tx(&[0x12]).summary.starts_with("MALFORMED(NSRangeException"));
        assert!(tx(&[0xee]).summary.starts_with("0B"));
    }

    #[test]
    fn records_print_on_one_line() {
        let rec = |dir, reliable| TraceRecord {
            seq: 0,
            t_ms: 12,
            dir,
            client: "A".into(),
            channel: 1,
            reliable,
            ty: 0x2d,
            len: 3,
            data: vec![0x2d, b'h', b'i'],
        };
        let rxl = pretty(&rec(Dir::Rx, true));
        assert!(rxl.contains("RX ch1 R 0x2d TIP_NOTIFICATION 3B") && rxl.ends_with("\"hi\""), "{rxl}");
        assert!(pretty(&rec(Dir::Tx, false)).contains("TX ch1 U 0x2d"));
    }
}

#[cfg(test)]
#[path = "decode_golden.rs"]
mod golden;
