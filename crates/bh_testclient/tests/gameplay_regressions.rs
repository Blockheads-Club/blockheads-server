//! End-to-end gameplay regressions, driven through the real wire protocol:
//!
//! * chests: items put into a chest are still there when the chest is opened again (and survive a reload);
//! * walking: a client's blockhead walking into a far part of the world gets that area loaded and explored (light
//!   light is sent), instead of it staying dark.
//!
//! * cave torches: walking past a worldgen torch "promise" tile turns it into a torch (and the torch is saved);
//! * glow blocks: glowing tiles (e.g. ore that glows) light their surroundings once a blockhead comes near;
//!
//! * placed objects: torches, eggs, paintings (their image is served by 0x30 / 0x31), rails and hand cars placed
//!   through fill requests exist (and survive a restart); a gate cannot be removed by its non-admin owner;
//! * found-item lists (0x4c) are kept for the player and handed back when the player joins again;
//!
//! Our server runs in-process (`blockheads_server::host`), so these always run. With `BH_PARITY=1` (+ docker and
//! the reference environment) the same flows also run against the reference server and must behave the same.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bh_testclient::harness::refenv::{RefServer, RunSpec, ServerHandle};
use bh_testclient::harness::{free_udp_port, parity_enabled, TempDir};
use bh_testclient::session::BatchKind;
use bh_testclient::{
    BlockheadSpec, ClientConfig, FillNetRequest, Identity, JoinOutcome, PaintNetRequest,
    PlaceInteractionObjectNetRequest, Session,
};
use blockheads_server::host::{self, HostConfig, Status, WorldSelect};
use blockheads_server::plist::value::PlistValue as PV;
use blockheads_server::protocol::{block_wire, codec};

const SERVER_CHEST_INVENTORY: u8 = 0x32;
const SERVER_LIGHT_BLOCK: u8 = 0x28;
const OBJECT_TYPE_BLOCKHEAD: u8 = 24;
const OBJECT_TYPE_CHEST: u8 = 46;
const ITEM_CHEST: u16 = 0x413;
const ITEM_DIRT: u16 = 1048;
const OBJECT_TYPE_TORCH: u8 = 17;
const OBJECT_TYPE_DOOR: u8 = 20;
const OBJECT_TYPE_EGG: u8 = 30;
const OBJECT_TYPE_RAIL: u8 = 40;
const OBJECT_TYPE_HAND_CAR: u8 = 41;
const OBJECT_TYPE_PAINTING: u8 = 52;
const ITEM_TORCH: u16 = 17;
const ITEM_DOOR: u16 = 52;
const ITEM_EGG: u16 = 91;
const ITEM_RAIL: u16 = 199;
const ITEM_HAND_CAR: u16 = 204;
const ITEM_PAINTING: u16 = 212;
const ITEM_GATE: u16 = 304;
const ITEM_COLUMN: u16 = 222;
const ITEM_BOAT: u16 = 111;
const ITEM_WATER: u16 = 1044;
const OBJECT_TYPE_COLUMN: u8 = 53;
const OBJECT_TYPE_BOAT: u8 = 32;
const CLIENT_REQUEST_PAINTING_DATA: u8 = 0x30;
const SERVER_PAINTING_DATA: u8 = 0x31;
/// A valid 1x1 PNG (the server checks paintings with `pnginfo` when it is installed).
const PNG_1X1: [u8; 67] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49,
    0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00,
    0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];
/// A fixed world (the same as the differential harness): torch promises and glowing ore near the portal.
const SEEDED_RULES: &str = r#"{"seed":"custom","custom_seed_value":"12345"}"#;

// ------------------------------------------------------------------------------------------------ servers

/// Our server on its own thread, stopped on drop.
struct Ours {
    h: Option<host::ServerHandle>,
    port: u16,
    dir: Option<TempDir>,
}

impl Ours {
    fn start(tag: &str, data: Option<TempDir>) -> Ours {
        Ours::start_with(tag, data, None)
    }
    /// A new world of width 1/4 made with `SEEDED_RULES` (or the existing world in `data`).
    fn start_seeded(tag: &str, data: Option<TempDir>) -> Ours {
        Ours::start_with(tag, data, Some(SEEDED_RULES))
    }
    fn start_with(tag: &str, data: Option<TempDir>, rules: Option<&str>) -> Ours {
        let dir = data.unwrap_or_else(|| TempDir::new(tag).unwrap());
        let root = dir.path().join("TheBlockheads");
        let (world, custom_rules_json) = match host::list_worlds(Some(&root)).into_iter().next() {
            Some(w) => (WorldSelect::Load(w.id), None),
            None => match rules {
                Some(r) => (WorldSelect::New { name: "REGRESS".into(), width_multiplier: 104 }, Some(r.to_string())),
                None => {
                    (WorldSelect::Load(host::create_world(Some(&root), "REGRESS", 1, false).expect("create").id), None)
                }
            },
        };
        let port = free_udp_port();
        let lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let l2 = lines.clone();
        let cfg = HostConfig {
            data_dir: Some(root),
            world,
            port: Some(port),
            save_delay: 1,
            custom_rules_json,
            ..HostConfig::default()
        };
        let echo = std::env::var_os("BH_REGRESS_LOG").is_some();
        let h = host::start(
            cfg,
            Arc::new(move |s| {
                if echo {
                    eprintln!("server: {}", s);
                }
                l2.lock().unwrap().push(s.to_string())
            }),
        )
        .unwrap();
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

/// The reference server on the `SEEDED_RULES` world.
fn reference_seeded(tag: &str) -> RefServer {
    let args = ["--new", "REGRESS", "--world_width", "1/4", "--custom-rules", SEEDED_RULES]
        .iter()
        .map(|s| s.to_string())
        .collect();
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

/// Joins again after a disconnect: the reference server takes a while to drop the old connection of the same player,
/// so a few attempts are made.
fn rejoin(port: u16, alias: &str) -> Session {
    for _ in 0..5 {
        std::thread::sleep(Duration::from_secs(2));
        let cfg = ClientConfig { addr: addr(port), ..Default::default() };
        let Ok(mut s) = Session::connect(cfg, Identity::new(alias)) else { continue };
        let j = s.join(10_000);
        if !matches!(j, Ok(JoinOutcome::Approved)) {
            eprintln!("rejoin {}: {:?}", alias, j.map(|o| format!("{:?}", o)));
            continue;
        }
        s.request_initial_data().unwrap();
        if s.wait_for(0x06, 10_000).ok().flatten().is_some() {
            return s;
        }
        s.disconnect();
    }
    panic!("{} could not join again", alias);
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

/// `BlockheadUpdateData` (0x70 bytes) of a blockhead standing on / walking to `(x, y)`.
fn blockhead_update(id: u64, x: i32, y: i32) -> Vec<u8> {
    let mut b = vec![0u8; 0x70];
    b[0..8].copy_from_slice(&id.to_le_bytes());
    for off in [0x08, 0x18, 0x20, 0x28] {
        b[off..off + 4].copy_from_slice(&(x as u32).to_le_bytes());
        b[off + 4..off + 8].copy_from_slice(&(y as u32).to_le_bytes());
    }
    b[0x33] = 127; // happiness
    b[0x3c] = 1; // isCurrentlyActive
    b[0x3f] = 127; // health
    b
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

/// The newest light data (the client's explored light block) the server sent for a macro block: from a 0x04 block
/// or a 0x28 light block, whichever came last. `None` if neither was received.
fn latest_light(s: &Session, macro_index: u32) -> Option<Vec<u8>> {
    let mut light = None;
    for rx in s.received() {
        match rx.data.first() {
            Some(0x04) => {
                if let Some(b) = block_wire::decode_server_block(&rx.data[1..]) {
                    if b.macro_index == macro_index {
                        light = Some(b.light);
                    }
                }
            }
            Some(&SERVER_LIGHT_BLOCK) => {
                if let Some(l) = block_wire::decode_light_block(&rx.data[1..]) {
                    if l.macro_index == macro_index {
                        light = Some(l.light);
                    }
                }
            }
            _ => {}
        }
    }
    light
}

/// Like a client whose view follows the blockhead: request the macro blocks along the way, then walk the blockhead
/// `tiles` tiles east of the portal, one tile per update. Returns the explored light of the destination block
/// before and after the walk.
fn walk_flow(s: &mut Session, tiles: i32) -> (Vec<u8>, Vec<u8>) {
    let (px, py) = portal(s);
    let bh = spawn_blockhead(s, px, py);
    let dest = s.macro_index_for_tile(px + tiles, py) as u32;
    let mut want = Vec::new();
    for x in (px..=px + tiles + 32).step_by(32) {
        for y in [py - 32, py, py + 32] {
            want.push(s.macro_index_for_tile(x, y));
        }
    }
    s.request_blocks_throttled(&want, true, 30_000).unwrap();
    s.run_for(1000).unwrap();
    let before = latest_light(s, dest).expect("destination block not sent");
    for dx in 1..=tiles {
        s.send_object_batch(BatchKind::Update, OBJECT_TYPE_BLOCKHEAD, &[blockhead_update(bh, px + dx, py)]).unwrap();
        s.run_for(60).unwrap();
    }
    s.run_for(3000).unwrap();
    (before, latest_light(s, dest).expect("destination block not sent"))
}

fn explored(light: &[u8]) -> usize {
    light.iter().filter(|b| **b != 0).count()
}

const WALK: i32 = 80;

/// Requests every macro block of the tile rectangle `x0..=x1` x `y0..=y1` (clamped at the bottom of the world).
fn request_area(s: &mut Session, x0: i32, x1: i32, y0: i32, y1: i32) {
    let mut want = Vec::new();
    for x in (x0..=x1 + 31).step_by(32).map(|x| x.min(x1)) {
        for y in (y0.max(0)..=y1 + 31).step_by(32).map(|y| y.min(y1)) {
            let i = s.macro_index_for_tile(x, y);
            if !want.contains(&i) {
                want.push(i);
            }
        }
    }
    s.request_blocks_throttled(&want, true, 60_000).unwrap();
    s.run_for(1000).unwrap();
}

/// World positions of the received tiles whose 64 bytes satisfy `pred`.
fn find_tiles(s: &Session, pred: impl Fn(&[u8]) -> bool) -> Vec<(i32, i32)> {
    let w = s.state.world_width_macro;
    let mut out = Vec::new();
    for (idx, b) in &s.state.block_msgs {
        for (i, t) in b.tiles.chunks(64).enumerate() {
            if pred(t) {
                let (bx, by) = ((*idx as i32 % w) * 32, (*idx as i32 / w) * 32);
                out.push((bx + (i % 32) as i32, by + (i / 32) as i32));
            }
        }
    }
    out
}

/// The tile nearest to `(x, y)` (Manhattan distance; ties: lowest position).
fn nearest(tiles: &[(i32, i32)], x: i32, y: i32) -> Option<(i32, i32)> {
    tiles.iter().copied().min_by_key(|(tx, ty)| ((tx - x).abs() + (ty - y).abs(), *tx, *ty))
}

/// Walks a client's blockhead from `from` to `to` in adjacent steps (x first, then y), requesting the blocks around
/// it every few steps like a client whose view follows it.
fn walk_to(s: &mut Session, bh: u64, from: (i32, i32), to: (i32, i32)) {
    let (mut x, mut y) = from;
    let mut n = 0;
    while (x, y) != to {
        if x != to.0 {
            x += (to.0 - x).signum();
        } else {
            y += (to.1 - y).signum();
        }
        s.send_object_batch(BatchKind::Update, OBJECT_TYPE_BLOCKHEAD, &[blockhead_update(bh, x, y)]).unwrap();
        if n % 8 == 0 {
            let want: Vec<i32> =
                [(-32, -32), (0, -32), (32, -32), (-32, 0), (0, 0), (32, 0), (-32, 32), (0, 32), (32, 32)]
                    .iter()
                    .filter(|(_, dy)| y + dy >= 0)
                    .map(|(dx, dy)| s.macro_index_for_tile(x + dx, y + dy))
                    .collect();
            s.request_blocks_throttled(&want, true, 20_000).unwrap();
        }
        n += 1;
        s.run_for(40).unwrap();
    }
    s.run_for(3000).unwrap();
}

/// Positions of the objects of `object_type` the server announced (`DynamicObjectNetData.pos` at 8 / 12).
fn object_positions(s: &Session, object_type: u8) -> Vec<(i32, i32)> {
    s.state
        .objects_seen
        .iter()
        .filter(|((t, _), b)| *t == object_type && b.len() >= 16)
        .map(|(_, b)| {
            (
                u32::from_le_bytes(b[8..12].try_into().unwrap()) as i32,
                u32::from_le_bytes(b[12..16].try_into().unwrap()) as i32,
            )
        })
        .collect()
}

/// A worldgen torch promise (foreground contents 0x34 .. 0x3c, even) near the portal.
fn is_torch_promise(t: &[u8]) -> bool {
    matches!(t[0xb], 0x34 | 0x36 | 0x38 | 0x3a | 0x3c)
}

/// Walks a blockhead to the torch promise nearest the portal. Returns the promise's position.
fn torch_flow(s: &mut Session) -> (i32, i32) {
    let (px, py) = portal(s);
    let bh = spawn_blockhead(s, px, py);
    request_area(s, px - 128, px + 128, py - 32, py + 32);
    let target = nearest(&find_tiles(s, is_torch_promise), px, py).expect("no torch promise near the portal");
    walk_to(s, bh, (px, py), target);
    target
}

/// A tile that needs a glow block: glowing ore (contents 0x4d) or a glowing block type.
fn is_glowing(t: &[u8]) -> bool {
    t[3] == 0x4d || matches!(t[0], 0x10 | 0x1a | 0x4d | 71..=75)
}

/// `artificialLightR` (u16 @ 0x0e) of the tile at `(x, y)` in the latest copy of its block.
fn artificial_light_r(s: &Session, x: i32, y: i32) -> u16 {
    let b = s.state.block_msgs.get(&(s.macro_index_for_tile(x, y) as u32)).expect("block of the tile");
    let off = (((y & 31) * 32 + (x & 31)) * 64) as usize;
    u16::from_le_bytes([b.tiles[off + 0xe], b.tiles[off + 0xf]])
}

/// Walks a blockhead next to the glowing tile nearest the portal, then fetches its block again. Returns the glowing
/// tile and its artificial red light before and after.
fn glow_flow(s: &mut Session) -> ((i32, i32), u16, u16) {
    let (px, py) = portal(s);
    let bh = spawn_blockhead(s, px, py);
    request_area(s, px - 96, px + 96, py - 224, py);
    let target = nearest(&find_tiles(s, is_glowing), px, py).expect("no glowing tile near the portal");
    let before = artificial_light_r(s, target.0, target.1);
    walk_to(s, bh, (px, py), (target.0, target.1 + 1));
    let i = s.macro_index_for_tile(target.0, target.1);
    s.request_block(i, true).unwrap();
    s.run_for(2000).unwrap();
    (target, before, artificial_light_r(s, target.0, target.1))
}

/// Air tiles standing on solid ground 6 to 30 columns from the portal (not under the portal itself, which blocks
/// placing), nearest first, at least 2 columns apart.
fn ground_spots(s: &mut Session, n: usize) -> Vec<(i32, i32)> {
    let (px, py) = portal(s);
    request_area(s, px - 40, px + 40, py - 32, py + 32);
    let mut all: Vec<(i32, i32)> = find_tiles(s, |_| true)
        .into_iter()
        .filter(|(x, y)| (x - px).abs() <= 30 && (6..=30).contains(&(x - px).abs()))
        .filter(|(x, y)| {
            let (Some(here), Some(below)) = (tile(s, *x, *y), tile(s, *x, *y - 1)) else { return false };
            here == (2, 0, 0) && ![2, 3, 5].contains(&below.0) && below.1 == 0 && below.2 == 0
        })
        .collect();
    all.sort_by_key(|(x, y)| ((x - px).abs() + (y - py).abs(), *x, *y));
    let mut out: Vec<(i32, i32)> = Vec::new();
    for p in all {
        if out.iter().all(|q| (q.0 - p.0).abs() >= 2) {
            out.push(p);
        }
        if out.len() == n {
            return out;
        }
    }
    panic!("not enough free ground near the portal: {:?}", out);
}

fn fill(s: &mut Session, (x, y): (i32, i32), ty: u16, tail: Option<&PV>) {
    let r = FillNetRequest { pos_x: x as u32, pos_y: y as u32, type_: ty, ..Default::default() };
    s.fill(r, tail).unwrap();
    s.run_for(300).unwrap();
}

/// The id of the object of `object_type` announced at `pos`.
fn object_at(s: &Session, object_type: u8, pos: (i32, i32)) -> Option<u64> {
    s.state.objects_seen.iter().find_map(|((t, id), b)| {
        let at = (
            u32::from_le_bytes(b[8..12].try_into().ok()?) as i32,
            u32::from_le_bytes(b[12..16].try_into().ok()?) as i32,
        );
        (*t == object_type && at == pos).then_some(*id)
    })
}

/// 0x30 for a painting; the image of the server's 0x31 answer (`None`: no answer).
fn painting_image(s: &mut Session, id: u64) -> Option<Vec<u8>> {
    let mut p = vec![CLIENT_REQUEST_PAINTING_DATA];
    p.extend_from_slice(&id.to_le_bytes());
    s.send_raw(&p).unwrap();
    let rx = s
        .wait_for_matching(5_000, &mut |rx| {
            rx.data.first() == Some(&SERVER_PAINTING_DATA) && rx.data.get(1..9) == Some(&id.to_le_bytes()[..])
        })
        .unwrap()?;
    Some(rx.data[9..].to_vec())
}

/// Where the placement flow put each object.
struct Placed {
    torch: (i32, i32),
    egg: (i32, i32),
    painting: (i32, i32),
    rail: (i32, i32),
}

/// Places a torch, an egg, a painting (with an image) and a rail with a hand car on it through fill requests.
fn place_flow(s: &mut Session) -> Placed {
    let spots = ground_spots(s, 3);
    // a painting hangs on a back wall (without one it drops as a free block)
    let (px, py) = portal(s);
    let walls = find_tiles(s, |t| t[0] == 2 && t[1] != 2 && t[3] == 0 && t[0xb] == 0 && t[0xc] == 0);
    let wall = nearest(&walls, px, py).expect("no free tile with a back wall near the portal");
    let p = Placed { torch: spots[0], egg: spots[1], painting: wall, rail: spots[2] };
    fill(s, p.torch, ITEM_TORCH, None);
    fill(s, p.egg, ITEM_EGG, None);
    let mut sd = PV::mutable_dict();
    if let PV::Dict(d) = &mut sd {
        d.map.insert("outputImageData".into(), PV::data(PNG_1X1.to_vec()));
    }
    let mut tail = PV::mutable_dict();
    if let PV::Dict(d) = &mut tail {
        d.map.insert("placeSaveDict".into(), sd);
    }
    fill(s, p.painting, ITEM_PAINTING, Some(&tail));
    fill(s, p.rail, ITEM_RAIL, None);
    // the hand car goes on the tile above the rail (it snaps down onto it)
    fill(s, (p.rail.0, p.rail.1 + 1), ITEM_HAND_CAR, None);
    s.run_for(1500).unwrap();
    p
}

/// Every placed object exists (`ctx` names the phase); the painting serves its image.
fn check_placed(s: &mut Session, p: &Placed, ctx: &str) {
    for (t, pos, what) in [
        (OBJECT_TYPE_TORCH, p.torch, "torch"),
        (OBJECT_TYPE_EGG, p.egg, "egg"),
        (OBJECT_TYPE_PAINTING, p.painting, "painting"),
        (OBJECT_TYPE_RAIL, p.rail, "rail"),
    ] {
        assert!(object_at(s, t, pos).is_some(), "{}: no {} at {:?}; seen {:?}", ctx, what, pos, object_positions(s, t));
    }
    assert!(
        s.state.objects_seen.keys().any(|(t, _)| *t == OBJECT_TYPE_HAND_CAR),
        "{}: no hand car on the rail at {:?}; objects {:?}",
        ctx,
        p.rail,
        s.state.objects_seen.keys().map(|k| k.0).collect::<std::collections::BTreeSet<_>>()
    );
    let id = object_at(s, OBJECT_TYPE_PAINTING, p.painting).unwrap();
    assert_eq!(painting_image(s, id).as_deref(), Some(&PNG_1X1[..]), "{}: painting image not served", ctx);
}

/// Places a gate and a door; another player watches while the (non-admin) owner asks to remove both. Returns
/// whether the gate and the door are still there for the watcher.
fn gate_flow(a: &mut Session, b: &mut Session) -> (bool, bool) {
    let spots = ground_spots(a, 2);
    fill(a, spots[0], ITEM_GATE, None);
    fill(a, spots[1], ITEM_DOOR, None);
    a.run_for(1000).unwrap();
    let gate = object_at(a, OBJECT_TYPE_DOOR, spots[0]).expect("gate not placed");
    let door = object_at(a, OBJECT_TYPE_DOOR, spots[1]).expect("door not placed");
    let (px, py) = portal(b);
    request_area(b, px - 40, px + 40, py - 32, py + 32);
    b.run_for(1000).unwrap();
    assert!(b.state.objects_seen.contains_key(&(OBJECT_TYPE_DOOR, gate)), "watcher does not see the gate");
    let blobs: Vec<Vec<u8>> =
        [gate, door].iter().map(|id| a.state.objects_seen[&(OBJECT_TYPE_DOOR, *id)][..0x18].to_vec()).collect();
    a.send_object_batch(BatchKind::Remove, OBJECT_TYPE_DOOR, &blobs).unwrap();
    a.run_for(500).unwrap();
    b.run_for(2000).unwrap();
    (
        b.state.objects_seen.contains_key(&(OBJECT_TYPE_DOOR, gate)),
        b.state.objects_seen.contains_key(&(OBJECT_TYPE_DOOR, door)),
    )
}

/// The found-item list in the player's initial data (`foundItems_v2`, gzip of the index-set serialisation).
fn found_items(s: &Session) -> Option<Vec<u8>> {
    let v = s.state.initial_plist.as_ref()?.object_for_key("foundItems_v2")?;
    match v {
        PV::Data(d) => blockheads_server::foundation::gzip::gzip_inflate(&d.bytes),
        _ => None,
    }
}

/// Places a column, then paints it (0x19). Returns the column's net data before and after the paint.
fn object_paint_flow(s: &mut Session) -> (Vec<u8>, Vec<u8>) {
    let spot = ground_spots(s, 1)[0];
    fill(s, spot, ITEM_COLUMN, None);
    s.run_for(1000).unwrap();
    let id = object_at(s, OBJECT_TYPE_COLUMN, spot).expect("column not placed");
    let before = s.state.objects_seen[&(OBJECT_TYPE_COLUMN, id)].clone();
    let r = PaintNetRequest {
        pos_x: spot.0 as u32,
        pos_y: spot.1 as u32,
        color_index: 5,
        face_index: 0,
        ..Default::default()
    };
    s.paint(r).unwrap();
    s.run_for(2000).unwrap();
    (before, s.state.objects_seen[&(OBJECT_TYPE_COLUMN, id)].clone())
}

/// Pours a bucket of water on the ground and puts a boat on it. Returns where.
fn boat_flow(s: &mut Session) -> (i32, i32) {
    let spot = ground_spots(s, 1)[0];
    fill(s, spot, ITEM_WATER, None);
    fill(s, spot, ITEM_BOAT, None);
    s.run_for(1500).unwrap();
    spot
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
fn walking_into_a_new_area_loads_and_explores_it() {
    let o = Ours::start("walk", None);
    let mut s = join(o.port, "WALKER");
    let (before, after) = walk_flow(&mut s, WALK);
    assert_eq!(explored(&before), 0, "destination already explored before the walk");
    assert!(explored(&after) > 0, "the area walked into was never explored (stays dark)");
    s.disconnect();
}

#[test]
fn walking_past_a_torch_promise_makes_a_saved_torch() {
    let o = Ours::start_seeded("torch", None);
    let mut s = join(o.port, "TORCHER");
    let target = torch_flow(&mut s);
    assert!(
        object_positions(&s, OBJECT_TYPE_TORCH).contains(&target),
        "no torch at the promise {:?}: {:?}",
        target,
        object_positions(&s, OBJECT_TYPE_TORCH)
    );
    s.disconnect();

    // the torch is saved with its macro block (`dw/<x>_<y>/17`) and comes back after a restart
    let data = o.stop();
    let o = Ours::start_seeded("torch2", Some(data));
    let mut s = join(o.port, "TORCHER");
    let (px, py) = portal(&s);
    let bh = spawn_blockhead(&mut s, px, py);
    walk_to(&mut s, bh, (px, py), target);
    assert!(object_positions(&s, OBJECT_TYPE_TORCH).contains(&target), "torch lost on reload");
    s.disconnect();
}

#[test]
fn glowing_tiles_light_up_once_a_blockhead_is_near() {
    let o = Ours::start_seeded("glow", None);
    let mut s = join(o.port, "GLOWER");
    let (tile, before, after) = glow_flow(&mut s);
    assert_eq!(before, 0, "glowing tile {:?} lit before anybody came near", tile);
    assert!(after > 0, "glowing tile {:?} gives no light (no glow block)", tile);
    s.disconnect();
}

#[test]
fn parity_torch_and_glow_match_reference() {
    if !parity_enabled() {
        eprintln!("SKIP: parity_torch_and_glow_match_reference (BH_PARITY=1 + docker required)");
        return;
    }
    let r = reference_seeded("torch");
    let mut s = join(r.port(), "TORCHER");
    let target = torch_flow(&mut s);
    let torches = object_positions(&s, OBJECT_TYPE_TORCH);
    s.disconnect();
    assert!(torches.contains(&target), "reference: no torch at {:?}: {:?}", target, torches);
    drop(r);
    let r = reference_seeded("glow");
    let mut s = join(r.port(), "GLOWER");
    let (tile, before, after) = glow_flow(&mut s);
    s.disconnect();
    assert_eq!(before, 0, "reference: {:?}", tile);
    assert!(after > 0, "reference: glowing tile {:?} gives no light", tile);
}

#[test]
fn placed_torch_egg_painting_rail_and_hand_car_exist_and_survive_a_restart() {
    let o = Ours::start_seeded("place", None);
    let mut s = join(o.port, "PLACER");
    let p = place_flow(&mut s);
    check_placed(&mut s, &p, "placed");
    s.disconnect();
    let data = o.stop();
    let o = Ours::start_seeded("place2", Some(data));
    let mut s = join(o.port, "PLACER");
    let (px, py) = portal(&s);
    request_area(&mut s, px - 40, px + 40, py - 32, py + 32);
    let _ = spawn_blockhead(&mut s, px, py);
    s.run_for(2000).unwrap();
    check_placed(&mut s, &p, "after restart");
    s.disconnect();
}

#[test]
fn painting_an_object_changes_its_colour() {
    let o = Ours::start_seeded("objpaint", None);
    let mut s = join(o.port, "PAINTER");
    let (before, after) = object_paint_flow(&mut s);
    assert_ne!(before, after, "the column's net data did not change after painting it");
    s.disconnect();
}

#[test]
fn a_boat_put_on_water_exists() {
    let o = Ours::start_seeded("boat", None);
    let mut s = join(o.port, "SAILOR");
    let spot = boat_flow(&mut s);
    assert!(
        object_positions(&s, OBJECT_TYPE_BOAT).iter().any(|p| (p.0 - spot.0).abs() <= 1 && (p.1 - spot.1).abs() <= 1),
        "no boat near {:?}: {:?}",
        spot,
        object_positions(&s, OBJECT_TYPE_BOAT)
    );
    s.disconnect();
}

#[test]
fn parity_object_paint_and_boat_match_reference() {
    if !parity_enabled() {
        eprintln!("SKIP: parity_object_paint_and_boat_match_reference (BH_PARITY=1 + docker required)");
        return;
    }
    let r = reference_seeded("objpaint");
    let mut s = join(r.port(), "PAINTER");
    let (before, after) = object_paint_flow(&mut s);
    let spot = boat_flow(&mut s);
    let boats = object_positions(&s, OBJECT_TYPE_BOAT);
    s.disconnect();
    assert_ne!(before, after, "reference: painting the column changed nothing");
    assert!(
        boats.iter().any(|p| (p.0 - spot.0).abs() <= 1 && (p.1 - spot.1).abs() <= 1),
        "reference: {:?} {:?}",
        spot,
        boats
    );
}

#[test]
fn a_gate_cannot_be_removed_by_its_non_admin_owner() {
    let o = Ours::start("gate", None);
    let mut a = join(o.port, "GATEKEEPER");
    let mut b = join(o.port, "WATCHER");
    let (gate_kept, door_kept) = gate_flow(&mut a, &mut b);
    assert!(gate_kept, "the gate was removed");
    assert!(!door_kept, "the plain door was not removed (the test does not reach the remove path)");
    a.disconnect();
    b.disconnect();
}

#[test]
fn found_item_list_is_kept_for_the_player() {
    let o = Ours::start("found", None);
    // the server quits when its last player leaves without any blockhead left: keep somebody connected
    let mut keeper = join(o.port, "KEEPER");
    let mut s = join(o.port, "FINDER");
    let (px, py) = portal(&s);
    let _ = spawn_blockhead(&mut s, px, py);
    s.send_raw(&blockheads_server::protocol::messages::found_items_v2(&[(5, 3), (1030, 2)])).unwrap();
    s.run_for(1500).unwrap();
    s.disconnect();
    let mut s = rejoin(o.port, "FINDER");
    let set = blockheads_server::foundation::indexset::IndexSet::from_serialized_data(found_items(&s).as_deref());
    assert!([5, 6, 7, 1030, 1031].iter().all(|i| set.contains_index(*i)), "found items not kept: {:?}", set.ranges());
    s.disconnect();
}

#[test]
fn parity_placing_and_gates_match_reference() {
    if !parity_enabled() {
        eprintln!("SKIP: parity_placing_and_gates_match_reference (BH_PARITY=1 + docker required)");
        return;
    }
    let r = reference_seeded("place");
    let mut s = join(r.port(), "PLACER");
    let p = place_flow(&mut s);
    check_placed(&mut s, &p, "reference");
    let mut b = join(r.port(), "WATCHER");
    let (gate_kept, door_kept) = gate_flow(&mut s, &mut b);
    assert!(gate_kept && !door_kept, "reference: gate kept {} door kept {}", gate_kept, door_kept);
    b.disconnect();
    s.disconnect();
    // Found items are not compared: after a 0x4c of ours the reference logs "unknown type ?16" when the player
    // joins again and never sends the initial data (open question: our index-set encoding vs the client's).
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

#[test]
fn parity_walk_matches_reference() {
    if !parity_enabled() {
        eprintln!("SKIP: parity_walk_matches_reference (BH_PARITY=1 + docker required)");
        return;
    }
    let r = reference("walk");
    let mut s = join(r.port(), "WALKER");
    let (before, after) = walk_flow(&mut s, WALK);
    s.disconnect();
    assert_eq!(explored(&before), 0, "reference: destination already explored");
    assert!(explored(&after) > 0, "reference: the area walked into was never explored");
}
