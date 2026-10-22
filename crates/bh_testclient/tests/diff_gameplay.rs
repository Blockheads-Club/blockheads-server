//! Differential gameplay harness: the same scripted client session against the reference server (refenv,
//! docker) and against ours, both started from the same `--new` world. Each run records
//!   * the full rx/tx trace of every client (JSONL),
//!   * semantic observations (chest inventory seen on open/reopen, chest updates, created objects, fill
//!     replies, blocks + light blocks received while walking, per-type packet counts per phase),
//!   * the world save after a SIGINT stop.
//! Observations are diffed as sets of lines; the saves with `diff_saves`.
//!
//! Needs docker, image `blockheads-ref:1.7.1`, `BH_REFENV` and `BH_PARITY=1`; skipped otherwise.
//! `BH_DIFF_OUT=<dir>` keeps every artifact (default: a temp dir, deleted).
//! `BH_DIFF_ONLY=ours|ref` runs one side only (debugging the script). `BH_DIFF_WALK=<tiles>` walk length.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bh_testclient::harness::refenv::*;
use bh_testclient::harness::save_diff::*;
use bh_testclient::harness::*;
use bh_testclient::*;
use blockheads_server::foundation::gsdict::GsDict;
use blockheads_server::plist::value::PlistValue as PV;
use blockheads_server::protocol::block_wire::{decode_light_block, decode_server_block, BlockMsg};
use blockheads_server::protocol::codec::{gz_plist_from, plist_gz};
use blockheads_server::protocol::object_batches::{decode_batch, ClientBatchKind as BatchKind};

const WORLD: &str = "e98c5bee38be74e00ad7748a3dd42119";
const ITEM_STICK: u16 = 4;
const ITEM_CHEST: u16 = 1043;
const ITEM_ROCK: u16 = 1047;
const ITEM_DIRT: u16 = 1048;
const T_BLOCKHEAD: u8 = 24;
const T_WORKBENCH: u8 = 45;
const T_CHEST: u8 = 46;

fn server_args() -> Vec<String> {
    [
        "--new",
        "PROTO",
        "--world_id",
        WORLD,
        "--world_width",
        "1/4",
        "--custom-rules",
        "{\"seed\":\"custom\",\"custom_seed_value\":\"12345\"}",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

// ------------------------------------------------------------------------------------------- helpers

fn fnv(b: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for x in b {
        h ^= *x as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h)
}

/// `InventoryItem saveData` header only: `{u16 type, u16 dataA, u16 dataB, u8 selected, u8 0}`.
fn item(t: u16) -> Vec<u8> {
    let mut v = Vec::with_capacity(8);
    v.extend_from_slice(&t.to_le_bytes());
    v.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    v
}

/// `n` slots; `fill = [(slot, item type, count)]`.
fn slots(n: usize, fill: &[(usize, u16, usize)]) -> PV {
    let mut out = Vec::new();
    for i in 0..n {
        let mut s = Vec::new();
        for (si, t, c) in fill {
            if *si == i {
                for _ in 0..*c {
                    s.push(PV::data(item(*t)));
                }
            }
        }
        out.push(PV::array(s));
    }
    PV::array(out)
}

fn d(pairs: Vec<(&str, PV)>) -> PV {
    let mut m: GsDict<String, PV> = GsDict::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), v);
    }
    PV::dict(m)
}

/// `s<slot>:<type>x<count>` per non-empty slot of a `saveItemSlots` array (or a dict holding one).
fn slot_summary(v: &PV) -> String {
    let arr = match v {
        PV::Dict(_) => match v.object_for_key("saveItemSlots") {
            Some(a) => a.clone(),
            None => return "no-saveItemSlots".into(),
        },
        other => other.clone(),
    };
    let PV::Array(a) = arr else { return "not-array".into() };
    let mut parts = Vec::new();
    for (i, s) in a.items.iter().enumerate() {
        let PV::Array(sa) = s else { continue };
        let mut by: BTreeMap<u16, usize> = BTreeMap::new();
        for e in &sa.items {
            if let PV::Data(b) = e {
                let t = u16::from_le_bytes([b.bytes[0], *b.bytes.get(1).unwrap_or(&0)]);
                *by.entry(t).or_default() += 1;
            }
        }
        for (t, c) in by {
            parts.push(format!("s{}:{}x{}", i, t, c));
        }
    }
    if parts.is_empty() {
        format!("empty({} slots)", a.items.len())
    } else {
        parts.join(",")
    }
}

/// Client chest update: the server's ChestNetData header with the in-use fields patched, + gzip plist.
fn chest_blob(base: &[u8], in_use: bool, bh: u64, slots_pv: Option<PV>, owner: &str) -> Vec<u8> {
    let mut b = base[..0x40.min(base.len())].to_vec();
    b.resize(0x40, 0);
    b[0x18..0x20].copy_from_slice(&(if in_use { bh } else { 0 }).to_le_bytes());
    b[0x22] = in_use as u8;
    let mut pairs = vec![("ownerID", PV::string(owner))];
    if let Some(s) = slots_pv {
        pairs.push(("saveItemSlots", s));
    }
    b.extend(plist_gz(&d(pairs)).unwrap_or_default());
    b
}

/// `BlockheadUpdateData` (0x70) for one adjacent step `from -> to`.
fn bh_update(uid: u64, from: (i32, i32), to: (i32, i32)) -> Vec<u8> {
    let mut b = vec![0u8; 0x70];
    b[0..8].copy_from_slice(&uid.to_le_bytes());
    for (off, v) in [(0x08, from.0), (0x0c, from.1), (0x18, from.0), (0x1c, from.1), (0x20, to.0), (0x24, to.1)] {
        b[off..off + 4].copy_from_slice(&(v as u32).to_le_bytes());
    }
    b[0x33] = 100; // happiness
    b[0x3c] = 1; // isCurrentlyActive
    b[0x3f] = 127; // health
    b
}

fn tile(bm: &BlockMsg, x: i32, y: i32) -> &[u8] {
    let i = ((y & 31) * 32 + (x & 31)) as usize * 64;
    &bm.tiles[i..i + 64]
}

#[derive(Default)]
struct Obs {
    lines: Vec<String>,
}
impl Obs {
    fn add(&mut self, s: impl Into<String>) {
        let s = s.into();
        eprintln!("  obs: {}", s);
        self.lines.push(s);
    }
}

/// Summarise every packet received since the last drain into observations.
fn absorb(s: &mut Session, obs: &mut Obs, phase: &str) {
    let mut counts: BTreeMap<u8, usize> = BTreeMap::new();
    for rx in s.drain() {
        let ty = rx.data[0];
        *counts.entry(ty).or_default() += 1;
        match ty {
            0x32 => {
                let id = u64::from_le_bytes(rx.data[1..9].try_into().unwrap());
                let inv = if rx.data.len() > 9 {
                    gz_plist_from(&rx.data[9..]).map(|v| slot_summary(&v)).unwrap_or_else(|| "undecodable".into())
                } else {
                    "none".into()
                };
                obs.add(format!("{} CHEST_INVENTORY_DATA id={} inv={}", phase, id, inv));
            }
            0x07 | 0x08 | 0x09 => {
                let Ok(Some(b)) = decode_batch(&rx.data) else { continue };
                for blob in &b.items {
                    if blob.len() < 16 {
                        continue;
                    }
                    let uid = u64::from_le_bytes(blob[0..8].try_into().unwrap());
                    let x = u32::from_le_bytes(blob[8..12].try_into().unwrap());
                    let y = u32::from_le_bytes(blob[12..16].try_into().unwrap());
                    if b.object_type == T_CHEST {
                        let in_use = blob.get(0x22).copied().unwrap_or(0);
                        let inv = if blob.len() > 0x40 {
                            gz_plist_from(&blob[0x40..]).map(|v| slot_summary(&v)).unwrap_or_default()
                        } else {
                            String::new()
                        };
                        obs.add(format!(
                            "{} CHEST {:02x} id={} pos={},{} inUse={} {}",
                            phase, ty, uid, x, y, in_use, inv
                        ));
                    } else if b.object_type == T_WORKBENCH {
                        let in_use = blob.get(0x22).copied().unwrap_or(0);
                        obs.add(format!("{} WORKBENCH {:02x} id={} pos={},{} inUse={}", phase, ty, uid, x, y, in_use));
                    } else if ty != 0x08 && b.object_type != T_BLOCKHEAD {
                        obs.add(format!("{} OBJ {:02x} type={} id={} pos={},{}", phase, ty, b.object_type, uid, x, y));
                    }
                }
            }
            0x1d => obs.add(format!("{} FILL_REPLY {}", phase, to_hex(&rx.data[1..]))),
            0x13 => obs.add(format!("{} PICKUP_REPLY {}", phase, to_hex(&rx.data[1..]))),
            _ => {}
        }
    }
    let mut line = format!("{} counts", phase);
    for (t, c) in counts {
        if matches!(t, 0x17 | 0x08 | 0x2c) {
            continue; // heartbeat / object update / electricity cadence is wall-clock dependent
        }
        let _ = write!(line, " {:02x}:{}", t, c);
    }
    obs.add(line);
}

/// Request every macro block in a box around tile (x, y), throttled like the real client.
fn request_around(s: &mut Session, x: i32, y: i32, rx: i32, ry: i32) {
    let mut idx = Vec::new();
    for my in ((y >> 5) - ry).max(0)..=((y >> 5) + ry) {
        for mx in (x >> 5) - rx..=(x >> 5) + rx {
            let i = s.macro_index_for_tile(mx * 32, my * 32);
            if !idx.contains(&i) {
                idx.push(i);
            }
        }
    }
    let _ = s.request_blocks_throttled(&idx, true, 20000);
}

/// Per macro block: latest light block (0x28) and latest block (0x04) fingerprints incl. fog-of-war bytes.
fn light_and_block_summary(s: &Session, obs: &mut Obs, phase: &str) {
    let mut light: BTreeMap<u32, (usize, String, usize)> = BTreeMap::new();
    let mut blocks: BTreeMap<u32, (usize, String)> = BTreeMap::new();
    for rx in s.received() {
        match rx.data[0] {
            0x28 => {
                if let Some(l) = decode_light_block(&rx.data[1..]) {
                    let e = light.entry(l.macro_index).or_default();
                    e.0 += 1;
                    e.1 = fnv(&l.light);
                    e.2 = l.light.iter().filter(|b| **b != 0).count();
                }
            }
            0x04 => {
                if let Some(b) = decode_server_block(&rx.data[1..]) {
                    let e = blocks.entry(b.macro_index).or_default();
                    e.0 += 1;
                    let (mut sun, mut fog, mut tiles) = (Vec::new(), Vec::new(), Vec::new());
                    for t in b.tiles.chunks(64) {
                        sun.push(t[7]);
                        fog.push(t[9]);
                        tiles.extend_from_slice(&t[0..4]);
                    }
                    e.1 = format!(
                        "tiles={} sun={} explored={}/{} light={}",
                        &fnv(&tiles)[..8],
                        &fnv(&sun)[..8],
                        fog.iter().filter(|v| **v != 0).count(),
                        &fnv(&fog)[..8],
                        &fnv(&b.light)[..8]
                    );
                }
            }
            _ => {}
        }
    }
    for (k, (c, m, n)) in &light {
        obs.add(format!("{} LIGHT_BLOCK idx={} nonzero={} hash={} (n={})", phase, k, n, &m[..8], c));
    }
    obs.add(format!("{} light blocks: {} distinct", phase, light.len()));
    for (k, (_c, f)) in &blocks {
        obs.add(format!("{} BLOCK idx={} {}", phase, k, f));
    }
    obs.add(format!("{} blocks: {} distinct", phase, blocks.len()));
    let explored: usize = blocks
        .values()
        .filter_map(|(_, f)| f.split("explored=").nth(1)?.split('/').next()?.parse::<usize>().ok())
        .sum();
    let lit = blocks.values().filter(|(_, f)| !f.contains("explored=0/")).count();
    // bucketed so that a few fog-edge tiles of timing jitter do not count as a difference
    obs.add(format!("{} explored_total ~{}00 tiles in {} blocks", phase, explored / 100, lit));
}

/// First air tile with solid ground below, scanning column x down from y0 (blocks must be loaded).
fn ground_at(s: &Session, x: i32, y0: i32) -> Option<i32> {
    for y in (1..=y0).rev() {
        let a = s.state.block_msgs.get(&(s.macro_index_for_tile(x, y) as u32))?;
        let b = s.state.block_msgs.get(&(s.macro_index_for_tile(x, y - 1) as u32))?;
        let t = tile(a, x, y);
        let air = matches!(t[0], 2 | 5) && t[3] == 0 && t[11] == 0;
        let solid = !matches!(tile(b, x, y - 1)[0], 2 | 3 | 5);
        if air && solid {
            return Some(y);
        }
    }
    None
}

fn new_blockhead(s: &mut Session, name: &str, x: i32, y: i32) -> Option<u64> {
    let before = s.state.created.len();
    s.create_blockhead(&BlockheadSpec::new(name, x as u32, y as u32)).ok()?;
    let _ = s.wait_for_matching(8000, &mut |r| r.data.first() == Some(&0x07) && r.data.get(1) == Some(&T_BLOCKHEAD));
    s.wait_quiet(500, 3000).ok()?;
    (s.state.created.len() > before).then(|| s.state.created.last().unwrap().unique_id)
}

fn join(addr: std::net::SocketAddr, alias: &str, trace: &Path) -> Session {
    let cfg = ClientConfig { addr, ..Default::default() };
    let mut s = Session::connect(cfg, Identity::new(alias)).expect("connect");
    s.set_trace(Some(TraceSink::create(trace, alias).unwrap()));
    assert_eq!(s.join(10000).unwrap(), JoinOutcome::Approved, "join {}", alias);
    s.request_initial_data().unwrap();
    s.wait_for(0x06, 8000).unwrap();
    s.wait_quiet(800, 5000).unwrap();
    s
}

// ------------------------------------------------------------------------------------------- the script

fn play(addr: std::net::SocketAddr, dir: &Path) -> Obs {
    let mut obs = Obs::default();
    let mut a = join(addr, "AUDITA", &dir.join("trace_A.jsonl"));
    let sp = |k: &str, s: &Session| s.state.summary.as_ref().and_then(|p| p.object_for_key(k)).map(|v| v.int_value());
    let (px, py) = (sp("startPortalPos.x", &a).unwrap(), sp("startPortalPos.y", &a).unwrap());
    obs.add(format!("portal {},{}", px, py));
    absorb(&mut a, &mut obs, "join");

    // ---- spawn area + blockhead
    request_around(&mut a, px, py, 1, 1);
    a.wait_quiet(800, 5000).unwrap();
    let bx = px + 3;
    let by = ground_at(&a, bx, py + 20).unwrap_or(py);
    obs.add(format!("blockhead ground {},{}", bx, by));
    let Some(bh) = new_blockhead(&mut a, "Ann", bx, by) else {
        obs.add("NO BLOCKHEAD CREATED");
        return obs;
    };
    absorb(&mut a, &mut obs, "bh_create");

    // ---- chest: place, open, put, close, reopen, take, close
    let cx = bx + 2;
    let cy = ground_at(&a, cx, by + 10).unwrap_or(by);
    a.upload_inventory(bh, &slots(8, &[(0, ITEM_CHEST, 1), (1, ITEM_DIRT, 10)])).unwrap();
    a.run_for(300).unwrap();
    let tail = d(vec![
        ("item", PV::data(item(ITEM_CHEST))),
        ("blockhead_uniqueID", PV::int(bh as i64)),
        ("blockhead_inventory_change", PV::data(plist_gz(&slots(8, &[(1, ITEM_DIRT, 10)])).unwrap())),
    ]);
    a.place_interaction_object(PlaceInteractionObjectNetRequest { pos_x: cx as u32, pos_y: cy as u32 }, &tail).unwrap();
    let _ = a.wait_for_matching(5000, &mut |r| r.data.first() == Some(&0x07) && r.data.get(1) == Some(&T_CHEST));
    a.wait_quiet(800, 4000).unwrap();
    obs.add(format!("chest target {},{}", cx, cy));
    absorb(&mut a, &mut obs, "chest_place");
    let Some((_cid, cblob)) =
        a.state.objects_seen.iter().filter(|((t, _), _)| *t == T_CHEST).map(|((_, id), b)| (*id, b.clone())).last()
    else {
        obs.add("NO CHEST CREATED");
        return obs;
    };
    let owner = a.id.player_id();
    let send_chest = |s: &mut Session, who: u64, in_use: bool, sl: Option<PV>| {
        s.send_object_batch(BatchKind::Update, T_CHEST, &[chest_blob(&cblob, in_use, who, sl, &owner)]).unwrap();
        s.wait_quiet(600, 2500).unwrap();
    };
    send_chest(&mut a, bh, true, None); // open: no inventory in the update
    absorb(&mut a, &mut obs, "chest_open1");
    send_chest(&mut a, bh, true, Some(slots(16, &[(3, ITEM_DIRT, 6)]))); // put 6 dirt into slot 3
    a.upload_inventory(bh, &slots(8, &[(1, ITEM_DIRT, 4)])).unwrap();
    absorb(&mut a, &mut obs, "chest_put");
    send_chest(&mut a, bh, false, Some(slots(16, &[(3, ITEM_DIRT, 6)]))); // close
    absorb(&mut a, &mut obs, "chest_close1");
    send_chest(&mut a, bh, true, None); // reopen
    absorb(&mut a, &mut obs, "chest_open2");
    send_chest(&mut a, bh, true, Some(slots(16, &[(3, ITEM_DIRT, 4)]))); // take 2
    send_chest(&mut a, bh, false, Some(slots(16, &[(3, ITEM_DIRT, 4)])));
    absorb(&mut a, &mut obs, "chest_take_close");

    // ---- second player opens, edits and closes the same chest; then A reopens
    a.run_for(100).unwrap();
    let mut b = join(addr, "AUDITB", &dir.join("trace_B.jsonl"));
    a.run_for(100).unwrap();
    for i in [b.macro_index_for_tile(px, py), b.macro_index_for_tile(px - 32, py), b.macro_index_for_tile(px + 32, py)]
    {
        b.request_block(i, true).unwrap();
        b.wait_for(0x04, 3000).unwrap();
        a.run_for(100).unwrap();
    }
    b.wait_quiet(500, 2000).unwrap();
    a.run_for(100).unwrap();
    let bh_b = new_blockhead(&mut b, "Bob", bx - 1, by).unwrap_or(0);
    a.run_for(500).unwrap();
    absorb(&mut b, &mut obs, "B_join");
    absorb(&mut a, &mut obs, "A_B_join");
    send_chest(&mut b, bh_b, true, None);
    absorb(&mut b, &mut obs, "B_chest_open");
    a.run_for(300).unwrap();
    absorb(&mut a, &mut obs, "A_while_B_open");
    send_chest(&mut b, bh_b, false, Some(slots(16, &[(0, ITEM_ROCK, 1), (3, ITEM_DIRT, 4)])));
    absorb(&mut b, &mut obs, "B_chest_close");
    a.run_for(300).unwrap();
    absorb(&mut a, &mut obs, "A_after_B_close");
    send_chest(&mut a, bh, true, None);
    absorb(&mut a, &mut obs, "A_chest_open3");
    send_chest(&mut a, bh, false, None);
    absorb(&mut a, &mut obs, "A_chest_close3");

    // ---- break / place a block
    let fx = cx + 3;
    let fy = ground_at(&a, fx, cy + 10).unwrap_or(cy) - 1;
    a.remove(RemoveNetRequest { remove_blockhead_id: bh, pos_x: fx as u32, pos_y: fy as u32, ..Default::default() })
        .unwrap();
    a.wait_quiet(800, 4000).unwrap();
    absorb(&mut a, &mut obs, "remove");
    a.fill(FillNetRequest { pos_x: fx as u32, pos_y: fy as u32, type_: 0x0b, ..Default::default() }, None).unwrap();
    a.wait_quiet(800, 4000).unwrap();
    absorb(&mut a, &mut obs, "fill");

    // ---- workbench: place, open, "craft" (client inventory gains sticks), close
    let wx = bx - 4;
    let wy = ground_at(&a, wx, by + 10).unwrap_or(by);
    a.place_workbench(
        PlaceWorkbenchNetRequest { pos_x: wx as u32, pos_y: wy as u32, workbench_type: 2, ..Default::default() },
        None,
    )
    .unwrap();
    let _ = a.wait_for_matching(5000, &mut |r| r.data.first() == Some(&0x07) && r.data.get(1) == Some(&T_WORKBENCH));
    a.wait_quiet(800, 4000).unwrap();
    absorb(&mut a, &mut obs, "workbench_place");
    let wb = a.state.objects_seen.iter().filter(|((t, _), _)| *t == T_WORKBENCH).map(|(_, b)| b.clone()).last();
    if let Some(wblob) = wb {
        let mut w = wblob[..0x38.min(wblob.len())].to_vec();
        w.resize(0x38, 0);
        for (in_use, who) in [(1u8, bh), (0u8, 0u64)] {
            let mut wi = w.clone();
            wi[0x18..0x20].copy_from_slice(&who.to_le_bytes());
            wi[0x22] = in_use;
            wi.extend(plist_gz(&d(vec![])).unwrap());
            a.send_object_batch(BatchKind::Update, T_WORKBENCH, &[wi]).unwrap();
            a.wait_quiet(800, 4000).unwrap();
            if in_use == 1 {
                a.upload_inventory(bh, &slots(8, &[(1, ITEM_DIRT, 4), (2, ITEM_STICK, 2)])).unwrap();
            }
        }
    } else {
        obs.add("NO WORKBENCH CREATED");
    }
    absorb(&mut a, &mut obs, "workbench_use");
    b.disconnect();

    // ---- walk far: right, then back left past the start (adjacent steps, block requests as the client does)
    let steps: i32 = std::env::var("BH_DIFF_WALK").ok().and_then(|v| v.parse().ok()).unwrap_or(160);
    let mut pos = (bx, by);
    for dirn in [1i32, -1] {
        let n = if dirn < 0 { steps * 2 } else { steps };
        for i in 0..n {
            let to = (pos.0 + dirn, ground_at(&a, pos.0 + dirn, pos.1 + 6).unwrap_or(pos.1));
            a.send_object_batch(BatchKind::Update, T_BLOCKHEAD, &[bh_update(bh, pos, to)]).unwrap();
            pos = to;
            if i % 8 == 0 {
                request_around(&mut a, pos.0, pos.1, 2, 1);
            }
            a.run_for(60).unwrap();
        }
        a.wait_quiet(1500, 8000).unwrap();
        obs.add(format!("walk end {},{}", pos.0, pos.1));
        absorb(&mut a, &mut obs, if dirn > 0 { "walk_right" } else { "walk_left" });
    }
    light_and_block_summary(&a, &mut obs, "walk");
    a.disconnect();
    obs
}

/// After a restart with `--load`: a fresh blockhead opens the chest placed before; the inventory must survive.
fn play_reload(addr: std::net::SocketAddr, dir: &Path, obs: &mut Obs) {
    let mut a = join(addr, "AUDITA", &dir.join("trace_reload.jsonl"));
    let sp = |k: &str, s: &Session| s.state.summary.as_ref().and_then(|p| p.object_for_key(k)).map(|v| v.int_value());
    let (px, py) = (sp("startPortalPos.x", &a).unwrap(), sp("startPortalPos.y", &a).unwrap());
    obs.add(format!(
        "reload initial blockheads={}",
        a.state.initial_plist.as_ref().map(decode::pv_json).unwrap_or_default().len()
    ));
    request_around(&mut a, px, py, 1, 1);
    a.wait_quiet(800, 5000).unwrap();
    let bx = px + 3;
    let by = ground_at(&a, bx, py + 20).unwrap_or(py);
    let Some(bh) = new_blockhead(&mut a, "Cat", bx, by) else {
        obs.add("reload NO BLOCKHEAD");
        return;
    };
    absorb(&mut a, obs, "reload_join");
    let chest = a.state.objects_seen.iter().filter(|((t, _), _)| *t == T_CHEST).map(|(_, b)| b.clone()).last();
    let Some(cblob) = chest else {
        obs.add("reload NO CHEST");
        return;
    };
    let owner = a.id.player_id();
    for in_use in [true, false] {
        a.send_object_batch(BatchKind::Update, T_CHEST, &[chest_blob(&cblob, in_use, bh, None, &owner)]).unwrap();
        a.wait_quiet(600, 2500).unwrap();
    }
    absorb(&mut a, obs, "reload_chest_open");
    a.disconnect();
}

// ------------------------------------------------------------------------------------------- driver

fn run_side(ours: bool, root: &Path) -> (Obs, PathBuf, Box<dyn ServerHandle>) {
    let tag = if ours { "ours" } else { "ref" };
    let dir = root.join(tag);
    std::fs::create_dir_all(&dir).unwrap();
    let spec = RunSpec { tag: format!("diffgp-{}", tag), args: server_args(), ..Default::default() };
    let mut h: Box<dyn ServerHandle> =
        if ours { Box::new(OurServer::start(&spec).unwrap()) } else { Box::new(RefServer::start(&spec).unwrap()) };
    h.wait_for_log("World load complete.", Duration::from_secs(120)).expect("load");
    let addr = format!("127.0.0.1:{}", h.port()).parse().unwrap();
    eprintln!("== {} on {}", tag, addr);
    let obs = play(addr, &dir);
    let _ = h.wait_for_saves(WORLD, 2, Duration::from_secs(30));
    let _ = h.stop_sigint(Duration::from_secs(15));
    std::fs::write(dir.join("obs.txt"), obs.lines.join("\n") + "\n").unwrap();
    std::fs::write(dir.join("server.log"), h.log()).unwrap();
    // restart from the saved world and check the chest
    let data = dir.join("data_reload");
    let _ = copy_dir_all(&h.data_dir(), &data);
    let mut obs = obs;
    let rspec = RunSpec {
        tag: format!("diffgp-{}-reload", tag),
        args: ["--load", WORLD].iter().map(|s| s.to_string()).collect(),
        data_dir: Some(data),
        ..Default::default()
    };
    let mut h2: Box<dyn ServerHandle> =
        if ours { Box::new(OurServer::start(&rspec).unwrap()) } else { Box::new(RefServer::start(&rspec).unwrap()) };
    if h2.wait_for_log("World load complete.", Duration::from_secs(120)).is_ok() {
        let addr2 = format!("127.0.0.1:{}", h2.port()).parse().unwrap();
        play_reload(addr2, &dir, &mut obs);
    } else {
        obs.add("reload FAILED TO LOAD");
    }
    let _ = h2.stop_sigint(Duration::from_secs(15));
    std::fs::write(dir.join("server_reload.log"), h2.log()).unwrap();
    std::fs::write(dir.join("obs.txt"), obs.lines.join("\n") + "\n").unwrap();
    let save = dir.join("save");
    let _ = copy_dir_all(&h.save_dir(WORLD), &save);
    if let Ok(sum) = summary(&save) {
        let _ = std::fs::write(dir.join("save_summary.txt"), sum);
    }
    (obs, save, h)
}

#[test]
fn diff_gameplay() {
    if !parity_enabled() {
        eprintln!("SKIP: diff_gameplay (BH_PARITY=1 + docker + BH_REFENV required)");
        return;
    }
    let (root, _tmp) = match std::env::var("BH_DIFF_OUT") {
        Ok(p) => {
            std::fs::create_dir_all(&p).unwrap();
            (PathBuf::from(p), None)
        }
        Err(_) => {
            let t = TempDir::new("diffgp").unwrap();
            (t.path().to_path_buf(), Some(t))
        }
    };
    let only = std::env::var("BH_DIFF_ONLY").ok();
    let mut sides = Vec::new();
    for ours in [false, true] {
        let tag = if ours { "ours" } else { "ref" };
        if only.as_deref().map_or(true, |o| o == tag) {
            sides.push(run_side(ours, &root));
        }
    }
    if sides.len() < 2 {
        return;
    }
    let (r, o) = (&sides[0], &sides[1]);
    let rs: BTreeSet<&String> = r.0.lines.iter().collect();
    let os: BTreeSet<&String> = o.0.lines.iter().collect();
    let mut rep = String::new();
    let (mut ni, mut nj) = (0, 0);
    // hard observations: deterministic outcomes a player would notice; the rest (packet counts, NPC
    // positions, per-block hashes) depends on wall-clock timing and the shared rand() stream and is reported only
    let hard = |l: &str| {
        ["CHEST_INVENTORY_DATA", "light blocks:", "explored_total", "NO ", "FAILED", "chest target", "portal "]
            .iter()
            .any(|k| l.contains(k))
    };
    let (mut hi, mut hj) = (0, 0);
    for l in r.0.lines.iter().filter(|l| !os.contains(l)) {
        let _ = writeln!(rep, "-{} ref : {}", if hard(l) { "!" } else { " " }, l);
        ni += 1;
        hi += hard(l) as usize;
    }
    for l in o.0.lines.iter().filter(|l| !rs.contains(l)) {
        let _ = writeln!(rep, "+{} ours: {}", if hard(l) { "!" } else { " " }, l);
        nj += 1;
        hj += hard(l) as usize;
    }
    let _ = writeln!(
        rep,
        "observations: ref {} / ours {} lines; {} ref-only, {} ours-only",
        r.0.lines.len(),
        o.0.lines.len(),
        ni,
        nj
    );
    match diff_saves(&r.1, &o.1, &SaveDiffOptions::parity_default()) {
        Ok(sd) => {
            let _ = writeln!(rep, "\n== save diff ==\n{}", sd.render(8));
            std::fs::write(root.join("save_diff_full.txt"), sd.render(100_000)).unwrap();
        }
        Err(e) => {
            let _ = writeln!(rep, "save diff failed: {}", e);
        }
    }
    std::fs::write(root.join("report.txt"), &rep).unwrap();
    eprintln!("{}", rep);
    let _ = writeln!(rep, "hard differences (marked !): {} ref-only, {} ours-only", hi, hj);
    std::fs::write(root.join("report.txt"), &rep).unwrap();
    assert!(hi == 0 && hj == 0, "hard differences found; see {}", root.join("report.txt").display());
}
