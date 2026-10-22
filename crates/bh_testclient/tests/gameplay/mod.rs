//! Scripted gameplay scenarios and their differential checks.
//!
//! A scenario is a client script (one or more [`Session`]s, optionally typing into the server console) that
//! records *observations*: `key -> value` pairs such as "the chest inventory the server sent when A reopened the
//! chest", "how many torches were created during the walk" or "how many `dw/*/17` records the save holds".
//!
//! * **Differential runs** (`BH_PARITY=1`, docker): the scenario runs against the reference server
//!   [`NoiseMask::RUNS`] times and once against ours, all from the same seeded `--new` world. Keys whose value is
//!   identical in every reference run are *stable* and become hard checks; keys that vary between reference runs
//!   are *volatile* and only reported. Keys starting with `~` are always soft (wall-clock cadence).
//! * **Fixture runs** (no docker): the learned mask is checked in under `tests/fixtures/scenarios/<name>.txt`
//!   and the scenario runs against our server in-process ([`HostServer`]); every stable key must match. Known
//!   bugs are listed in `<name>.xfail` (`key  # reason`) and must still fail (strict), so a fix shows up as a
//!   failing test that asks for the entry to be removed.
//!
//! Regenerating fixtures: `BH_PARITY=1 BH_REFENV=<refenv> BH_SCENARIO_BLESS=1 cargo test -p bh_testclient
//! --test diff_gameplay -- --nocapture` (see `tests/fixtures/scenarios/README.md`).
//!
//! This kit and its scripts are test code shared by the `diff_gameplay` and `scenarios_ours` test binaries
//! (`mod gameplay;` in each); each binary uses only part of it.
#![allow(dead_code)]

pub mod scenarios;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use blockheads_server::foundation::gsdict::GsDict;
use blockheads_server::plist::value::PlistValue as PV;
use blockheads_server::protocol::codec::{gz_plist_from, plist_gz};
use blockheads_server::protocol::object_batches::decode_batch;

use bh_testclient::harness::inproc::HostServer;
use bh_testclient::harness::refenv::{copy_dir_all, OurServer, RefServer, RunSpec, ServerHandle};
use bh_testclient::harness::save_diff::{diff_saves, load_records, SaveDiffOptions};
use bh_testclient::harness::{parity_enabled, to_hex, TempDir};
use bh_testclient::session::BatchKind;
use bh_testclient::{BlockheadSpec, ClientConfig, Identity, JoinOutcome, Session, TraceSink};

// ------------------------------------------------------------------------------------------------ constants

/// World id of every scenario world (`--world_id`).
pub const WORLD: &str = "e98c5bee38be74e00ad7748a3dd42119";

pub mod obj {
    //! Dynamic object types.
    pub const TORCH: u8 = 17;
    pub const BLOCKHEAD: u8 = 24;
    pub const WORKBENCH: u8 = 45;
    pub const CHEST: u8 = 46;
}

/// `--new PROTO --world_id WORLD --world_width 1/4 --custom-rules {seed 12345}`.
pub fn world_args() -> Vec<String> {
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

// ------------------------------------------------------------------------------------------------ observations

/// Observations of one run: `key -> value`, in recording order. A repeated key gets a `#n` suffix.
#[derive(Clone, Debug, Default)]
pub struct Obs {
    pub kv: Vec<(String, String)>,
    seen: BTreeMap<String, usize>,
}

fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

impl Obs {
    /// Records `key = value`.
    pub fn add(&mut self, key: impl Into<String>, value: impl ToString) {
        let mut key = clean(&key.into());
        let n = self.seen.entry(key.clone()).or_insert(0);
        *n += 1;
        if *n > 1 {
            key = format!("{}#{}", key, n);
        }
        let value = clean(&value.to_string());
        eprintln!("  obs: {} = {}", key, value);
        self.kv.push((key, value));
    }
    /// Records an always-soft observation (reported, never a hard check).
    pub fn soft(&mut self, key: impl Into<String>, value: impl ToString) {
        self.add(format!("~{}", key.into()), value);
    }
    pub fn map(&self) -> BTreeMap<String, String> {
        self.kv.iter().cloned().collect()
    }
    pub fn render(&self) -> String {
        self.kv.iter().map(|(k, v)| format!("{}\t{}\n", k, v)).collect()
    }
}

// ------------------------------------------------------------------------------------------------ noise mask

/// Stable and volatile keys learned from several reference runs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NoiseMask {
    /// identical in every reference run (`~` keys included, they are compared softly)
    pub stable: BTreeMap<String, String>,
    /// missing from a run or different between runs
    pub volatile: BTreeSet<String>,
}

/// Counts that follow wall-clock timing and machine speed rather than game logic: packets received per type in a
/// phase, objects created/removed per type in a phase, block resends after an action, how many blocks a phase
/// received or explored, and how many block / light-block records the save holds (the area the server happened to
/// load). Fixtures learned on one fast machine pin them exactly, but slower runners drift by a few, so they are
/// compared with [`count_tolerance`]. Object counts per type in the save, inventories, paint, sign text, replies and
/// chat stay exact.
pub fn timing_count(key: &str) -> bool {
    let k = key.trim_start_matches('~');
    let last = k.rsplit('.').next().unwrap_or("");
    let per_type = |kind: &str| {
        k.rsplit('.').nth(1) == Some(kind)
            && last.len() > 1
            && last.starts_with('t')
            && last[1..].chars().all(|c| c.is_ascii_digit())
    };
    k.rsplit('.').nth(1) == Some("rx")
        || per_type("create")
        || per_type("remove")
        || matches!(last, "block_resends" | "blocks" | "light_blocks" | "explored_blocks")
        || k == "save.world_db.blocks.N_N"
        || k == "save.lightBlocks.main.H_N_N"
}

/// How far a [`timing_count`] may drift from the reference: 2, or a quarter of the reference value when larger.
pub fn count_tolerance(want: u64) -> u64 {
    want.div_ceil(4).max(2)
}

/// Both values are counts within [`count_tolerance`] of each other.
pub fn counts_close(want: &str, got: &str) -> bool {
    match (want.parse::<u64>(), got.parse::<u64>()) {
        (Ok(w), Ok(g)) => w.abs_diff(g) <= count_tolerance(w),
        _ => false,
    }
}

/// Result of checking one run against a mask.
#[derive(Clone, Debug, Default)]
pub struct Verdict {
    pub hard: Vec<String>,
    pub soft: Vec<String>,
}

impl NoiseMask {
    /// Reference runs per differential test (env `BH_DIFF_REF_RUNS`, default 2).
    pub const RUNS: usize = 2;

    pub fn learn(runs: &[BTreeMap<String, String>]) -> NoiseMask {
        let mut m = NoiseMask::default();
        let keys: BTreeSet<&String> = runs.iter().flat_map(|r| r.keys()).collect();
        for k in keys {
            match runs[0].get(k) {
                Some(first) if runs.iter().all(|r| r.get(k) == Some(first)) => {
                    m.stable.insert(k.clone(), first.clone());
                }
                _ => {
                    m.volatile.insert(k.clone());
                }
            }
        }
        m
    }

    /// Hard: a stable non-`~` key differs or is missing, or a key that no reference run had shows up. A
    /// [`timing_count`] key is hard only when it is off by more than [`count_tolerance`] (a missing count is 0).
    pub fn check(&self, ours: &BTreeMap<String, String>) -> Verdict {
        let mut v = Verdict::default();
        for (k, want) in &self.stable {
            let got = ours.get(k);
            let line = match got {
                Some(got) if got == want => continue,
                Some(got) => format!("{}: ref `{}` ours `{}`", k, want, got),
                None => format!("{}: ref `{}` ours <missing>", k, want),
            };
            let near = timing_count(k) && counts_close(want, got.map_or("0", String::as_str));
            if k.starts_with('~') || near {
                v.soft.push(line);
            } else {
                v.hard.push(line);
            }
        }
        for (k, got) in ours {
            if self.stable.contains_key(k) || self.volatile.contains(k) {
                continue;
            }
            let line = format!("{}: ref <missing> ours `{}`", k, got);
            if k.starts_with('~') || (timing_count(k) && counts_close("0", got)) {
                v.soft.push(line);
            } else {
                v.hard.push(line);
            }
        }
        v
    }

    /// Fixture text: `S<TAB>key<TAB>value` / `V<TAB>key` lines after a `#` header.
    pub fn to_fixture(&self, header: &str) -> String {
        let mut s = String::new();
        for l in header.lines() {
            let _ = writeln!(s, "# {}", l);
        }
        for (k, v) in &self.stable {
            let _ = writeln!(s, "S\t{}\t{}", k, v);
        }
        for k in &self.volatile {
            let _ = writeln!(s, "V\t{}", k);
        }
        s
    }

    pub fn from_fixture(text: &str) -> NoiseMask {
        let mut m = NoiseMask::default();
        for l in text.lines() {
            let mut it = l.splitn(3, '\t');
            match (it.next(), it.next(), it.next()) {
                (Some("S"), Some(k), v) => {
                    m.stable.insert(k.to_string(), v.unwrap_or("").to_string());
                }
                (Some("V"), Some(k), _) => {
                    m.volatile.insert(k.to_string());
                }
                _ => {}
            }
        }
        m
    }
}

/// Keys of a `.xfail` file (`key  # reason`; blank lines and `#` lines ignored).
pub fn parse_xfail(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| match l.split_once(" #") {
            Some((k, r)) => (k.trim().to_string(), r.trim().to_string()),
            None => (l.to_string(), String::new()),
        })
        .collect()
}

/// The key a verdict line is about.
pub fn verdict_key(line: &str) -> &str {
    line.split(": ").next().unwrap_or(line)
}

// ------------------------------------------------------------------------------------------------ item / blob builders

pub fn fnv(b: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for x in b {
        h ^= *x as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h)
}

/// `InventoryItem saveData` header only: `{u16 type, u16 dataA, u16 dataB, u8 selected, u8 0}`.
pub fn item(t: u16) -> Vec<u8> {
    item_ab(t, 0, 0)
}

/// Item with `dataA` / `dataB` (paint colour, …).
pub fn item_ab(t: u16, a: u16, b: u16) -> Vec<u8> {
    let mut v = Vec::with_capacity(8);
    v.extend_from_slice(&t.to_le_bytes());
    v.extend_from_slice(&a.to_le_bytes());
    v.extend_from_slice(&b.to_le_bytes());
    v.extend_from_slice(&[0, 0]);
    v
}

/// `n` slots; `fill = [(slot, item type, count)]`.
pub fn slots(n: usize, fill: &[(usize, u16, usize)]) -> PV {
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

pub fn d(pairs: Vec<(&str, PV)>) -> PV {
    let mut m: GsDict<String, PV> = GsDict::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), v);
    }
    PV::dict(m)
}

/// `s<slot>:<type>x<count>` per non-empty slot of a `saveItemSlots` array (or a dict holding one).
pub fn slot_summary(v: &PV) -> String {
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
pub fn chest_blob(base: &[u8], in_use: bool, bh: u64, slots_pv: Option<PV>, owner: &str) -> Vec<u8> {
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
pub fn bh_update(uid: u64, from: (i32, i32), to: (i32, i32)) -> Vec<u8> {
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

// ------------------------------------------------------------------------------------------------ world helpers

pub fn tile(bm: &blockheads_server::protocol::block_wire::BlockMsg, x: i32, y: i32) -> &[u8] {
    let i = ((y & 31) * 32 + (x & 31)) as usize * 64;
    &bm.tiles[i..i + 64]
}

/// The 64 tile bytes at (x, y) if the block is loaded.
pub fn tile_at(s: &Session, x: i32, y: i32) -> Option<Vec<u8>> {
    let b = s.state.block_msgs.get(&(s.macro_index_for_tile(x, y) as u32))?;
    Some(tile(b, x, y).to_vec())
}

/// First air tile with solid ground below, scanning column x down from y0 (blocks must be loaded).
pub fn ground_at(s: &Session, x: i32, y0: i32) -> Option<i32> {
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

/// Request every macro block in a box around tile (x, y), throttled like the real client.
pub fn request_around(s: &mut Session, x: i32, y: i32, rx: i32, ry: i32) {
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

pub fn new_blockhead(s: &mut Session, name: &str, x: i32, y: i32) -> Option<u64> {
    let before = s.state.created.len();
    s.create_blockhead(&BlockheadSpec::new(name, x as u32, y as u32)).ok()?;
    let _ = s.wait_for_matching(8000, &mut |r| r.data.first() == Some(&0x07) && r.data.get(1) == Some(&obj::BLOCKHEAD));
    s.wait_quiet(500, 3000).ok()?;
    (s.state.created.len() > before).then(|| s.state.created.last().unwrap().unique_id)
}

pub fn join(addr: SocketAddr, alias: &str, trace: &Path) -> Session {
    let cfg = ClientConfig { addr, ..Default::default() };
    let mut s = Session::connect(cfg, Identity::new(alias)).expect("connect");
    s.set_trace(Some(TraceSink::create(trace, alias).unwrap()));
    assert_eq!(s.join(10000).unwrap(), JoinOutcome::Approved, "join {}", alias);
    s.request_initial_data().unwrap();
    s.wait_for(0x06, 8000).unwrap();
    s.wait_quiet(800, 5000).unwrap();
    s
}

pub fn summary_int(s: &Session, k: &str) -> Option<i32> {
    s.state.summary.as_ref().and_then(|p| p.object_for_key(k)).map(|v| v.int_value())
}

/// Start portal position.
pub fn portal(s: &Session) -> (i32, i32) {
    (summary_int(s, "startPortalPos.x").unwrap_or(0), summary_int(s, "startPortalPos.y").unwrap_or(0))
}

/// The newest blob of an object of `ty` announced to `s` (by 0x07/0x08).
pub fn last_object(s: &Session, ty: u8) -> Option<(u64, Vec<u8>)> {
    s.state.objects_seen.iter().filter(|((t, _), _)| *t == ty).map(|((_, id), b)| (*id, b.clone())).next_back()
}

/// Wait for a 0x07 of object type `ty` and return its id and blob.
pub fn wait_created(s: &mut Session, ty: u8, ms: u64) -> Option<(u64, Vec<u8>)> {
    let before: BTreeSet<u64> = s.state.objects_seen.keys().filter(|(t, _)| *t == ty).map(|(_, id)| *id).collect();
    let _ = s.wait_for_matching(ms, &mut |r| r.data.first() == Some(&0x07) && r.data.get(1) == Some(&ty));
    let _ = s.wait_quiet(500, 3000);
    s.state
        .objects_seen
        .iter()
        .filter(|((t, id), _)| *t == ty && !before.contains(id))
        .map(|((_, id), b)| (*id, b.clone()))
        .next_back()
}

/// Object blobs of `ty` in server batches (0x07 create, 0x08 update, 0x46 creation-data update) received since
/// packet index `mark` (`s.received().len()` taken earlier): `(packet type, blob)`.
pub fn objects_since(s: &Session, mark: usize, ty: u8) -> Vec<(u8, Vec<u8>)> {
    let mut out = Vec::new();
    for rx in s.received().get(mark..).unwrap_or(&[]) {
        if !matches!(rx.data[0], 0x07 | 0x08 | 0x46) || rx.data.get(1) != Some(&ty) {
            continue;
        }
        if let Ok(Some(b)) = decode_batch(&rx.data) {
            out.extend(b.items.into_iter().map(|i| (rx.data[0], i)));
        }
    }
    out
}

/// Raw packets of type `ty` received since index `mark`.
pub fn packets_since(s: &Session, mark: usize, ty: u8) -> Vec<Vec<u8>> {
    s.received().get(mark..).unwrap_or(&[]).iter().filter(|r| r.data[0] == ty).map(|r| r.data.clone()).collect()
}

/// The gzip plist after a fixed-size net data header.
pub fn tail_plist(blob: &[u8], header: usize) -> Option<PV> {
    gz_plist_from(blob.get(header..)?)
}

pub fn u16_at(b: &[u8], o: usize) -> u16 {
    b.get(o..o + 2).map(|x| u16::from_le_bytes([x[0], x[1]])).unwrap_or(0)
}
pub fn i16_at(b: &[u8], o: usize) -> i16 {
    u16_at(b, o) as i16
}
pub fn u64_at(b: &[u8], o: usize) -> u64 {
    b.get(o..o + 8).map(|x| u64::from_le_bytes(x.try_into().unwrap())).unwrap_or(0)
}
pub fn put_u64(b: &mut [u8], o: usize, v: u64) {
    b[o..o + 8].copy_from_slice(&v.to_le_bytes());
}
pub fn put_u32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
pub fn put_u16(b: &mut [u8], o: usize, v: u16) {
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
}

/// A string value of a plist dict.
pub fn pstr(p: &PV, k: &str) -> String {
    p.object_for_key(k).and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| "<none>".into())
}

// ------------------------------------------------------------------------------------------------ packet summaries

/// Packet types whose count is soft: wall-clock cadence (heartbeat, object updates, electricity, the 2 s coalesced player list), and block /
/// light-block / object-batch counts, which follow the client's request throttling and simulation timing. The
/// content checks (inventories, object counts per type, explored block counts) stay hard.
const CADENCE: &[u8] = &[0x17, 0x08, 0x2c, 0x04, 0x28, 0x07, 0x09, 0x1e];

/// Object types whose creations follow wall-clock simulation (freeblocks dropped by plants/trees, NPC spawns and
/// wandering into view, blockheads re-announced as they cross macro blocks): soft.
pub const SOFT_OBJECTS: &[u8] = &[14, 13, 24, 25, 28, 35, 36, 39, 51, 63];

/// Trees and plants from world generation. How many blocks hold them in the save follows how much of the world
/// the server loaded while the scripts ran, which drifts with build speed: their save record counts are soft.
pub const NATURAL_OBJECTS: &[u8] = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 27, 33, 34, 37, 57, 58, 59, 61, 62];

/// Summarise every packet received since the last drain into observations under `phase`:
/// packet counts per type, object creates/removes per object type, chest inventories, fill/pickup replies and
/// server chat text. Object ids never appear in keys (they follow the shared `rand()` stream).
pub fn absorb(s: &mut Session, obs: &mut Obs, phase: &str) {
    let mut counts: BTreeMap<u8, usize> = BTreeMap::new();
    let mut objs: BTreeMap<(u8, u8), usize> = BTreeMap::new();
    for rx in s.drain() {
        let ty = rx.data[0];
        *counts.entry(ty).or_default() += 1;
        match ty {
            0x32 => {
                let inv = if rx.data.len() > 9 {
                    gz_plist_from(&rx.data[9..]).map(|v| slot_summary(&v)).unwrap_or_else(|| "undecodable".into())
                } else {
                    "none".into()
                };
                obs.add(format!("{}.chest_inventory", phase), inv);
            }
            0x07 | 0x09 => {
                let Ok(Some(b)) = decode_batch(&rx.data) else { continue };
                *objs.entry((ty, b.object_type)).or_default() += b.items.len();
            }
            0x24 => {
                if let Some(p) = blockheads_server::protocol::codec::plist_from(&rx.data[1..]) {
                    let msg = p.object_for_key("message").and_then(|v| v.as_str().map(str::to_string));
                    let alias = p.object_for_key("alias").and_then(|v| v.as_str().map(str::to_string));
                    obs.add(
                        format!("{}.chat", phase),
                        format!("{}: {}", alias.unwrap_or_default(), mask_digits(&msg.unwrap_or_default())),
                    );
                }
            }
            0x1d => obs.add(format!("{}.fill_reply", phase), to_hex(&rx.data[1..])),
            0x13 => obs.add(format!("{}.pickup_reply", phase), rx.data.len()),
            _ => {}
        }
    }
    for (t, c) in counts {
        if CADENCE.contains(&t) {
            obs.soft(format!("{}.rx.{:02x}", phase, t), c);
        } else {
            obs.add(format!("{}.rx.{:02x}", phase, t), c);
        }
    }
    for ((k, t), c) in objs {
        let key = format!("{}.{}.t{}", phase, if k == 0x07 { "create" } else { "remove" }, t);
        if SOFT_OBJECTS.contains(&t) {
            obs.soft(key, c);
        } else {
            obs.add(key, c);
        }
    }
}

/// Memory sizes, uptimes and similar numbers in server text are masked to `#`.
pub fn mask_digits(s: &str) -> String {
    let mut out = String::new();
    let mut prev = false;
    for c in s.chars() {
        let dig = c.is_ascii_digit();
        if dig && !prev {
            out.push('#');
        } else if !dig {
            out.push(c);
        }
        prev = dig;
    }
    out
}

/// Light blocks (0x28) and blocks (0x04) received so far: counts, explored tiles, per-block fingerprints.
pub fn light_and_block_summary(s: &Session, obs: &mut Obs, phase: &str) {
    use blockheads_server::protocol::block_wire::{decode_light_block, decode_server_block};
    let mut light: BTreeMap<u32, (usize, String, usize)> = BTreeMap::new();
    let mut blocks: BTreeMap<u32, (usize, String, usize)> = BTreeMap::new();
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
                    let mut tiles = Vec::new();
                    let mut fog = 0;
                    for t in b.tiles.chunks(64) {
                        fog += (t[9] != 0) as usize;
                        tiles.extend_from_slice(&t[0..4]);
                    }
                    e.1 = fnv(&tiles)[..8].to_string();
                    e.2 = fog;
                }
            }
            _ => {}
        }
    }
    // per-block fingerprints depend on the time of day and NPC/plant activity: soft; the totals are hard
    for (k, (_c, m, _n)) in &light {
        obs.soft(format!("{}.light_block.{}", phase, k), &m[..8]);
    }
    obs.add(format!("{}.light_blocks", phase), light.len());
    for (k, (_c, f, _)) in &blocks {
        obs.soft(format!("{}.block_tiles.{}", phase, k), f);
    }
    obs.add(format!("{}.blocks", phase), blocks.len());
    let explored: usize = blocks.values().map(|b| b.2).sum();
    let lit = blocks.values().filter(|b| b.2 > 0).count();
    // Soft: the tile total follows how far the blockhead got while the client watched, which drifts with build
    // speed (a slow instrumented build crossed the 100-tile bucket). The explored block count stays hard.
    obs.soft(format!("{}.explored_total", phase), format!("~{}00", explored / 100));
    obs.add(format!("{}.explored_blocks", phase), lit);
}

/// Record-count observations of a saved world: per env/db and per normalised key pattern (digits -> `N`,
/// 32-hex ids -> `H`), e.g. `save.world_db.dw.N_N/17 = 5` (torches) or `save.world_db.dw.N_N/chest_N = 1`.
pub fn save_obs(save: &Path, obs: &mut Obs) {
    let Ok(recs) = load_records(save) else {
        obs.add("save.readable", false);
        return;
    };
    let mut pat: BTreeMap<String, usize> = BTreeMap::new();
    for id in recs.keys() {
        let k = id.key_string();
        // `dw` records are `<block>/<object type>` (or `<block>/chest_<id>`): keep the object type
        let p = match (id.db.as_str(), k.split_once('/')) {
            ("dw", Some((blk, ty))) if ty.chars().all(|c| c.is_ascii_digit()) => format!("{}/{}", key_pattern(blk), ty),
            _ => key_pattern(&k),
        };
        *pat.entry(format!("save.{}.{}.{}", id.env, id.db, p)).or_default() += 1;
    }
    for (k, n) in pat {
        let ty = k.rsplit('/').next().and_then(|t| t.parse::<u8>().ok());
        let soft = ty.is_some_and(|t| SOFT_OBJECTS.contains(&t) || NATURAL_OBJECTS.contains(&t));
        if soft {
            obs.soft(k, n);
        } else {
            obs.add(k, n);
        }
    }
}

pub fn key_pattern(k: &str) -> String {
    let mut out = String::new();
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        if run.len() == 32 && run.chars().all(|c| c.is_ascii_hexdigit()) {
            out.push('H');
        } else if !run.is_empty() && run.chars().all(|c| c.is_ascii_digit()) {
            out.push('N');
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for c in k.chars() {
        if c.is_ascii_alphanumeric() {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    out
}

// ------------------------------------------------------------------------------------------------ running

/// Which server a scenario runs against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// the original in docker
    Ref,
    /// our binary (`BH_SERVER_BIN` / target dir)
    OursBin,
    /// our server in-process
    OursHost,
}

impl Side {
    pub fn tag(self) -> &'static str {
        match self {
            Side::Ref => "ref",
            Side::OursBin => "ours",
            Side::OursHost => "host",
        }
    }
    pub fn start(self, spec: &RunSpec) -> Box<dyn ServerHandle> {
        match self {
            Side::Ref => Box::new(RefServer::start(spec).expect("start reference")),
            Side::OursBin => Box::new(OurServer::start(spec).expect("start ours")),
            Side::OursHost => Box::new(HostServer::start(spec).expect("start host")),
        }
    }
}

/// What a scenario script gets.
pub struct Ctx<'a> {
    pub addr: SocketAddr,
    pub dir: PathBuf,
    pub obs: Obs,
    pub server: &'a mut dyn ServerHandle,
    pub side: Side,
    /// `play` or `reload` (trace file names)
    pub phase: &'static str,
}

impl Ctx<'_> {
    pub fn join(&self, alias: &str) -> Session {
        join(self.addr, alias, &self.dir.join(format!("trace_{}_{}.jsonl", self.phase, alias)))
    }
    /// Type a console line and wait a little for the server to act on it.
    pub fn console(&mut self, line: &str) {
        let before = self.server.log().len();
        if let Err(e) = self.server.send_console(line) {
            self.obs.soft(format!("console_failed.{}", line), mask_digits(&e.to_string()));
            return;
        }
        let end = std::time::Instant::now() + Duration::from_millis(1500);
        while std::time::Instant::now() < end {
            std::thread::sleep(Duration::from_millis(100));
            if self.server.log().len() > before {
                std::thread::sleep(Duration::from_millis(300));
                break;
            }
        }
    }
    /// Console output since byte offset `from` of the log, NSLog prefixes stripped, digits masked.
    pub fn log_since(&self, from: usize) -> Vec<String> {
        let log = self.server.log();
        log.get(from..)
            .unwrap_or("")
            .lines()
            .map(|l| mask_digits(bh_testclient::harness::strip_nslog_prefix(l)))
            .filter(|l| !l.trim().is_empty())
            .collect()
    }
}

/// A scenario: a script plus optional reload script (run after a restart from the saved world).
#[derive(Clone, Copy)]
pub struct Scenario {
    pub name: &'static str,
    pub play: fn(&mut Ctx),
    pub reload: Option<fn(&mut Ctx)>,
    /// add save record counts to the observations
    pub save: bool,
    pub console: bool,
}

/// Runs a script; a panic (lost connection, missing reply) becomes the observation `<what>.panic`.
fn guarded(ctx: &mut Ctx, f: fn(&mut Ctx), what: &str) {
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(ctx)));
    if let Err(e) = r {
        let msg = e
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default();
        // ports and addresses differ between runs
        let msg = mask_digits(msg.lines().next().unwrap_or(""));
        ctx.obs.add(format!("{}.panic", what), msg);
    }
}

/// Runs a scenario once and returns its observations (plus the save copy under `root/<tag>/save`).
pub fn run_side(sc: &Scenario, side: Side, root: &Path, run: usize) -> (Obs, PathBuf) {
    let tag = format!("{}{}", side.tag(), if run > 0 { format!("{}", run + 1) } else { String::new() });
    let dir = root.join(&tag);
    std::fs::create_dir_all(&dir).unwrap();
    let spec = RunSpec {
        tag: format!("sc-{}-{}", sc.name, tag),
        args: world_args(),
        console: sc.console,
        ..Default::default()
    };
    let mut h = side.start(&spec);
    h.wait_for_log("World load complete.", Duration::from_secs(180)).expect("load");
    let addr = format!("127.0.0.1:{}", h.port()).parse().unwrap();
    eprintln!("== {} {} on {}", sc.name, tag, addr);
    let mut ctx = Ctx { addr, dir: dir.clone(), obs: Obs::default(), server: h.as_mut(), side, phase: "play" };
    guarded(&mut ctx, sc.play, "script");
    let mut obs = std::mem::take(&mut ctx.obs);
    if h.is_running() {
        let _ = h.wait_for_saves(WORLD, 2, Duration::from_secs(30));
    } else {
        obs.add("server.exited", true);
    }
    let _ = h.stop_sigint(Duration::from_secs(15));
    std::fs::write(dir.join("server.log"), h.log()).unwrap();
    let save = dir.join("save");
    let _ = copy_dir_all(&h.save_dir(WORLD), &save);
    if sc.save {
        save_obs(&save, &mut obs);
    }
    if let Some(reload) = sc.reload {
        let data = dir.join("data_reload");
        let _ = copy_dir_all(&h.data_dir(), &data);
        let rspec = RunSpec {
            tag: format!("sc-{}-{}-reload", sc.name, tag),
            args: ["--load", WORLD].iter().map(|s| s.to_string()).collect(),
            data_dir: Some(data.clone()),
            console: sc.console,
            ..Default::default()
        };
        let mut h2 = side.start(&rspec);
        if h2.wait_for_log("World load complete.", Duration::from_secs(180)).is_ok() {
            let addr2 = format!("127.0.0.1:{}", h2.port()).parse().unwrap();
            let mut ctx = Ctx { addr: addr2, dir: dir.clone(), obs, server: h2.as_mut(), side, phase: "reload" };
            guarded(&mut ctx, reload, "reload_script");
            obs = std::mem::take(&mut ctx.obs);
        } else {
            obs.add("reload.loaded", false);
        }
        let _ = h2.stop_sigint(Duration::from_secs(15));
        std::fs::write(dir.join("server_reload.log"), h2.log()).unwrap();
        drop(h2);
        let _ = std::fs::remove_dir_all(&data);
    }
    std::fs::write(dir.join("obs.txt"), obs.render()).unwrap();
    drop(h);
    (obs, save)
}

type Pending = (Side, std::sync::mpsc::Receiver<(Obs, PathBuf)>);

fn spawn_side(sc: Scenario, side: Side, root: &Path, run: usize) -> Pending {
    let (tx, rx) = std::sync::mpsc::channel();
    let root = root.to_path_buf();
    std::thread::spawn(move || {
        let _ = tx.send(run_side(&sc, side, &root, run));
    });
    (side, rx)
}

/// Per-run watchdog (env `BH_SCENARIO_TIMEOUT`, seconds, default 900): a run that hangs (a lost connection that
/// is waited for, a server thread that never stops) fails the test instead of blocking the suite. The reference
/// containers of this process for the scenario are removed; a hung in-process server thread is abandoned.
fn await_side(sc: &Scenario, (side, rx): Pending) -> (Obs, PathBuf) {
    let secs: u64 = std::env::var("BH_SCENARIO_TIMEOUT").ok().and_then(|v| v.parse().ok()).unwrap_or(900);
    match rx.recv_timeout(Duration::from_secs(secs)) {
        Ok(r) => r,
        Err(e) => {
            if side == Side::Ref {
                remove_own_containers(sc.name);
            }
            panic!("{}: {} run did not finish within {} s ({:?})", sc.name, side.tag(), secs, e);
        }
    }
}

/// `docker rm -f` the reference containers this process started for `scenario` (names carry our pid).
fn remove_own_containers(scenario: &str) {
    use std::process::{Command, Stdio};
    let Ok(out) = Command::new("docker").args(["ps", "-a", "--format", "{{.Names}}"]).stderr(Stdio::null()).output()
    else {
        return;
    };
    let pid = format!("-{}-", std::process::id());
    let prefix = format!("bh-diff-sc-{}-", scenario.replace('_', "-"));
    for name in String::from_utf8_lossy(&out.stdout).lines() {
        if name.starts_with(&prefix) && name.contains(&pid) {
            let _ =
                Command::new("docker").args(["rm", "-f", name]).stdout(Stdio::null()).stderr(Stdio::null()).status();
        }
    }
}

fn out_root(name: &str) -> (PathBuf, Option<TempDir>) {
    match std::env::var("BH_DIFF_OUT") {
        Ok(p) => {
            let p = PathBuf::from(p).join(name);
            std::fs::create_dir_all(&p).unwrap();
            (p, None)
        }
        Err(_) => {
            let t = TempDir::new(name).unwrap();
            (t.path().to_path_buf(), Some(t))
        }
    }
}

/// `tests/fixtures/scenarios` of this crate.
pub fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scenarios")
}

/// The differential test: [`NoiseMask::RUNS`] reference runs, one run of ours, hard/soft verdict, save diff.
/// `BH_SCENARIO_BLESS=1` writes the learned mask as the scenario's fixture. Skips without `BH_PARITY=1`.
pub fn diff_scenario(sc: &Scenario) {
    if !parity_enabled() {
        eprintln!("SKIP: {} (BH_PARITY=1 + docker + image blockheads-ref:1.7.1 required)", sc.name);
        return;
    }
    let (root, _tmp) = out_root(sc.name);
    let runs: usize =
        std::env::var("BH_DIFF_REF_RUNS").ok().and_then(|v| v.parse().ok()).unwrap_or(NoiseMask::RUNS).max(1);
    let ours_side = match std::env::var("BH_DIFF_OURS").as_deref() {
        Ok("host") => Side::OursHost,
        _ => Side::OursBin,
    };
    // the sides are independent servers on their own ports: run them concurrently, each under the watchdog
    let mut pending: Vec<_> = (0..runs).map(|i| spawn_side(*sc, Side::Ref, &root, i)).collect();
    pending.push(spawn_side(*sc, ours_side, &root, 0));
    let mut results: Vec<(Obs, PathBuf)> = pending.into_iter().map(|p| await_side(sc, p)).collect();
    let ours = results.pop().unwrap();
    let refs = results;
    let mask = NoiseMask::learn(&refs.iter().map(|r| r.0.map()).collect::<Vec<_>>());
    let v = mask.check(&ours.0.map());
    let mut rep = String::new();
    let _ = writeln!(
        rep,
        "== {}: {} ref runs, {} stable keys ({} volatile); ours {} keys",
        sc.name,
        runs,
        mask.stable.len(),
        mask.volatile.len(),
        ours.0.kv.len()
    );
    for l in &v.hard {
        let _ = writeln!(rep, "HARD {}", l);
    }
    for l in &v.soft {
        let _ = writeln!(rep, "soft {}", l);
    }
    if sc.save {
        match diff_saves(&refs[0].1, &ours.1, &SaveDiffOptions::parity_default()) {
            Ok(sd) => {
                let _ = writeln!(rep, "\n== save diff (ref run 1 vs ours) ==\n{}", sd.render(12));
                let _ = std::fs::write(root.join("save_diff_full.txt"), sd.render(100_000));
            }
            Err(e) => {
                let _ = writeln!(rep, "save diff failed: {}", e);
            }
        }
    }
    let _ = writeln!(rep, "hard differences: {}", v.hard.len());
    std::fs::write(root.join("report.txt"), &rep).unwrap();
    eprintln!("{}", rep);
    if std::env::var("BH_SCENARIO_BLESS").as_deref() == Ok("1") {
        let header = format!(
            "scenario {}: noise mask learned from {} runs of the reference server (blockheads-ref:1.7.1)\n\
             regenerate: see README.md in this directory",
            sc.name, runs
        );
        std::fs::create_dir_all(fixture_dir()).unwrap();
        std::fs::write(fixture_dir().join(format!("{}.txt", sc.name)), mask.to_fixture(&header)).unwrap();
        eprintln!("blessed {}", fixture_dir().join(format!("{}.txt", sc.name)).display());
    }
    assert!(
        v.hard.is_empty(),
        "{}: {} hard differences; see {}",
        sc.name,
        v.hard.len(),
        root.join("report.txt").display()
    );
}

/// The docker-free test: our server in-process against the checked-in reference fixture. Keys listed in
/// `<name>.xfail` are known bugs: they must still differ (strict), everything else stable must match.
pub fn fixture_scenario(sc: &Scenario) {
    let fx = fixture_dir().join(format!("{}.txt", sc.name));
    let text =
        std::fs::read_to_string(&fx).unwrap_or_else(|e| panic!("{}: {} (bless it, see README)", fx.display(), e));
    let mask = NoiseMask::from_fixture(&text);
    let xfail = std::fs::read_to_string(fixture_dir().join(format!("{}.xfail", sc.name)))
        .map(|t| parse_xfail(&t))
        .unwrap_or_default();
    let (root, _tmp) = out_root(&format!("{}-host", sc.name));
    let (obs, _save) = await_side(sc, spawn_side(*sc, Side::OursHost, &root, 0));
    let v = mask.check(&obs.map());
    let failing: BTreeSet<&str> = v.hard.iter().map(|l| verdict_key(l)).collect();
    let unexpected: Vec<&String> = v.hard.iter().filter(|l| !xfail.contains_key(verdict_key(l))).collect();
    let fixed: Vec<String> =
        xfail.iter().filter(|(k, _)| !failing.contains(k.as_str())).map(|(k, r)| format!("{}  # {}", k, r)).collect();
    for l in &v.hard {
        eprintln!("{} {}", if xfail.contains_key(verdict_key(l)) { "xfail" } else { "HARD " }, l);
    }
    for l in v.soft.iter().filter(|l| !l.starts_with('~')) {
        eprintln!("drift {}", l);
    }
    assert!(unexpected.is_empty(), "{}: differs from the reference fixture:\n{:#?}", sc.name, unexpected);
    assert!(
        fixed.is_empty(),
        "{}: known bugs now match the reference; remove them from {}.xfail:\n{}",
        sc.name,
        sc.name,
        fixed.join("\n")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(v: &[(&str, &str)]) -> BTreeMap<String, String> {
        v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn obs_keys_and_soft() {
        let mut o = Obs::default();
        o.add("a", 1);
        o.add("a", 2);
        o.soft("b\tc", "x\ny");
        assert_eq!(o.render(), "a\t1\na#2\t2\n~b c\tx y\n");
        assert_eq!(o.map().len(), 3);
    }

    #[test]
    fn mask_learns_and_checks() {
        let mask = NoiseMask::learn(&[
            m(&[("s", "1"), ("v", "1"), ("only1", "x"), ("~c", "5")]),
            m(&[("s", "1"), ("v", "2"), ("~c", "5")]),
        ]);
        assert_eq!(mask.stable, m(&[("s", "1"), ("~c", "5")]));
        assert_eq!(mask.volatile.iter().cloned().collect::<Vec<_>>(), vec!["only1", "v"]);
        let ok = mask.check(&m(&[("s", "1"), ("v", "9"), ("~c", "5")]));
        assert!(ok.hard.is_empty() && ok.soft.is_empty());
        let bad = mask.check(&m(&[("v", "9"), ("~c", "6"), ("new", "1"), ("~new", "2")]));
        assert_eq!(bad.hard, vec!["s: ref `1` ours <missing>", "new: ref <missing> ours `1`"]);
        assert_eq!(bad.soft, vec!["~c: ref `5` ours `6`", "~new: ref <missing> ours `2`"]);
        let bad = mask.check(&m(&[("s", "2"), ("~c", "5")]));
        assert_eq!(bad.hard, vec!["s: ref `1` ours `2`"]);
        assert_eq!(verdict_key(&bad.hard[0]), "s");
        let fx = mask.to_fixture("h1\nh2");
        assert!(fx.starts_with("# h1\n# h2\nS\ts\t1\n"));
        assert_eq!(NoiseMask::from_fixture(&(fx + "junk\n")), mask);
    }

    #[test]
    fn timing_counts_have_a_tolerance() {
        for k in [
            "plain_rx.rx.26",
            "~join.rx.04",
            "walk_left.create.t17",
            "B_join.remove.t52",
            "paint.chest.block_resends",
            "walk.blocks",
            "walk.light_blocks",
            "walk.explored_blocks",
            "save.world_db.blocks.N_N",
            "save.lightBlocks.main.H_N_N",
        ] {
            assert!(timing_count(k), "{k}");
        }
        for k in [
            "a.chest_inventory",
            "save.world_db.dw.N_N/17",
            "save.lightBlocks.main.H_allIndexes",
            "x.create.tx",
            "x.create.t",
            "paint.door.tile_paint",
            "/players",
        ] {
            assert!(!timing_count(k), "{k}");
        }
        assert_eq!((count_tolerance(0), count_tolerance(8), count_tolerance(9), count_tolerance(48)), (2, 2, 3, 12));
        assert!(
            counts_close("0", "2") && !counts_close("0", "3") && counts_close("48", "36") && !counts_close("48", "61")
        );
        assert!(!counts_close("1", "x"));
        let mask = NoiseMask::learn(&[m(&[("p.rx.26", "3"), ("p.create.t17", "48"), ("s.blocks", "5"), ("t", "1")])]);
        let v = mask.check(&m(&[
            ("p.rx.26", "4"),
            ("p.create.t17", "70"),
            ("p.create.t5", "1"),
            ("p.rx.30", "9"),
            ("t", "1"),
        ]));
        assert_eq!(v.soft, vec!["p.rx.26: ref `3` ours `4`", "p.create.t5: ref <missing> ours `1`"]);
        assert_eq!(
            v.hard,
            vec![
                "p.create.t17: ref `48` ours `70`",
                "s.blocks: ref `5` ours <missing>",
                "p.rx.30: ref <missing> ours `9`"
            ]
        );
    }

    #[test]
    fn xfail_and_patterns() {
        let x = parse_xfail("# c\n\nk1  # B3 torches\nk2\n");
        assert_eq!(x.get("k1").map(String::as_str), Some("B3 torches"));
        assert_eq!(x.get("k2").map(String::as_str), Some(""));
        assert_eq!(key_pattern("83_17/chest_124"), "N_N/chest_N");
        assert_eq!(key_pattern("e98c5bee38be74e00ad7748a3dd42119_3_4"), "H_N_N");
        assert_eq!(mask_digits("up 12.5s, 3 MB"), "up #.#s, # MB");
    }

    #[test]
    fn builders() {
        assert_eq!(item_ab(0x413, 2, 3), vec![0x13, 4, 2, 0, 3, 0, 0, 0]);
        let s = slots(3, &[(1, 5, 2)]);
        assert_eq!(slot_summary(&s), "s1:5x2");
        assert_eq!(slot_summary(&slots(2, &[])), "empty(2 slots)");
        assert_eq!(slot_summary(&d(vec![("saveItemSlots", s)])), "s1:5x2");
        assert_eq!(slot_summary(&d(vec![])), "no-saveItemSlots");
        assert_eq!(slot_summary(&PV::int(1)), "not-array");
        let c = chest_blob(&[7u8; 0x50], true, 9, Some(slots(1, &[])), "o");
        assert_eq!((c[0x18], c[0x22], c[0]), (9, 1, 7));
        let b = bh_update(1, (2, 3), (4, 5));
        assert_eq!((b[0], b[8], b[0x20], b[0x24], b.len()), (1, 2, 4, 5, 0x70));
        assert_eq!(fnv(b"").len(), 16);
        assert_eq!(world_args().len(), 8);
        assert_eq!([Side::Ref.tag(), Side::OursBin.tag(), Side::OursHost.tag()], ["ref", "ours", "host"]);
    }
}
