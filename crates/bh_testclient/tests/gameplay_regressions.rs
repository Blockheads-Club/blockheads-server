//! End-to-end gameplay regressions, driven through the real wire protocol:
//!
//! * chests: items put into a chest are still there when the chest is opened again (and survive a reload).
//!
//! Our server runs in-process (`blockheads_server::host`), so these always run. With `BH_PARITY=1` (+ docker and
//! the reference environment) the same flows also run against the reference server and must behave the same.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bh_testclient::harness::refenv::{RefServer, RunSpec, ServerHandle};
use bh_testclient::harness::{free_udp_port, parity_enabled, TempDir};
use bh_testclient::session::BatchKind;
use bh_testclient::{BlockheadSpec, ClientConfig, Identity, JoinOutcome, PlaceInteractionObjectNetRequest, Session};
use blockheads_server::host::{self, HostConfig, Status, WorldSelect};
use blockheads_server::plist::value::PlistValue as PV;
use blockheads_server::protocol::codec;

const SERVER_CHEST_INVENTORY: u8 = 0x32;
const OBJECT_TYPE_BLOCKHEAD: u8 = 24;
const OBJECT_TYPE_CHEST: u8 = 46;
const ITEM_CHEST: u16 = 0x413;
const ITEM_DIRT: u16 = 1048;

// ------------------------------------------------------------------------------------------------ servers

/// Our server on its own thread, stopped on drop.
struct Ours {
    h: Option<host::ServerHandle>,
    port: u16,
    dir: Option<TempDir>,
}

impl Ours {
    fn start(tag: &str, data: Option<TempDir>) -> Ours {
        let dir = data.unwrap_or_else(|| TempDir::new(tag).unwrap());
        let root = dir.path().join("TheBlockheads");
        let world = match host::list_worlds(Some(&root)).into_iter().next() {
            Some(w) => WorldSelect::Load(w.id),
            None => WorldSelect::Load(host::create_world(Some(&root), "REGRESS", 1, false).expect("create").id),
        };
        let port = free_udp_port();
        let lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let l2 = lines.clone();
        let cfg = HostConfig { data_dir: Some(root), world, port: Some(port), save_delay: 1, ..HostConfig::default() };
        let h = host::start(cfg, Arc::new(move |s| l2.lock().unwrap().push(s.to_string()))).unwrap();
        let end = Instant::now() + Duration::from_secs(120);
        while h.status() != Status::Running && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(h.status(), Status::Running, "{:#?}", lines.lock().unwrap());
        Ours { h: Some(h), port, dir: Some(dir) }
    }
    /// Stop the server and hand back its data directory.
    fn stop(mut self) -> TempDir {
        if let Some(mut h) = self.h.take() {
            assert_eq!(h.stop(), Status::Stopped(0));
        }
        self.dir.take().unwrap()
    }
}

impl Drop for Ours {
    fn drop(&mut self) {
        if let Some(mut h) = self.h.take() {
            let _ = h.stop();
        }
    }
}

fn reference(tag: &str) -> RefServer {
    let args = ["--new", "REGRESS", "--world_width", "1"].iter().map(|s| s.to_string()).collect();
    let mut r = RefServer::start(&RunSpec { tag: tag.into(), args, ..Default::default() }).unwrap();
    r.wait_for_log("World load complete.", Duration::from_secs(120)).expect("reference load");
    r
}

fn addr(port: u16) -> SocketAddr {
    format!("127.0.0.1:{}", port).parse().unwrap()
}

// ------------------------------------------------------------------------------------------------ client helpers

fn join(port: u16, alias: &str) -> Session {
    let cfg = ClientConfig { addr: addr(port), ..Default::default() };
    let mut s = Session::connect(cfg, Identity::new(alias)).expect("connect");
    assert!(matches!(s.join(10_000).unwrap(), JoinOutcome::Approved));
    s.request_initial_data().unwrap();
    s.wait_for(0x06, 10_000).unwrap().expect("initial data");
    s
}

fn summary_int(s: &Session, key: &str) -> i32 {
    s.state.summary.as_ref().and_then(|p| p.object_for_key(key)).map(|v| v.int_value()).expect(key)
}

/// The start portal, from the world summary.
fn portal(s: &Session) -> (i32, i32) {
    (summary_int(s, "startPortalPos.x"), summary_int(s, "startPortalPos.y"))
}

/// Request the macro block holding a tile and wait for it.
fn load_block_at(s: &mut Session, x: i32, y: i32) {
    let i = s.macro_index_for_tile(x, y);
    s.request_block(i, true).unwrap();
    s.wait_for_matching(10_000, &mut |rx| {
        rx.data.first() == Some(&0x04) && rx.data.get(1..5) == Some(&(i as u32).to_le_bytes()[..])
    })
    .unwrap()
    .unwrap_or_else(|| panic!("block {} not sent", i));
}

/// `(type_index, contents, foreground_contents)` of a tile from a received block.
fn tile(s: &Session, x: i32, y: i32) -> Option<(u8, u8, u8)> {
    let b = s.state.block_msgs.get(&(s.macro_index_for_tile(x, y) as u32))?;
    let off = (((y % 32) * 32 + (x % 32)) * 64) as usize;
    let t = b.tiles.get(off..off + 64)?;
    Some((t[0], t[3], t[0xb]))
}

/// An empty air tile standing on solid ground, a few tiles east of `(x, y)` (both in loaded blocks).
fn free_spot_near(s: &Session, x: i32, y: i32) -> (i32, i32) {
    const AIR: u8 = 2;
    for cx in x + 3..x + 24 {
        for cy in (y - 8..y + 8).rev() {
            let (Some(here), Some(below)) = (tile(s, cx, cy), tile(s, cx, cy - 1)) else { continue };
            if here == (AIR, 0, 0) && ![AIR, 3, 5].contains(&below.0) && below.1 == 0 && below.2 == 0 {
                return (cx, cy);
            }
        }
    }
    panic!("no free spot near {},{}", x, y);
}

/// Creates a blockhead at `(x, y)` and returns its id.
fn spawn_blockhead(s: &mut Session, x: i32, y: i32) -> u64 {
    load_block_at(s, x, y);
    s.run_for(500).unwrap();
    s.request_unique_id().unwrap();
    s.create_blockhead(&BlockheadSpec::new("Bob", x as u32, y as u32)).unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    while Instant::now() < end {
        if let Some(c) = s.state.created.iter().find(|c| c.object_type == OBJECT_TYPE_BLOCKHEAD) {
            return c.unique_id;
        }
        s.run_for(100).unwrap();
    }
    panic!("blockhead was not created");
}

/// One inventory item's `saveData` (no extra dictionary).
fn item(ty: u16) -> Vec<u8> {
    let mut v = ty.to_le_bytes().to_vec();
    v.extend_from_slice(&[0; 6]);
    v
}

/// The id of the first chest the server announced.
fn wait_for_chest(s: &mut Session) -> (u64, Vec<u8>) {
    let end = Instant::now() + Duration::from_secs(10);
    while Instant::now() < end {
        if let Some(((_, id), blob)) = s.state.objects_seen.iter().find(|((t, _), _)| *t == OBJECT_TYPE_CHEST) {
            return (*id, blob.clone());
        }
        s.run_for(100).unwrap();
    }
    panic!("chest was not created");
}

/// `ChestNetData` (0x40) of `chest_blob` with the use state replaced, followed by a gzip plist (`slots` = the
/// `saveItemSlots` to upload, if any).
fn chest_update(chest_blob: &[u8], user: u64, in_use: bool, slots: Option<Vec<Vec<Vec<u8>>>>) -> Vec<u8> {
    let mut b = chest_blob[..0x40].to_vec();
    b[0x18..0x20].copy_from_slice(&(if in_use { user } else { 0 }).to_le_bytes());
    b[0x22] = in_use as u8;
    let mut d = PV::mutable_dict();
    if let (PV::Dict(dd), Some(slots)) = (&mut d, slots) {
        let arr = slots.into_iter().map(|s| PV::array(s.into_iter().map(PV::data).collect())).collect();
        dd.map.insert("saveItemSlots".into(), PV::array(arr));
    }
    b.extend(codec::plist_gz(&d).unwrap());
    b
}

/// Opens the chest and returns the inventory part of the server's 0x32 answer (empty = no inventory).
fn open_chest(s: &mut Session, chest: u64, blob: &[u8], user: u64) -> Vec<u8> {
    s.send_object_batch(BatchKind::Update, OBJECT_TYPE_CHEST, &[chest_update(blob, user, true, None)]).unwrap();
    let rx = s
        .wait_for_matching(10_000, &mut |rx| {
            rx.data.first() == Some(&SERVER_CHEST_INVENTORY) && rx.data.get(1..9) == Some(&chest.to_le_bytes()[..])
        })
        .unwrap();
    let rx = rx.expect("no chest inventory (0x32) after opening the chest");
    rx.data[9..].to_vec()
}

fn close_chest(s: &mut Session, blob: &[u8], user: u64) {
    s.send_object_batch(BatchKind::Update, OBJECT_TYPE_CHEST, &[chest_update(blob, user, false, None)]).unwrap();
    s.run_for(300).unwrap();
}

/// Item types per slot of a 0x32 inventory (`gzip(plist [[saveData]])`).
fn inventory_types(inv: &[u8]) -> Vec<Vec<u16>> {
    let Some(PV::Array(slots)) = codec::gz_plist_from(inv) else { return Vec::new() };
    slots
        .items
        .iter()
        .map(|s| match s {
            PV::Array(a) => a
                .items
                .iter()
                .filter_map(|i| match i {
                    PV::Data(d) if d.bytes.len() >= 2 => Some(u16::from_le_bytes([d.bytes[0], d.bytes[1]])),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        })
        .collect()
}

// ------------------------------------------------------------------------------------------------ flows

/// Place a chest, open it, put dirt into slot 0, close it, open it again. Returns
/// the item types per slot on the first and on the second open.
fn chest_flow(s: &mut Session) -> (Vec<Vec<u16>>, Vec<Vec<u16>>) {
    let (px, py) = portal(s);
    let bh = spawn_blockhead(s, px, py);
    let (cx, cy) = free_spot_near(s, px, py);
    let mut tail = PV::mutable_dict();
    if let PV::Dict(d) = &mut tail {
        d.map.insert("item".into(), PV::data(item(ITEM_CHEST)));
    }
    s.place_interaction_object(PlaceInteractionObjectNetRequest { pos_x: cx as u32, pos_y: cy as u32 }, &tail).unwrap();
    let (chest, blob) = wait_for_chest(s);
    let first = inventory_types(&open_chest(s, chest, &blob, bh));
    let mut slots = vec![Vec::new(); 16];
    slots[0] = vec![item(ITEM_DIRT), item(ITEM_DIRT)];
    s.send_object_batch(BatchKind::Update, OBJECT_TYPE_CHEST, &[chest_update(&blob, bh, true, Some(slots))]).unwrap();
    s.run_for(300).unwrap();
    close_chest(s, &blob, bh);
    let second = inventory_types(&open_chest(s, chest, &blob, bh));
    close_chest(s, &blob, bh);
    (first, second)
}

// ------------------------------------------------------------------------------------------------ tests

#[test]
fn chest_contents_survive_closing_and_reopening() {
    let o = Ours::start("chest", None);
    let mut s = join(o.port, "CHESTER");
    let (first, second) = chest_flow(&mut s);
    assert!(first.iter().all(|x| x.is_empty()), "new chest not empty: {:?}", first);
    assert_eq!(second.first(), Some(&vec![ITEM_DIRT, ITEM_DIRT]), "chest lost its items: {:?}", second);
    s.disconnect();

    // ... and after a restart (the inventory is saved under `<x>_<y>/chest_<id>`)
    let data = o.stop();
    let o = Ours::start("chest2", Some(data));
    let mut s = join(o.port, "CHESTER");
    let (px, py) = portal(&s);
    load_block_at(&mut s, px, py);
    let (cx, cy) = free_spot_near(&s, px, py);
    load_block_at(&mut s, cx, cy); // the chest's block (may be the portal's)
    let (chest, blob) = wait_for_chest(&mut s);
    let bh = spawn_blockhead(&mut s, px, py);
    let again = inventory_types(&open_chest(&mut s, chest, &blob, bh));
    assert_eq!(again.first(), Some(&vec![ITEM_DIRT, ITEM_DIRT]), "chest lost its items on reload: {:?}", again);
    s.disconnect();
}

#[test]
fn parity_chest_matches_reference() {
    if !parity_enabled() {
        eprintln!("SKIP: parity_chest_matches_reference (BH_PARITY=1 + docker required)");
        return;
    }
    let r = reference("chest");
    let mut s = join(r.port(), "CHESTER");
    let (first, second) = chest_flow(&mut s);
    s.disconnect();
    assert!(first.iter().all(|x| x.is_empty()), "reference: {:?}", first);
    assert_eq!(second.first(), Some(&vec![ITEM_DIRT, ITEM_DIRT]), "reference: {:?}", second);
}
