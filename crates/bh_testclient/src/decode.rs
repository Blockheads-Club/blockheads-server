//! Human-readable and JSON decoders of server->client and client->server packets.
//!
//! Built on `protocol::messages::{decode_server_packet, decode_client_packet}`; never panics.

use blockheads_server::foundation::md5::data_string_from_md5;
use blockheads_server::plist::value::PlistValue as PV;
use blockheads_server::protocol::block_wire::ExtraData;
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
        let r = std::panic::catch_unwind(|| match dir {
            Dir::Rx => decode_rx(packet),
            Dir::Tx => decode_tx(packet),
        });
        r.unwrap_or_else(|_| (format!("MALFORMED(decoder panic) {}", hex_prefix(packet, 16)), "{}".to_string()))
    };
    Decoded { ty, name, dir, summary, json }
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

#[allow(dead_code)]
fn _uses(_: ExtraData) {}

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

    fn vectors_dir() -> Option<std::path::PathBuf> {
        let p = crate::harness::private_docs().join("protocol_vectors");
        p.is_dir().then_some(p)
    }

    /// U2: every captured server packet decodes without MALFORMED; the solo 0x04 vector prints the known hash.
    #[test]
    fn u2_vectors() {
        let Some(dir) = vectors_dir() else { return };
        let mut n = 0;
        for sub in ["solo", "two_clients"] {
            let Ok(rd) = std::fs::read_dir(dir.join(sub)) else { continue };
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if !name.ends_with(".bin") || name.starts_with("A_") && false {
                    continue;
                }
                let data = std::fs::read(e.path()).unwrap();
                let d = decode_packet(Dir::Rx, &data);
                assert!(!d.summary.contains("MALFORMED"), "{}: {}", name, d.summary);
                if sub == "solo" && name.starts_with("007_") {
                    assert!(d.summary.contains("idx=0 "), "{}", d.summary);
                    assert!(d.summary.contains("ver=0 upd=0"), "{}", d.summary);
                    assert!(d.summary.contains("hash=1128c17e53085db291e4b0d5393d0420"), "{}", d.summary);
                }
                n += 1;
            }
        }
        assert!(n > 0);
    }
}
