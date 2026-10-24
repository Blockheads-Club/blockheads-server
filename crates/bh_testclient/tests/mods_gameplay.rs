//! API 0.2 gameplay events on a live server, driven by the protocol test client through the real request paths:
//! pre-login rejection, block break / place and object placement cancelled by the land-claim sample (the client is
//! resynced through the protected-tile path), block-changed, PvP damage (cancel and amount change), movement and
//! death. A probe mod logs every event it sees; the test reads the server log.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bh_testclient::session::BatchKind;
use bh_testclient::{
    BlockheadSpec, ClientConfig, FillNetRequest, Identity, JoinOutcome, PlaceWorkbenchNetRequest, RemoveNetRequest,
    Session,
};
use blockheads_server::host::{self, HostConfig, ServerHandle, Status, WorldSelect};
use blockheads_server::mods::ModsConfig;
use blockheads_server::plist::value::PlistValue as PV;
use blockheads_server::protocol::block_wire::{decode_server_block, BlockMsg};
use blockheads_server::protocol::codec::{plist_from, t};

const OBJECT_TYPE_BLOCKHEAD: u8 = 24;
const ITEM_DIRT: u16 = 1048;
const AIR: u8 = 2;
const PLOT: i32 = 16;

const PROBE: &str = r#"
local function who(p) return if p then p:alias() else "-" end
local function log(...) bh.log.info(string.format(...)) end
return { on_enable = function()
    bh.events.on("player-pre-login", function(ev)
        log("pre-login %s", ev.alias)
        if ev.alias == "BADGUY" then ev.cancelled = true; ev.reason = "banned" end
    end)
    bh.events.on("block-break", function(ev)
        log("break by %s at %d,%d %s tile=%d cancelled=%s", who(ev.player), ev.pos.x, ev.pos.y, ev.layer, ev.tile, tostring(ev.cancelled))
    end, { priority = "monitor" })
    bh.events.on("block-place", function(ev)
        log("place by %s at %d,%d item=%d cancelled=%s", who(ev.player), ev.pos.x, ev.pos.y, ev.item, tostring(ev.cancelled))
    end, { priority = "monitor" })
    bh.events.on("object-place", function(ev)
        log("object by %s at %d,%d %s item=%d cancelled=%s", who(ev.player), ev.pos.x, ev.pos.y, ev.object, ev.item, tostring(ev.cancelled))
    end, { priority = "monitor" })
    bh.events.on("block-changed", function(ev)
        if ev.player then log("changed at %d,%d tile=%d by %s", ev.pos.x, ev.pos.y, ev.tile, who(ev.player)) end
    end)
    bh.events.on("damage", function(ev)
        if ev.amount > 0.5 then ev.cancelled = true else ev.amount = ev.amount * 2 end
    end)
    bh.events.on("damage", function(ev)
        log("damage %s by %s amount=%.2f cancelled=%s", who(ev.player), who(ev.attacker), ev.amount, tostring(ev.cancelled))
    end, { priority = "monitor" })
    bh.events.on("death", function(ev) log("death %s blockhead=%d", who(ev.player), ev.blockhead) end)
    bh.events.on("player-move", function(ev)
        log("move %s %d,%d>%d,%d", who(ev.player), ev.from.x, ev.from.y, ev.to.x, ev.to.y)
    end)
end }
"#;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}

fn mods_dir(root: &Path) -> PathBuf {
    let mods = root.join("mods");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/mods");
    for m in ["economy", "landclaim", "perms"] {
        copy_dir(&fixtures.join(m), &mods.join(m));
    }
    let probe = mods.join("probe");
    std::fs::create_dir_all(probe.join("server")).unwrap();
    std::fs::write(
        probe.join("mod.toml"),
        "schema = 1\nid = \"probe\"\nversion = \"1.0.0\"\nname = { en = \"Probe\" }\nauthors = [\"t\"]\nlicense = \"MIT\"\n\
         api = \"^0.2\"\n[sides]\nserver = true\nclient = false\n[entry]\nserver = { runtime = \"lua\", path = \"server/main.luau\" }\n",
    )
    .unwrap();
    std::fs::write(probe.join("server/main.luau"), PROBE).unwrap();
    std::fs::write(probe.join("LICENSE"), "MIT\n").unwrap();
    mods
}

struct Running {
    handle: ServerHandle,
    lines: Arc<Mutex<Vec<String>>>,
    port: u16,
}

impl Running {
    fn start(data: &Path, world: &str, port: u16, mods: PathBuf) -> Running {
        let lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let l2 = lines.clone();
        let cfg = HostConfig {
            data_dir: Some(data.to_path_buf()),
            world: WorldSelect::Load(world.to_string()),
            port: Some(port),
            mods: ModsConfig { dir: Some(mods), disabled: false },
            ..HostConfig::default()
        };
        let handle = host::start(cfg, Arc::new(move |s| l2.lock().unwrap().push(s.to_string()))).unwrap();
        let end = Instant::now() + Duration::from_secs(90);
        while handle.status() == Status::Starting && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(handle.status(), Status::Running, "{:#?}", lines.lock().unwrap());
        Running { handle, lines, port }
    }
    fn log(&self) -> String {
        self.lines.lock().unwrap().join("\n")
    }
    fn has(&self, needle: &str) -> bool {
        self.lines.lock().unwrap().iter().any(|l| l.contains(needle))
    }
    /// Waits for a log line containing `needle` while pumping the sessions.
    fn wait(&self, sessions: &mut [&mut Session], needle: &str) {
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end {
            if self.has(needle) {
                return;
            }
            for s in sessions.iter_mut() {
                s.run_for(20).unwrap();
            }
        }
        panic!("no log line containing {needle:?}\n{}", self.log());
    }
    fn connect(&self, alias: &str) -> Session {
        let cfg = ClientConfig { addr: SocketAddr::from(([127, 0, 0, 1], self.port)), ..Default::default() };
        Session::connect(cfg, Identity::new(alias)).unwrap()
    }
    fn join(&self, alias: &str) -> Session {
        let mut s = self.connect(alias);
        assert_eq!(s.join(10_000).unwrap(), JoinOutcome::Approved, "{}", self.log());
        s.request_initial_data().unwrap();
        s.wait_for(0x06, 10_000).unwrap().expect("initial data");
        s
    }
}

fn summary_int(s: &Session, key: &str) -> i32 {
    s.state.summary.as_ref().and_then(|p| p.object_for_key(key)).map(|v| v.int_value()).expect(key)
}

/// Every copy of the block holding `(x, y)` received since packet number `mark`.
fn blocks_since(s: &Session, mark: usize, x: i32, y: i32) -> Vec<BlockMsg> {
    let i = s.macro_index_for_tile(x, y) as u32;
    s.received()[mark..]
        .iter()
        .filter(|rx| rx.data.first() == Some(&0x04) && rx.data.get(1..5) == Some(&i.to_le_bytes()[..]))
        .filter_map(|rx| decode_server_block(&rx.data[1..]))
        .collect()
}

/// Requests the macro block holding a tile (dropping it first, so the server sends it again) and waits for it.
/// Returns the packet mark from before the request.
fn load_block_at(s: &mut Session, x: i32, y: i32) -> usize {
    let i = s.macro_index_for_tile(x, y);
    let mark = s.received().len();
    s.block_removal_notification(i as u32).unwrap();
    s.state.block_msgs.remove(&(i as u32));
    s.request_block(i, true).unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    while blocks_since(s, mark, x, y).is_empty() {
        assert!(Instant::now() < end, "block {i} not sent");
        s.run_for(20).unwrap();
    }
    mark
}

/// `(type_index, contents, foreground_contents)` of a tile in a block.
fn tile_in(b: &BlockMsg, x: i32, y: i32) -> (u8, u8, u8) {
    let off = (((y & 31) * 32 + (x & 31)) * 64) as usize;
    let t = &b.tiles[off..off + 64];
    (t[0], t[3], t[0xb])
}

/// The tile in the last copy of its block received since `mark` (0 = ever).
fn tile_since(s: &Session, mark: usize, x: i32, y: i32) -> Option<(u8, u8, u8)> {
    blocks_since(s, mark, x, y).last().map(|b| tile_in(b, x, y))
}

fn tile(s: &Session, x: i32, y: i32) -> Option<(u8, u8, u8)> {
    tile_since(s, 0, x, y)
}

/// The receipts (resync list) in every copy of the block holding `(x, y)` received since `mark`.
fn receipts_since(s: &Session, mark: usize, x: i32, y: i32) -> Vec<i64> {
    let mut out = Vec::new();
    for b in blocks_since(s, mark, x, y) {
        if let Some(PV::Array(a)) = b.extra.as_ref().and_then(|e| e.object_for_key("receipts")) {
            out.extend(a.items.iter().map(|v| v.int_value() as i64));
        }
    }
    out
}

fn plot(x: i32, y: i32) -> (i32, i32) {
    (x.div_euclid(PLOT), y.div_euclid(PLOT))
}

/// Requests (and waits for) every macro block the search window of `spot` touches, so the search does not depend on
/// which neighbours the server happened to have streamed already (a portal near a block edge has part of its window
/// in the next block, east or north/south, and under load the server's own streaming arrives late).
fn load_spot_window(s: &mut Session, x: i32, y: i32) {
    let mut seen = Vec::new();
    for bx in [x, x + 3, x + 28] {
        for by in [y - 9, y, y + 8] {
            let i = s.macro_index_for_tile(bx, by);
            if !seen.contains(&i) {
                seen.push(i);
                load_block_at(s, bx, by);
            }
        }
    }
}

/// An air tile standing on solid ground east of `(x, y)`, with another air tile and a solid tile in the same plot.
fn spot(s: &Session, x: i32, y: i32) -> ((i32, i32), (i32, i32), (i32, i32)) {
    for cx in x + 3..x + 28 {
        for cy in (y - 8..y + 8).rev() {
            let (Some(here), Some(below)) = (tile(s, cx, cy), tile(s, cx, cy - 1)) else { continue };
            let ok = here == (AIR, 0, 0) && ![AIR, 3, 5].contains(&below.0) && below.1 == 0 && below.2 == 0;
            if !ok || plot(cx, cy) != plot(cx, cy - 1) || plot(cx, cy) != plot(cx + 1, cy) {
                continue;
            }
            if tile(s, cx + 1, cy) == Some((AIR, 0, 0)) {
                return ((cx, cy), (cx + 1, cy), (cx, cy - 1));
            }
        }
    }
    panic!("no spot near {x},{y}");
}

/// Creates a blockhead at `(x, y)` and returns its id.
fn spawn_blockhead(s: &mut Session, x: i32, y: i32) -> u64 {
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

/// A `BlockheadUpdateData` (0x70 bytes) standing on `to`, coming from `from`.
fn bh_update(uid: u64, from: (i32, i32), to: (i32, i32), regenerating: bool) -> Vec<u8> {
    let mut b = vec![0u8; 0x70];
    b[0..8].copy_from_slice(&uid.to_le_bytes());
    for (off, v) in [(0x08, to.0), (0x0c, to.1), (0x18, from.0), (0x1c, from.1), (0x20, to.0), (0x24, to.1)] {
        b[off..off + 4].copy_from_slice(&(v as u32).to_le_bytes());
    }
    b[0x33] = 100; // happiness
    b[0x3c] = 1; // isCurrentlyActive
    b[0x3f] = 127; // health
    b[0x6b] = regenerating as u8;
    b
}

fn chat_text(data: &[u8]) -> Option<String> {
    if data.first() != Some(&t::CHAT_MESSAGE) {
        return None;
    }
    match plist_from(&data[1..])?.object_for_key("message") {
        Some(PV::String(s)) => Some(s.s.clone()),
        _ => None,
    }
}

fn wait_chat(s: &mut Session, needle: &str) {
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        s.run_for(50).unwrap();
        if s.drain().iter().filter_map(|rx| chat_text(&rx.data)).any(|m| m.contains(needle)) {
            return;
        }
    }
    panic!("no chat line containing {needle:?}");
}

/// The damage amounts of the 0x2a packets received within `ms`.
fn damage_received(s: &mut Session, ms: u64) -> Vec<f32> {
    s.run_for(ms).unwrap();
    s.drain()
        .iter()
        .filter(|rx| rx.data.first() == Some(&0x2a) && rx.data.len() >= 13)
        .map(|rx| f32::from_le_bytes(rx.data[9..13].try_into().unwrap()))
        .collect()
}

fn damage_request(blockhead: u64, amount: f32) -> Vec<u8> {
    let mut p = vec![0x2a];
    p.extend(blockhead.to_le_bytes());
    p.extend(amount.to_le_bytes());
    p.extend([1, 0, 0, 0]);
    p
}

#[test]
fn gameplay_events_and_cancellation() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("TheBlockheads");
    let w = host::create_world(Some(&data), "Claims", 104, false).expect("create");
    let mut srv = Running::start(&data, &w.id, 15420, mods_dir(tmp.path()));
    assert!(srv.has("loaded 4 mod(s): economy 1.0.0, perms 1.0.0, landclaim 1.0.0, probe 1.0.0"), "{}", srv.log());

    // player-pre-login: rejected with the vanilla "banned" reason (12).
    let mut bad = srv.connect("BADGUY");
    assert_eq!(bad.join(10_000).unwrap(), JoinOutcome::Rejected { reason: 12 }, "{}", srv.log());
    assert!(srv.has("[mods] probe: pre-login BADGUY"));

    // Alice stands on free ground and buys the plot.
    let mut alice = srv.join("ALICE");
    let (px, py) = (summary_int(&alice, "startPortalPos.x"), summary_int(&alice, "startPortalPos.y"));
    load_spot_window(&mut alice, px, py);
    let ((x, y), (ax, ay), (gx, gy)) = spot(&alice, px, py);
    load_block_at(&mut alice, x, y - 16);
    let alice_bh = spawn_blockhead(&mut alice, x, y);
    alice.chat("/claim").unwrap();
    wait_chat(&mut alice, "You bought this plot for 50 coins.");

    // Bob cannot dig in Alice's plot: the tile stays, and his next copy of the block carries the receipt (resync).
    let mut bob = srv.join("BOB");
    load_block_at(&mut bob, gx, gy);
    let ground = tile(&bob, gx, gy).unwrap();
    assert_ne!(ground.0, AIR);
    let mark = bob.received().len();
    bob.remove(RemoveNetRequest { pos_x: gx as u32, pos_y: gy as u32, ..Default::default() }).unwrap();
    wait_chat(&mut bob, "This land belongs to ALICE.");
    srv.wait(&mut [&mut alice, &mut bob], &format!("break by BOB at {gx},{gy} block tile={} cancelled=true", ground.0));
    load_block_at(&mut bob, gx, gy);
    assert_eq!(tile_since(&bob, mark, gx, gy), Some(ground), "a cancelled break leaves the tile");
    let r = receipts_since(&bob, mark, gx, gy);
    assert!(r.contains(&bob.fill_index(gx, gy)), "the client gets its resync receipt: {r:?}");

    // Nor build there.
    let mark = bob.received().len();
    bob.fill(FillNetRequest { pos_x: ax as u32, pos_y: ay as u32, type_: ITEM_DIRT, ..Default::default() }, None)
        .unwrap();
    srv.wait(&mut [&mut alice, &mut bob], &format!("place by BOB at {ax},{ay} item={ITEM_DIRT} cancelled=true"));
    load_block_at(&mut bob, ax, ay);
    assert_eq!(tile_since(&bob, mark, ax, ay), Some((AIR, 0, 0)), "a cancelled place leaves the tile");
    assert!(receipts_since(&bob, mark, ax, ay).contains(&bob.fill_index(ax, ay)));
    bob.place_workbench(
        PlaceWorkbenchNetRequest { pos_x: ax as u32, pos_y: ay as u32, workbench_type: 1, ..Default::default() },
        None,
    )
    .unwrap();
    srv.wait(&mut [&mut alice, &mut bob], &format!("object by BOB at {ax},{ay} workbench item=1 cancelled=true"));

    // The owner may: the tile changes and block-changed names her.
    alice.remove(RemoveNetRequest { pos_x: gx as u32, pos_y: gy as u32, ..Default::default() }).unwrap();
    srv.wait(
        &mut [&mut alice, &mut bob],
        &format!("break by ALICE at {gx},{gy} block tile={} cancelled=false", ground.0),
    );
    srv.wait(&mut [&mut alice, &mut bob], &format!("changed at {gx},{gy} tile="));
    srv.wait(&mut [&mut alice, &mut bob], "by ALICE");
    let mark = load_block_at(&mut bob, gx, gy);
    assert_ne!(tile_since(&bob, mark, gx, gy), Some(ground), "the owner's break went through");

    // PvP damage reaches Alice's client through the damage event: the probe doubles small hits and cancels big ones.
    srv.handle.send_command("/pvp-on");
    srv.wait(&mut [&mut alice, &mut bob], "PVP");
    damage_received(&mut alice, 200);
    bob.send_raw(&damage_request(alice_bh, 0.2)).unwrap();
    srv.wait(&mut [&mut bob], "damage ALICE by BOB amount=0.40 cancelled=false");
    let got = damage_received(&mut alice, 500);
    assert_eq!(got, vec![0.4f32], "{}", srv.log());
    bob.send_raw(&damage_request(alice_bh, 0.9)).unwrap();
    srv.wait(&mut [&mut bob], "damage ALICE by BOB amount=0.90 cancelled=true");
    assert!(damage_received(&mut alice, 500).is_empty(), "a cancelled hit sends nothing");

    // Movement and death of a player's blockhead (the client simulates it and reports it in its updates).
    let step = bh_update(alice_bh, (x, y), (ax, ay), false);
    alice.send_object_batch(BatchKind::Update, OBJECT_TYPE_BLOCKHEAD, &[step]).unwrap();
    srv.wait(&mut [&mut alice], &format!("move ALICE {x},{y}>{ax},{ay}"));
    let dead = bh_update(alice_bh, (ax, ay), (ax, ay), true);
    alice.send_object_batch(BatchKind::Update, OBJECT_TYPE_BLOCKHEAD, &[dead]).unwrap();
    srv.wait(&mut [&mut alice], &format!("death ALICE blockhead={alice_bh}"));

    alice.disconnect();
    bob.disconnect();
    assert_eq!(srv.handle.stop(), Status::Stopped(0), "{}", srv.log());
}
