//! The scenario scripts. Each one records observations into `ctx.obs`; see the module docs of
//! [`super`] for how they are compared.

use std::collections::BTreeMap;

use blockheads_server::foundation::gsdict::GsDict;
use blockheads_server::plist::value::PlistValue as PV;
use blockheads_server::protocol::codec::{gz_plist_from, plist_gz};

use super::*;
use bh_testclient::{FillNetRequest, PlaceInteractionObjectNetRequest, PlaceWorkbenchNetRequest, RemoveNetRequest};

pub const ITEM_STICK: u16 = 4;
pub const ITEM_CHEST: u16 = 1043;
pub const ITEM_ROCK: u16 = 1047;
pub const ITEM_DIRT: u16 = 1048;

/// Every scenario, by name.
pub fn all() -> Vec<Scenario> {
    vec![
        chest_walk(),
        commands(),
        paint_sign(),
        craft(),
        sleep(),
        npc(),
        vehicles(),
        trade(),
        portal_chest(),
        rules_welcome(),
    ]
}

pub fn by_name(name: &str) -> Option<Scenario> {
    all().into_iter().find(|s| s.name == name)
}

/// A blockhead standing next to the portal: (session, blockhead id, x, y). Records `<tag>.blockhead`.
pub fn spawn(ctx: &mut Ctx, alias: &str, name: &str, dx: i32) -> Option<(Session, u64, i32, i32)> {
    let mut a = ctx.join(alias);
    let (px, py) = portal(&a);
    request_around(&mut a, px, py, 1, 1);
    a.wait_quiet(800, 5000).unwrap();
    let bx = px + dx;
    let by = ground_at(&a, bx, py + 20).unwrap_or(py);
    let bh = new_blockhead(&mut a, name, bx, by);
    ctx.obs.add(format!("{}.blockhead", alias), bh.is_some());
    Some((a, bh?, bx, by))
}

/// Place an interaction object (0x16) from the blockhead's inventory and return the created object.
pub fn place_object(
    a: &mut Session,
    bh: u64,
    item_type: u16,
    obj_type: u8,
    pos: (i32, i32),
    inv_after: PV,
) -> Option<(u64, Vec<u8>)> {
    let tail = d(vec![
        ("item", PV::data(item(item_type))),
        ("blockhead_uniqueID", PV::int(bh as i64)),
        ("blockhead_inventory_change", PV::data(plist_gz(&inv_after).unwrap())),
    ]);
    a.place_interaction_object(PlaceInteractionObjectNetRequest { pos_x: pos.0 as u32, pos_y: pos.1 as u32 }, &tail)
        .unwrap();
    wait_created(a, obj_type, 5000)
}

// ------------------------------------------------------------------------------------------------ chest + walk

/// The original audit script: chest lifecycle with two players, a block removed and filled, a workbench, a long
/// walk (fog of war, light blocks, worldgen torches), then a reload that reopens the chest.
pub fn chest_walk() -> Scenario {
    Scenario { name: "chest_walk", play: chest_walk_play, reload: Some(chest_walk_reload), save: true, console: false }
}

fn chest_walk_play(ctx: &mut Ctx) {
    let Some((mut a, bh, bx, by)) = spawn(ctx, "AUDITA", "Ann", 3) else { return };
    let (px, py) = portal(&a);
    absorb(&mut a, &mut ctx.obs, "bh_create");
    let obs = &mut ctx.obs;

    // ---- chest: place, open, put, close, reopen, take, close
    let cx = bx + 2;
    let cy = ground_at(&a, cx, by + 10).unwrap_or(by);
    a.upload_inventory(bh, &slots(8, &[(0, ITEM_CHEST, 1), (1, ITEM_DIRT, 10)])).unwrap();
    a.run_for(300).unwrap();
    let placed = place_object(&mut a, bh, ITEM_CHEST, obj::CHEST, (cx, cy), slots(8, &[(1, ITEM_DIRT, 10)]));
    a.wait_quiet(800, 4000).unwrap();
    obs.add("chest.target", format!("{},{}", cx, cy));
    absorb(&mut a, obs, "chest_place");
    let Some((_cid, cblob)) = placed.or_else(|| last_object(&a, obj::CHEST)) else {
        obs.add("chest.created", false);
        return;
    };
    let owner = a.id.player_id();
    let send_chest = |s: &mut Session, who: u64, in_use: bool, sl: Option<PV>| {
        s.send_object_batch(BatchKind::Update, obj::CHEST, &[chest_blob(&cblob, in_use, who, sl, &owner)]).unwrap();
        s.wait_quiet(600, 2500).unwrap();
    };
    send_chest(&mut a, bh, true, None); // open: no inventory in the update
    absorb(&mut a, obs, "chest_open1");
    send_chest(&mut a, bh, true, Some(slots(16, &[(3, ITEM_DIRT, 6)]))); // put 6 dirt into slot 3
    a.upload_inventory(bh, &slots(8, &[(1, ITEM_DIRT, 4)])).unwrap();
    absorb(&mut a, obs, "chest_put");
    send_chest(&mut a, bh, false, Some(slots(16, &[(3, ITEM_DIRT, 6)]))); // close
    absorb(&mut a, obs, "chest_close1");
    send_chest(&mut a, bh, true, None); // reopen
    absorb(&mut a, obs, "chest_open2");
    send_chest(&mut a, bh, true, Some(slots(16, &[(3, ITEM_DIRT, 4)]))); // take 2
    send_chest(&mut a, bh, false, Some(slots(16, &[(3, ITEM_DIRT, 4)])));
    absorb(&mut a, obs, "chest_take_close");

    // ---- second player opens, edits and closes the same chest; then A reopens
    a.run_for(100).unwrap();
    let mut b = join(ctx.addr, "AUDITB", &ctx.dir.join("trace_play_AUDITB.jsonl"));
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
    absorb(&mut b, obs, "B_join");
    absorb(&mut a, obs, "A_B_join");
    send_chest(&mut b, bh_b, true, None);
    absorb(&mut b, obs, "B_chest_open");
    a.run_for(300).unwrap();
    absorb(&mut a, obs, "A_while_B_open");
    send_chest(&mut b, bh_b, false, Some(slots(16, &[(0, ITEM_ROCK, 1), (3, ITEM_DIRT, 4)])));
    absorb(&mut b, obs, "B_chest_close");
    a.run_for(300).unwrap();
    absorb(&mut a, obs, "A_after_B_close");
    send_chest(&mut a, bh, true, None);
    absorb(&mut a, obs, "A_chest_open3");
    send_chest(&mut a, bh, false, None);
    absorb(&mut a, obs, "A_chest_close3");

    // ---- break / place a block
    let fx = cx + 3;
    let fy = ground_at(&a, fx, cy + 10).unwrap_or(cy) - 1;
    a.remove(RemoveNetRequest { remove_blockhead_id: bh, pos_x: fx as u32, pos_y: fy as u32, ..Default::default() })
        .unwrap();
    a.wait_quiet(800, 4000).unwrap();
    absorb(&mut a, obs, "remove");
    a.fill(FillNetRequest { pos_x: fx as u32, pos_y: fy as u32, type_: 0x0b, ..Default::default() }, None).unwrap();
    a.wait_quiet(800, 4000).unwrap();
    absorb(&mut a, obs, "fill");

    // ---- workbench: place, open, "craft" (client inventory gains sticks), close
    let wx = bx - 4;
    let wy = ground_at(&a, wx, by + 10).unwrap_or(by);
    a.place_workbench(
        PlaceWorkbenchNetRequest { pos_x: wx as u32, pos_y: wy as u32, workbench_type: 2, ..Default::default() },
        None,
    )
    .unwrap();
    let wb = wait_created(&mut a, obj::WORKBENCH, 5000);
    a.wait_quiet(800, 4000).unwrap();
    absorb(&mut a, obs, "workbench_place");
    if let Some((_, wblob)) = wb {
        let mut w = wblob[..0x38.min(wblob.len())].to_vec();
        w.resize(0x38, 0);
        for (in_use, who) in [(1u8, bh), (0u8, 0u64)] {
            let mut wi = w.clone();
            wi[0x18..0x20].copy_from_slice(&who.to_le_bytes());
            wi[0x22] = in_use;
            wi.extend(plist_gz(&d(vec![])).unwrap());
            a.send_object_batch(BatchKind::Update, obj::WORKBENCH, &[wi]).unwrap();
            a.wait_quiet(800, 4000).unwrap();
            if in_use == 1 {
                a.upload_inventory(bh, &slots(8, &[(1, ITEM_DIRT, 4), (2, ITEM_STICK, 2)])).unwrap();
            }
        }
    } else {
        obs.add("workbench.created", false);
    }
    absorb(&mut a, obs, "workbench_use");
    b.disconnect();

    // ---- walk far: right, then back left past the start (adjacent steps, block requests as the client does)
    let steps: i32 = std::env::var("BH_DIFF_WALK").ok().and_then(|v| v.parse().ok()).unwrap_or(160);
    let mut pos = (bx, by);
    for dirn in [1i32, -1] {
        let n = if dirn < 0 { steps * 2 } else { steps };
        for i in 0..n {
            let to = (pos.0 + dirn, ground_at(&a, pos.0 + dirn, pos.1 + 6).unwrap_or(pos.1));
            a.send_object_batch(BatchKind::Update, obj::BLOCKHEAD, &[bh_update(bh, pos, to)]).unwrap();
            pos = to;
            if i % 8 == 0 {
                request_around(&mut a, pos.0, pos.1, 2, 1);
            }
            a.run_for(60).unwrap();
        }
        a.wait_quiet(1500, 8000).unwrap();
        obs.add(if dirn > 0 { "walk_right.end" } else { "walk_left.end" }, format!("{},{}", pos.0, pos.1));
        absorb(&mut a, obs, if dirn > 0 { "walk_right" } else { "walk_left" });
    }
    light_and_block_summary(&a, obs, "walk");
    a.disconnect();
}

/// After a restart with `--load`: a fresh blockhead opens the chest placed before; the inventory must survive.
fn chest_walk_reload(ctx: &mut Ctx) {
    let Some((mut a, bh, _, _)) = spawn(ctx, "AUDITA", "Cat", 3) else { return };
    let obs = &mut ctx.obs;
    obs.add(
        "reload.initial_blockheads",
        a.state.initial_plist.as_ref().map(bh_testclient::decode::pv_json).unwrap_or_default().len(),
    );
    absorb(&mut a, obs, "reload_join");
    let Some((_, cblob)) = last_object(&a, obj::CHEST) else {
        obs.add("reload.chest_seen", false);
        return;
    };
    let owner = a.id.player_id();
    for in_use in [true, false] {
        a.send_object_batch(BatchKind::Update, obj::CHEST, &[chest_blob(&cblob, in_use, bh, None, &owner)]).unwrap();
        a.wait_quiet(600, 2500).unwrap();
    }
    absorb(&mut a, obs, "reload_chest_open");
    a.disconnect();
}

// ------------------------------------------------------------------------------------------------ commands

/// Every server command, typed into the console and sent as chat by an admin, a mod and a plain player:
/// replies (0x24 / console output), list files and kicks.
pub fn commands() -> Scenario {
    Scenario { name: "commands", play: commands_play, reload: None, save: true, console: true }
}

/// Every command line the scenario issues (console and chat), `NAME` = a joined player.
const COMMANDS: &[&str] = &[
    "/help",
    "/players",
    "/list-blacklist",
    "/list-whitelist",
    "/list-adminlist",
    "/list-modlist",
    "/ban someone",
    "/ban-no-device other",
    "/list-blacklist",
    "/unban someone",
    "/unban nobody",
    "/whitelist friend",
    "/whitelist CMDA",
    "/whitelist CMDB",
    "/whitelist cmdm",
    "/list-whitelist",
    "/unwhitelist friend",
    "/admin helper",
    "/list-adminlist",
    "/unadmin helper",
    "/mod modder",
    "/list-modlist",
    "/unmod modder",
    "/pvp-on",
    "/pvp-off",
    "/set-privacy searchable",
    "/set-privacy bogus",
    "/set-password secret",
    "/remove-password",
    "/remove-password",
    "/reset-owner somebody",
    "/reset-owner somebody",
    "/repair",
    "/load-lists",
    "/debug-log",
    "/clear",
    "/notacommand",
    "/clear-blacklist",
    "/clear-whitelist",
    "/clear-modlist",
];

/// Console-only owner commands.
/// (`/set-salt` turns on cloud keys and locks the test clients out: it is typed last, see `commands_play`.)
const CONSOLE_ONLY: &[&str] = &["/set-owner bigboss", "/remove-owner", "/set-credit 5"];

fn list_files(ctx: &mut Ctx, phase: &str) {
    let dir = ctx.server.save_dir(WORLD);
    for f in ["adminlist.txt", "blacklist.txt", "modlist.txt", "whitelist.txt"] {
        let body = std::fs::read_to_string(dir.join(f)).unwrap_or_else(|_| "<missing>".into());
        let entries: Vec<&str> = body.lines().skip(1).filter(|l| !l.trim().is_empty()).collect();
        ctx.obs.add(format!("{}.file.{}", phase, f), entries.join(","));
    }
}

fn chat_cmd(s: &mut Session, obs: &mut Obs, phase: &str, line: &str) {
    let replies = s.command(line).unwrap_or_default();
    let texts: Vec<String> = replies
        .iter()
        .map(|p| mask_digits(p.object_for_key("message").and_then(|v| v.as_str()).unwrap_or("")))
        .collect();
    obs.add(format!("{}.{}", phase, line), texts.join(" | "));
}

fn commands_play(ctx: &mut Ctx) {
    // console, nobody connected
    // `/repair` from the console throws in the original and the console stops reading: it comes last
    for line in COMMANDS.iter().filter(|l| **l != "/repair").chain(CONSOLE_ONLY) {
        let from = ctx.server.log().len();
        ctx.console(line);
        let out = ctx.log_since(from).join(" | ");
        ctx.obs.add(format!("console.{}", line), out);
    }
    list_files(ctx, "console");

    // players: CMDA (admin), CMDM (mod), CMDB (plain)
    let mut a = ctx.join("CMDA");
    let mut m = ctx.join("CMDM");
    let mut b = ctx.join("CMDB");
    ctx.console("/admin cmda");
    ctx.console("/mod cmdm");
    a.wait_quiet(500, 2000).unwrap();
    let _ = (m.drain(), b.drain(), a.drain());
    for line in COMMANDS.iter().chain(CONSOLE_ONLY) {
        chat_cmd(&mut a, &mut ctx.obs, "admin", line);
        // keep the other clients' ENet connections serviced
        m.run_for(5).unwrap();
        b.run_for(5).unwrap();
    }
    list_files(ctx, "admin");
    for line in ["/help", "/players", "/ban x", "/unban x", "/admin x", "/kick cmda", "/list-blacklist"] {
        chat_cmd(&mut m, &mut ctx.obs, "mod", line);
    }
    for line in ["/help", "/players", "/kick cmda", "/ban cmda"] {
        chat_cmd(&mut b, &mut ctx.obs, "plain", line);
    }
    absorb(&mut b, &mut ctx.obs, "plain_rx");

    // kick and ban by an admin; the target notices the disconnect
    chat_cmd(&mut a, &mut ctx.obs, "admin", "/kick cmdb");
    b.run_for(1500).unwrap();
    ctx.obs.add("kick.cmdb_disconnected", b.state.disconnected);
    chat_cmd(&mut a, &mut ctx.obs, "admin", "/ban cmdm");
    m.run_for(1500).unwrap();
    ctx.obs.add("ban.cmdm_disconnected", m.state.disconnected);
    match Session::connect(ClientConfig { addr: ctx.addr, ..Default::default() }, Identity::new("CMDM")) {
        Ok(mut m2) => {
            ctx.obs.add("ban.rejoin", format!("{:?}", m2.join(10_000).ok()));
            m2.disconnect();
        }
        Err(e) => ctx.obs.add("ban.rejoin", format!("connect failed: {:?}", e.kind())),
    }
    chat_cmd(&mut a, &mut ctx.obs, "admin", "/unadmin cmda");
    chat_cmd(&mut a, &mut ctx.obs, "admin", "/help");
    list_files(ctx, "end");
    absorb(&mut a, &mut ctx.obs, "admin_rx");
    a.disconnect();

    // /stop from the console ends the server, unless /repair broke the console before
    let from = ctx.server.log().len();
    ctx.console("/clear-adminlist");
    ctx.console("/set-salt abc");
    let mut late = Session::connect(ClientConfig { addr: ctx.addr, ..Default::default() }, Identity::new("CMDA")).ok();
    let j = late.as_mut().map(|s| format!("{:?}", s.join(10_000).ok()));
    ctx.obs.add("salt.join", j.unwrap_or_else(|| "connect failed".into()));
    if let Some(mut s) = late {
        s.disconnect();
    }
    ctx.console("/repair");
    ctx.console("/stop");
    std::thread::sleep(Duration::from_secs(3));
    // server replies only: client disconnect lines race with the log offset
    let out: Vec<String> =
        ctx.log_since(from).into_iter().filter(|l| l.starts_with("SERVER:") || l.contains("Exiting")).collect();
    ctx.obs.add("console./stop", out.join(" | "));
    ctx.obs.add("stop.running", ctx.server.is_running());
}

// ------------------------------------------------------------------------------------------------ shared helpers

pub const ITEM_SIGN: u16 = 163;
pub const ITEM_BED: u16 = 63;
pub const ITEM_DOOR: u16 = 52;
pub const ITEM_PAINTING: u16 = 212;
pub const ITEM_TRADE_PORTAL: u16 = 210;
pub const ITEM_TRADING_POST: u16 = 168;
pub const ITEM_PORTAL_CHEST: u16 = 1074;
pub const ITEM_RAIL: u16 = 199;
pub const ITEM_HAND_CAR: u16 = 204;
pub const ITEM_BOAT: u16 = 111;
pub const ITEM_DODO_EGG: u16 = 91;

pub const T_DOOR: u8 = 20;
pub const T_BED: u8 = 23;
pub const T_DODO: u8 = 13;
pub const T_FREEBLOCK: u8 = 14;
pub const T_EGG: u8 = 30;
pub const T_BOAT: u8 = 32;
pub const T_RAIL: u8 = 40;
pub const T_HAND_CAR: u8 = 41;
pub const T_SIGN: u8 = 47;
pub const T_TRADING_POST: u8 = 48;
pub const T_TRADE_PORTAL: u8 = 50;
pub const T_PAINTING: u8 = 52;

/// A 1x1 PNG (painting image).
pub const PNG_1X1: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

pub fn png() -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(PNG_1X1).unwrap()
}

/// Ground spots (air on solid) at x = x0, x0+step, … (the first `n` found within 40 columns).
pub fn ground_spots(s: &Session, x0: i32, y0: i32, step: i32, n: usize) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    let mut x = x0;
    for _ in 0..40 {
        if out.len() >= n {
            break;
        }
        if let Some(y) = ground_at(s, x, y0 + 12) {
            if ground_at(s, x + 1, y0 + 12) == Some(y) {
                out.push((x, y));
            }
        }
        x += step;
    }
    out
}

/// Place an object and record `<label>.created`.
fn place(
    ctx: &mut Ctx,
    a: &mut Session,
    bh: u64,
    it: u16,
    ty: u8,
    pos: (i32, i32),
    label: &str,
) -> Option<(u64, Vec<u8>)> {
    let r = place_object(a, bh, it, ty, pos, slots(8, &[]));
    ctx.obs.add(format!("{}.created", label), r.is_some());
    r
}

#[allow(clippy::too_many_arguments)]
/// Fill (0x10) with an item type and record `<label>.created` for the object it should create.
fn fill_item(
    ctx: &mut Ctx,
    a: &mut Session,
    bh: u64,
    it: u16,
    ty: u8,
    pos: (i32, i32),
    save: Option<PV>,
    label: &str,
) -> Option<(u64, Vec<u8>)> {
    let mut pairs = vec![("blockhead_uniqueID", PV::int(bh as i64))];
    if let Some(sd) = save {
        pairs.push(("placeSaveDict", sd));
    }
    a.fill(
        FillNetRequest { pos_x: pos.0 as u32, pos_y: pos.1 as u32, type_: it, ..Default::default() },
        Some(&d(pairs)),
    )
    .unwrap();
    let r = wait_created(a, ty, 4000);
    ctx.obs.add(format!("{}.created", label), r.is_some());
    r
}

/// An object update blob: the server's header (first `header` bytes of `base`) patched by `f`, then a gzip plist.
fn update_blob(base: &[u8], header: usize, tail: Option<PV>, f: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    let mut b = base[..header.min(base.len())].to_vec();
    b.resize(header, 0);
    f(&mut b);
    if let Some(t) = tail {
        b.extend(plist_gz(&t).unwrap());
    }
    b
}

fn owner_tail(s: &Session) -> PV {
    d(vec![("ownerID", PV::string(s.id.player_id())), ("ownerName", PV::string(s.id.alias.clone()))])
}

/// The newest 0x07/0x46 creation blob of each object of `ty` seen so far.
fn creation_blobs(s: &Session, ty: u8) -> BTreeMap<u64, Vec<u8>> {
    let mut m = BTreeMap::new();
    for (k, b) in objects_since(s, 0, ty) {
        if k != 0x08 && b.len() >= 8 {
            m.insert(u64_at(&b, 0), b);
        }
    }
    m
}

// ------------------------------------------------------------------------------------------------ paint, signs, paintings

/// Paint (0x19) a chest, a sign, a bed, a door, a painting and a plain tile; edit a sign's text; place a painting
/// (0x10 with an image) and request its image (0x30 -> 0x31); reload and check what persisted.
pub fn paint_sign() -> Scenario {
    Scenario { name: "paint_sign", play: paint_sign_play, reload: Some(paint_sign_reload), save: true, console: false }
}

const RED: u16 = 0x3000;

fn paint_at(ctx: &mut Ctx, a: &mut Session, bh: u64, label: &str, ty: Option<u8>, pos: (i32, i32)) {
    let mark = a.received().len();
    a.paint(bh_testclient::PaintNetRequest {
        paint_blockhead_id: bh,
        pos_x: pos.0 as u32,
        pos_y: pos.1 as u32,
        color_index: RED,
        face_index: 0,
        ..Default::default()
    })
    .unwrap();
    a.wait_quiet(800, 4000).unwrap();
    if let Some(ty) = ty {
        let ups: Vec<String> = objects_since(a, mark, ty)
            .iter()
            .filter(|(k, _)| *k == 0x08)
            .map(|(_, b)| format!("{:04x}", u16_at(b, 0x24)))
            .collect();
        ctx.obs.add(format!("paint.{}.object_paint", label), ups.join(","));
    }
    // the block resend carries the tile paint
    let blocks = packets_since(a, mark, 0x04).len();
    ctx.obs.add(format!("paint.{}.block_resends", label), blocks);
    if let Some(t) = tile_at(a, pos.0, pos.1) {
        // paintFront..paintBot, and what the tile holds
        ctx.obs.add(format!("paint.{}.tile_paint", label), to_hex(&t[28..38]));
        ctx.obs.add(format!("paint.{}.tile_kind", label), format!("type={} contents={} fg={}", t[0], t[3], t[11]));
    }
}

fn paint_sign_play(ctx: &mut Ctx) {
    let Some((mut a, bh, bx, by)) = spawn(ctx, "PAINTA", "Pam", 0) else { return };
    absorb(&mut a, &mut ctx.obs, "join");
    let spots = ground_spots(&a, bx + 2, by, 3, 5);
    ctx.obs.soft("spots", format!("{:?}", spots));
    if spots.len() < 5 {
        return;
    }
    let chest = place(ctx, &mut a, bh, ITEM_CHEST, obj::CHEST, spots[0], "chest");
    let sign = place(ctx, &mut a, bh, ITEM_SIGN, T_SIGN, spots[1], "sign");
    let _bed = place(ctx, &mut a, bh, ITEM_BED, T_BED, spots[2], "bed");
    let _door = fill_item(ctx, &mut a, bh, ITEM_DOOR, T_DOOR, spots[3], None, "door");
    let pimg = d(vec![("outputImageData", PV::data(png()))]);
    let painting = fill_item(ctx, &mut a, bh, ITEM_PAINTING, T_PAINTING, spots[4], Some(pimg), "painting");
    a.wait_quiet(800, 4000).unwrap();
    absorb(&mut a, &mut ctx.obs, "placed");

    paint_at(ctx, &mut a, bh, "chest", Some(obj::CHEST), spots[0]);
    paint_at(ctx, &mut a, bh, "sign", Some(T_SIGN), spots[1]);
    paint_at(ctx, &mut a, bh, "bed", Some(T_BED), spots[2]);
    paint_at(ctx, &mut a, bh, "door", Some(T_DOOR), spots[3]);
    paint_at(ctx, &mut a, bh, "painting", Some(T_PAINTING), spots[4]);
    let below = (spots[0].0 - 1, spots[0].1 - 1);
    paint_at(ctx, &mut a, bh, "ground", None, below);
    absorb(&mut a, &mut ctx.obs, "painted");

    // edit the sign's text
    if let Some((_, sblob)) = sign {
        let mark = a.received().len();
        let mut tail = owner_tail(&a);
        if let PV::Dict(dd) = &mut tail {
            dd.map.insert("text".into(), PV::string("Edited by the script"));
        }
        let blob = update_blob(&sblob, 0x30, Some(tail), |b| {
            put_u64(b, 0x18, 0);
            b[0x22] = 0;
        });
        a.send_object_batch(BatchKind::Update, T_SIGN, &[blob]).unwrap();
        a.wait_quiet(800, 4000).unwrap();
        let echoes: Vec<String> = objects_since(&a, mark, T_SIGN)
            .iter()
            .map(|(_, b)| tail_plist(b, 0x30).map(|p| pstr(&p, "text")).unwrap_or_else(|| "<no tail>".into()))
            .collect();
        ctx.obs.add("sign.edit_echo", echoes.join(","));
        absorb(&mut a, &mut ctx.obs, "sign_edit");
    }
    // request the painting's image
    if let Some((pid, _)) = painting {
        let mark = a.received().len();
        let mut p = vec![0x30];
        p.extend_from_slice(&pid.to_le_bytes());
        a.send_raw(&p).unwrap();
        a.wait_quiet(800, 4000).unwrap();
        let r = packets_since(&a, mark, 0x31);
        ctx.obs.add("painting.image_replies", r.len());
        if let Some(r) = r.first() {
            ctx.obs.add("painting.image_matches", r.get(9..) == Some(&png()[..]));
        }
    }
    let _ = chest;
    a.disconnect();
}

fn paint_sign_reload(ctx: &mut Ctx) {
    let Some((mut a, _bh, _, _)) = spawn(ctx, "PAINTA", "Pia", 0) else { return };
    for (ty, label) in
        [(obj::CHEST, "chest"), (T_SIGN, "sign"), (T_BED, "bed"), (T_PAINTING, "painting"), (T_DOOR, "door")]
    {
        let blobs = creation_blobs(&a, ty);
        ctx.obs.add(format!("reload.{}.count", label), blobs.len());
        for b in blobs.values() {
            let pos = format!(
                "{},{}",
                u32::from_le_bytes(b[8..12].try_into().unwrap()),
                u32::from_le_bytes(b[12..16].try_into().unwrap())
            );
            if ty != T_DOOR && ty != T_PAINTING {
                ctx.obs.add(format!("reload.{}.{}.paint", label, pos), format!("{:04x}", u16_at(b, 0x24)));
            }
            if ty == T_SIGN {
                let t = tail_plist(b, 0x30).map(|p| pstr(&p, "text")).unwrap_or_else(|| "<no tail>".into());
                ctx.obs.add(format!("reload.sign.{}.text", pos), t);
            }
            if ty == T_PAINTING {
                let mark = a.received().len();
                let mut p = vec![0x30];
                p.extend_from_slice(&b[0..8]);
                a.send_raw(&p).unwrap();
                a.wait_quiet(800, 4000).unwrap();
                ctx.obs.add(format!("reload.painting.{}.image_replies", pos), packets_since(&a, mark, 0x31).len());
            }
        }
    }
    absorb(&mut a, &mut ctx.obs, "reload");
    a.disconnect();
}

// ------------------------------------------------------------------------------------------------ crafting / smelting

/// Workbench crafting as the client does it (crafting is client-side: inventory upload, a freeblock 0x14 of the
/// product, pickup 0x12), furnace and kiln fuel (0x0b fuelFraction) and burning, reload.
pub fn craft() -> Scenario {
    Scenario { name: "craft", play: craft_play, reload: Some(craft_reload), save: true, console: false }
}

const WB_HEADER: usize = 0x38;

fn wb_summary(b: &[u8]) -> String {
    format!(
        "type={} level={} inUse={} hasFuel={} fuel~{} inUseFuel={}",
        b.get(0x32).copied().unwrap_or(0),
        b.get(0x33).copied().unwrap_or(0),
        b.get(0x22).copied().unwrap_or(0),
        b.get(0x34).copied().unwrap_or(0),
        i16_at(b, 0x30) / 16,
        b.get(0x35).copied().unwrap_or(0)
    )
}

fn place_bench(ctx: &mut Ctx, a: &mut Session, ty: u8, pos: (i32, i32), label: &str) -> Option<(u64, Vec<u8>)> {
    a.place_workbench(
        PlaceWorkbenchNetRequest { pos_x: pos.0 as u32, pos_y: pos.1 as u32, workbench_type: ty, ..Default::default() },
        None,
    )
    .unwrap();
    let r = wait_created(a, obj::WORKBENCH, 5000);
    ctx.obs.add(format!("{}.created", label), r.as_ref().map(|(_, b)| wb_summary(b)).unwrap_or_else(|| "no".into()));
    r
}

fn bench_update(ctx: &mut Ctx, a: &mut Session, blob: &[u8], label: &str, f: impl FnOnce(&mut Vec<u8>)) {
    let mark = a.received().len();
    let tail = owner_tail(a);
    let b = update_blob(blob, WB_HEADER, Some(tail), f);
    a.send_object_batch(BatchKind::Update, obj::WORKBENCH, &[b]).unwrap();
    a.wait_quiet(800, 4000).unwrap();
    // the first echo only: the periodic fuel-burn update (every 6 s) may land in the same window
    let echo = objects_since(a, mark, obj::WORKBENCH).iter().find(|(k, _)| *k == 0x08).map(|(_, b)| wb_summary(b));
    ctx.obs.add(format!("{}.echo", label), echo.unwrap_or_default());
}

fn craft_play(ctx: &mut Ctx) {
    let Some((mut a, bh, bx, by)) = spawn(ctx, "CRAFTA", "Cal", 0) else { return };
    absorb(&mut a, &mut ctx.obs, "join");
    let spots = ground_spots(&a, bx + 2, by, 3, 3);
    ctx.obs.soft("spots", format!("{:?}", spots));
    if spots.len() < 3 {
        return;
    }
    // ---- basic workbench: open, ingredients leave the inventory, the product drops as a freeblock, pick it up
    a.upload_inventory(bh, &slots(8, &[(0, ITEM_DIRT, 4), (1, ITEM_ROCK, 6)])).unwrap();
    if let Some((_, wb)) = place_bench(ctx, &mut a, 2, spots[0], "workbench") {
        bench_update(ctx, &mut a, &wb, "workbench.open", |b| {
            put_u64(b, 0x18, bh);
            b[0x22] = 1;
        });
        a.upload_inventory(bh, &slots(8, &[(0, ITEM_DIRT, 4)])).unwrap();
        let fid = a.request_unique_id().unwrap().unwrap_or(0);
        let mut fb = vec![0u8; 0x38];
        put_u64(&mut fb, 0, fid);
        put_u32(&mut fb, 8, spots[0].0 as u32);
        put_u32(&mut fb, 12, spots[0].1 as u32 + 1);
        put_u64(&mut fb, 0x18, bh);
        put_u16(&mut fb, 0x20, ITEM_CHEST);
        let mut p = vec![0x14];
        p.extend(plist_gz(&PV::array(vec![PV::data(fb)])).unwrap());
        a.send_raw(&p).unwrap();
        let made = wait_created(&mut a, T_FREEBLOCK, 4000);
        ctx.obs.add("craft.freeblock_created", made.as_ref().map(|(_, b)| u16_at(b, 0x20)).unwrap_or(0));
        if let Some((id, _)) = made {
            let mark = a.received().len();
            a.pickup(bh, &[id]).unwrap();
            a.wait_quiet(800, 4000).unwrap();
            let r = packets_since(&a, mark, 0x13);
            ctx.obs.add("craft.pickup_accepted", r.first().and_then(|r| r.get(1).copied()).unwrap_or(255));
            a.upload_inventory(bh, &slots(8, &[(0, ITEM_DIRT, 4), (2, ITEM_CHEST, 1)])).unwrap();
        }
        bench_update(ctx, &mut a, &wb, "workbench.close", |b| {
            put_u64(b, 0x18, 0);
            b[0x22] = 0;
        });
        absorb(&mut a, &mut ctx.obs, "workbench");
    }
    // ---- furnace (coal, 0.5) and kiln (wood, 0.2): open with fuel, let it burn, close
    for (ty, pos, fuel, label) in [(9u8, spots[1], 127i16, "furnace"), (8u8, spots[2], 51i16, "kiln")] {
        let Some((_, wb)) = place_bench(ctx, &mut a, ty, pos, label) else { continue };
        bench_update(ctx, &mut a, &wb, &format!("{}.fuel", label), |b| {
            put_u64(b, 0x18, bh);
            b[0x22] = 1;
            put_u64(b, 0x28, bh);
            put_u16(b, 0x30, fuel as u16);
            b[0x35] = 1;
        });
        let mark = a.received().len();
        a.run_for(13_000).unwrap();
        let burn: Vec<String> = objects_since(&a, mark, obj::WORKBENCH)
            .iter()
            .filter(|(k, _)| *k == 0x08)
            .map(|(_, b)| wb_summary(b))
            .collect();
        // the periodic burn update follows the wall clock (every 6 s): its count is soft, its content is not
        ctx.obs.soft(format!("{}.burn_updates", label), burn.len());
        ctx.obs.add(format!("{}.burn_last", label), burn.last().cloned().unwrap_or_default());
        bench_update(ctx, &mut a, &wb, &format!("{}.close", label), |b| {
            put_u64(b, 0x18, 0);
            b[0x22] = 0;
            b[0x35] = 0;
            put_u64(b, 0x28, 0);
            put_u16(b, 0x30, i16_at(&wb, 0x30) as u16);
        });
        absorb(&mut a, &mut ctx.obs, label);
    }
    a.disconnect();
}

fn craft_reload(ctx: &mut Ctx) {
    let Some((mut a, _bh, _, _)) = spawn(ctx, "CRAFTA", "Cid", 0) else { return };
    let benches = creation_blobs(&a, obj::WORKBENCH);
    ctx.obs.add("reload.workbenches", benches.len());
    for b in benches.values() {
        ctx.obs.add(format!("reload.workbench.type{}", b.get(0x32).copied().unwrap_or(0)), wb_summary(b));
    }
    // soft, like freeblock creations elsewhere: plants and trees drop more of them the longer the world ran
    ctx.obs.soft("reload.freeblocks", creation_blobs(&a, T_FREEBLOCK).len());
    absorb(&mut a, &mut ctx.obs, "reload");
    a.disconnect();
}

// ------------------------------------------------------------------------------------------------ sleeping

/// Sleep in a bed (bed in use + blockhead lying on it): the world fast-forwards (heartbeat 0x17 byte 8) until the
/// blockhead wakes up.
pub fn sleep() -> Scenario {
    Scenario { name: "sleep", play: sleep_play, reload: None, save: true, console: false }
}

/// A blockhead update lying on (or getting up from) the bed at `pos`.
pub fn bed_update(bh: u64, bed: u64, pos: (i32, i32), asleep: bool) -> Vec<u8> {
    let mut b = bh_update(bh, pos, pos);
    if asleep {
        put_u32(&mut b, 0x28, pos.0 as u32);
        put_u32(&mut b, 0x2c, pos.1 as u32);
        b[0x38] = 1; // interacting
        b[0x3b] = 5; // USE_INTERACTIVE_OBJECT
        b[0x3e] = 5; // LIE_ON_BED
        put_u64(&mut b, 0x50, bed);
    }
    b
}

fn fast_forward_flags(a: &Session, mark: usize) -> Vec<u8> {
    let mut v: Vec<u8> = Vec::new();
    for p in packets_since(a, mark, 0x17) {
        let f = p.get(9).copied().unwrap_or(255);
        if v.last() != Some(&f) {
            v.push(f);
        }
    }
    v
}

/// Lie down on the bed at `bed_pos` and stay for `ms`; returns the heartbeat fast-forward flag sequence.
pub fn sleep_in_bed(a: &mut Session, bh: u64, bed: (u64, &[u8]), pos: (i32, i32), ms: u64) -> (Vec<u8>, Vec<u8>) {
    let tail = owner_tail(a);
    let use_bed = |in_use: bool| {
        update_blob(bed.1, 0x30, Some(tail.clone()), |b| {
            put_u64(b, 0x18, if in_use { bh } else { 0 });
            b[0x22] = in_use as u8;
        })
    };
    let mark = a.received().len();
    a.send_object_batch(BatchKind::Update, T_BED, &[use_bed(true)]).unwrap();
    for _ in 0..4 {
        a.send_object_batch(BatchKind::Update, obj::BLOCKHEAD, &[bed_update(bh, bed.0, pos, true)]).unwrap();
        a.run_for(500).unwrap();
    }
    a.run_for(ms).unwrap();
    let asleep = fast_forward_flags(a, mark);
    let mark = a.received().len();
    for _ in 0..3 {
        a.send_object_batch(BatchKind::Update, obj::BLOCKHEAD, &[bed_update(bh, bed.0, pos, false)]).unwrap();
        a.run_for(500).unwrap();
    }
    a.send_object_batch(BatchKind::Update, T_BED, &[use_bed(false)]).unwrap();
    a.run_for(4000).unwrap();
    (asleep, fast_forward_flags(a, mark))
}

fn sleep_play(ctx: &mut Ctx) {
    let Some((mut a, bh, bx, by)) = spawn(ctx, "SLEEPA", "Sam", 0) else { return };
    absorb(&mut a, &mut ctx.obs, "join");
    let spots = ground_spots(&a, bx + 2, by, 3, 1);
    let Some(&pos) = spots.first() else { return };
    let Some((bed, blob)) = place(ctx, &mut a, bh, ITEM_BED, T_BED, pos, "bed") else { return };
    ctx.obs.add("bed.item", u16_at(&blob, 0x28));
    absorb(&mut a, &mut ctx.obs, "bed_place");
    let (asleep, awake) = sleep_in_bed(&mut a, bh, (bed, &blob), pos, 8000);
    ctx.obs.add("sleep.fast_forward", format!("{:?}", asleep));
    ctx.obs.add("wake.fast_forward", format!("{:?}", awake));
    absorb(&mut a, &mut ctx.obs, "slept");
    a.disconnect();
}

// ------------------------------------------------------------------------------------------------ NPC feeding / taming, eggs

/// Feed a dodo (0x45 creation-data update with interactionType FEED) twice, sleep to let the tame cooldown pass,
/// feed again (tame), lay an egg (0x10 type 91); reload and check the tame.
pub fn npc() -> Scenario {
    Scenario { name: "npc", play: npc_play, reload: Some(npc_reload), save: true, console: false }
}

fn npc_state(b: &[u8]) -> String {
    format!(
        "fed={} tameType={} cooldown={} fullness~{}",
        b.get(0x36).copied().unwrap_or(0),
        b.get(0x37).copied().unwrap_or(0),
        i16_at(b, 0x3e) > 0,
        i16_at(b, 0x38) / 1000
    )
}

fn feed(ctx: &mut Ctx, a: &mut Session, bh: u64, ty: u8, id: u64, label: &str) {
    let Some(base) = creation_blobs(a, ty).get(&id).cloned() else {
        ctx.obs.add(format!("{}.npc_known", label), false);
        return;
    };
    let mut b = base.clone();
    put_u64(&mut b, 0x28, bh);
    b[0x40] = 1; // FEED
    put_u16(&mut b, 0x32, 0);
    let mark = a.received().len();
    a.send_object_batch(BatchKind::CreationDataUpdate, ty, &[b]).unwrap();
    a.wait_quiet(1000, 4000).unwrap();
    let r: Vec<String> = objects_since(a, mark, ty)
        .iter()
        .filter(|(k, b)| *k == 0x46 && u64_at(b, 0) == id)
        .map(|(_, b)| npc_state(b))
        .collect();
    ctx.obs.add(format!("{}.reply", label), r.join(" / "));
}

fn npc_play(ctx: &mut Ctx) {
    let Some((mut a, bh, bx, by)) = spawn(ctx, "NPCA", "Nat", 0) else { return };
    request_around(&mut a, bx, by, 3, 1);
    a.wait_quiet(1000, 6000).unwrap();
    absorb(&mut a, &mut ctx.obs, "join");
    let dodos = creation_blobs(&a, T_DODO);
    // ids follow the shared rand() stream: pick the dodo closest to the blockhead
    let pick = dodos.iter().min_by_key(|(_, b)| (u32::from_le_bytes(b[8..12].try_into().unwrap()) as i32 - bx).abs());
    ctx.obs.add("dodo.found", pick.is_some());
    let spots = ground_spots(&a, bx + 2, by, 3, 2);
    if let Some((&id, _)) = pick {
        feed(ctx, &mut a, bh, T_DODO, id, "feed1");
        feed(ctx, &mut a, bh, T_DODO, id, "feed2_cooldown");
        if let Some(&pos) = spots.first() {
            if let Some((bed, blob)) = place(ctx, &mut a, bh, ITEM_BED, T_BED, pos, "bed") {
                let (ff, _) = sleep_in_bed(&mut a, bh, (bed, &blob), pos, 40_000);
                ctx.obs.add("sleep.fast_forward", format!("{:?}", ff));
            }
        }
        feed(ctx, &mut a, bh, T_DODO, id, "feed3_tame");
    }
    if let Some(&pos) = spots.get(1) {
        fill_item(ctx, &mut a, bh, ITEM_DODO_EGG, T_EGG, pos, None, "egg");
    }
    absorb(&mut a, &mut ctx.obs, "fed");
    a.disconnect();
}

fn npc_reload(ctx: &mut Ctx) {
    let Some((mut a, _bh, bx, by)) = spawn(ctx, "NPCA", "Ned", 0) else { return };
    request_around(&mut a, bx, by, 3, 1);
    a.wait_quiet(1000, 6000).unwrap();
    let tamed = creation_blobs(&a, T_DODO).values().filter(|b| b.get(0x37) == Some(&1)).count();
    ctx.obs.add("reload.dodos_tamed_by_me", tamed);
    ctx.obs.add("reload.eggs", creation_blobs(&a, T_EGG).len());
    absorb(&mut a, &mut ctx.obs, "reload");
    a.disconnect();
}

// ------------------------------------------------------------------------------------------------ vehicles

/// Rails (0x10 type 199), a hand car on them (204), a boat on water (111) if there is water nearby; reload.
pub fn vehicles() -> Scenario {
    Scenario { name: "vehicles", play: vehicles_play, reload: Some(vehicles_reload), save: true, console: false }
}

fn vehicles_play(ctx: &mut Ctx) {
    let Some((mut a, bh, bx, by)) = spawn(ctx, "RAILA", "Ray", 0) else { return };
    request_around(&mut a, bx, by, 6, 1);
    a.wait_quiet(1000, 6000).unwrap();
    absorb(&mut a, &mut ctx.obs, "join");
    // four rails on flat ground: the first run of four columns with the same ground height, nearest first
    let mut run: Vec<(i32, i32)> = Vec::new();
    'search: for dx in 2..150 {
        for x0 in [bx + dx, bx - dx - 3] {
            let ys: Vec<Option<i32>> = (0..4).map(|i| ground_at(&a, x0 + i, by + 16)).collect();
            if let Some(y) = ys[0] {
                if ys.iter().all(|v| *v == Some(y)) {
                    run = (0..4).map(|i| (x0 + i, y)).collect();
                    break 'search;
                }
            }
        }
    }
    ctx.obs.soft("rails.spots", format!("{:?}", run));
    for (i, p) in run.iter().enumerate() {
        fill_item(ctx, &mut a, bh, ITEM_RAIL, T_RAIL, *p, None, &format!("rail{}", i));
    }
    if let Some(p) = run.get(1) {
        fill_item(ctx, &mut a, bh, ITEM_HAND_CAR, T_HAND_CAR, *p, None, "hand_car");
    }
    // the nearest surface water tile within 3 blocks
    let mut water = None;
    'outer: for dx in 0..190 {
        for x in [bx + dx, bx - dx] {
            for y in (by - 20..by + 20).rev() {
                let (Some(t), Some(up)) = (tile_at(&a, x, y), tile_at(&a, x, y + 1)) else { continue };
                if t[0] == 3 && up[0] == 2 {
                    water = Some((x, y));
                    break 'outer;
                }
            }
        }
    }
    ctx.obs.add("boat.water", format!("{:?}", water));
    if let Some(w) = water {
        fill_item(ctx, &mut a, bh, ITEM_BOAT, T_BOAT, w, None, "boat");
    }
    absorb(&mut a, &mut ctx.obs, "placed");
    a.disconnect();
}

fn vehicles_reload(ctx: &mut Ctx) {
    let Some((mut a, _bh, bx, by)) = spawn(ctx, "RAILA", "Rex", 0) else { return };
    request_around(&mut a, bx, by, 6, 1);
    a.wait_quiet(1000, 6000).unwrap();
    for (ty, l) in [(T_RAIL, "rails"), (T_HAND_CAR, "hand_cars"), (T_BOAT, "boats")] {
        ctx.obs.add(format!("reload.{}", l), creation_blobs(&a, ty).len());
    }
    absorb(&mut a, &mut ctx.obs, "reload");
    a.disconnect();
}

// ------------------------------------------------------------------------------------------------ trading

/// Trade transactions (0x2e -> 0x2f to everyone), a trade portal and a trading post (coins via 0x0b); reload.
pub fn trade() -> Scenario {
    Scenario { name: "trade", play: trade_play, reload: Some(trade_reload), save: true, console: false }
}

fn prices(p: &[u8]) -> String {
    let Some(PV::Dict(dd)) = gz_plist_from(&p[1..]) else { return "undecodable".into() };
    let mut v: Vec<String> = dd.map.iter().map(|(k, v)| format!("{}={:.6}", k, v.double_value())).collect();
    v.sort();
    v.join(",")
}

fn trade_tx(ctx: &mut Ctx, a: &mut Session, others: &mut [&mut Session], label: &str, sold: &[(&str, f64)]) {
    let mut m: GsDict<String, PV> = GsDict::new();
    for (k, c) in sold {
        m.insert(k.to_string(), PV::real_f64(*c));
    }
    let mark = a.received().len();
    let marks: Vec<usize> = others.iter().map(|o| o.received().len()).collect();
    a.trade(&PV::dict(m)).unwrap();
    a.wait_quiet(800, 4000).unwrap();
    let mine: Vec<String> = packets_since(a, mark, 0x2f).iter().map(|p| prices(p)).collect();
    ctx.obs.add(format!("{}.sender", label), mine.join(" / "));
    for (i, o) in others.iter_mut().enumerate() {
        o.run_for(300).unwrap();
        let theirs: Vec<String> = packets_since(o, marks[i], 0x2f).iter().map(|p| prices(p)).collect();
        ctx.obs.add(format!("{}.other{}", label, i), theirs.join(" / "));
    }
}

fn trade_play(ctx: &mut Ctx) {
    let Some((mut a, bh, bx, by)) = spawn(ctx, "TRADEA", "Tia", 0) else { return };
    let mut b = ctx.join("TRADEB");
    absorb(&mut a, &mut ctx.obs, "join");
    trade_tx(ctx, &mut a, &mut [&mut b], "tx1", &[("298", 5.0), ("21", -2.0), ("4", 0.001)]);
    trade_tx(ctx, &mut a, &mut [&mut b], "tx2_empty", &[]);
    trade_tx(ctx, &mut b, &mut [&mut a], "tx3_by_b", &[("298", 1.0)]);
    let spots = ground_spots(&a, bx + 2, by, 3, 2);
    ctx.obs.soft("spots", format!("{:?}", spots));
    if spots.len() == 2 {
        if let Some((_, tp)) = place(ctx, &mut a, bh, ITEM_TRADE_PORTAL, T_TRADE_PORTAL, spots[0], "trade_portal") {
            ctx.obs.add("trade_portal.level", tp.get(0x28).copied().unwrap_or(0));
        }
        if let Some((_, post)) = place(ctx, &mut a, bh, ITEM_TRADING_POST, T_TRADING_POST, spots[1], "trading_post") {
            let mark = a.received().len();
            let tail = d(vec![
                ("ownerID", PV::string(a.id.player_id())),
                ("ownerName", PV::string("TRADEA")),
                ("sellerClientName", PV::string("TRADEA")),
                ("sellSlot", PV::array(vec![PV::data(item(21))])),
            ]);
            let blob = update_blob(&post, 0x30, Some(tail), |x| {
                put_u32(x, 0x28, 10);
                put_u16(x, 0x2c, 2);
            });
            a.send_object_batch(BatchKind::Update, T_TRADING_POST, &[blob]).unwrap();
            a.wait_quiet(800, 4000).unwrap();
            let echo: Vec<String> = objects_since(&a, mark, T_TRADING_POST)
                .iter()
                .map(|(_, x)| {
                    format!("coins={} tier={}", u32::from_le_bytes(x[0x28..0x2c].try_into().unwrap()), u16_at(x, 0x2c))
                })
                .collect();
            ctx.obs.add("trading_post.echo", echo.join(" / "));
        }
    }
    absorb(&mut a, &mut ctx.obs, "placed");
    b.disconnect();
    a.disconnect();
}

fn trade_reload(ctx: &mut Ctx) {
    let Some((mut a, _bh, _, _)) = spawn(ctx, "TRADEA", "Tod", 0) else { return };
    trade_tx(ctx, &mut a, &mut [], "reload.tx", &[("298", 1.0), ("21", 1.0)]);
    for x in creation_blobs(&a, T_TRADING_POST).values() {
        ctx.obs.add(
            "reload.trading_post",
            format!(
                "coins={} tier={} sell={}",
                u32::from_le_bytes(x[0x28..0x2c].try_into().unwrap()),
                u16_at(x, 0x2c),
                tail_plist(x, 0x30).and_then(|p| p.object_for_key("sellSlot").map(slot_summary)).unwrap_or_default()
            ),
        );
    }
    ctx.obs.add("reload.trade_portals", creation_blobs(&a, T_TRADE_PORTAL).len());
    absorb(&mut a, &mut ctx.obs, "reload");
    a.disconnect();
}

// ------------------------------------------------------------------------------------------------ portal chest

/// Portal chest transactions: 0x3e is acknowledged (0x3f) after the next inventory upload, 0x40 resends answer
/// 0x3f (known id) or 0x41; the last acknowledged id survives a restart. Also places a portal chest item.
pub fn portal_chest() -> Scenario {
    Scenario {
        name: "portal_chest",
        play: portal_chest_play,
        reload: Some(portal_chest_reload),
        save: true,
        console: false,
    }
}

fn pc_step(ctx: &mut Ctx, a: &mut Session, label: &str, f: impl FnOnce(&mut Session)) {
    let mark = a.received().len();
    f(a);
    a.wait_quiet(800, 4000).unwrap();
    let mut r: Vec<String> = Vec::new();
    for t in [0x3f, 0x41] {
        r.extend(packets_since(a, mark, t).iter().map(|p| to_hex(p)));
    }
    ctx.obs.add(format!("{}.reply", label), r.join(","));
}

fn portal_chest_play(ctx: &mut Ctx) {
    let Some((mut a, bh, bx, by)) = spawn(ctx, "PORTA", "Pat", 0) else { return };
    absorb(&mut a, &mut ctx.obs, "join");
    if let Some(&pos) = ground_spots(&a, bx + 2, by, 3, 1).first() {
        if let Some((_, c)) = place(ctx, &mut a, bh, ITEM_PORTAL_CHEST, obj::CHEST, pos, "portal_chest") {
            ctx.obs.add("portal_chest.chest_type", c.get(0x38).copied().unwrap_or(255));
        }
    }
    pc_step(ctx, &mut a, "tx7", |s| s.portal_chest(7, false).unwrap());
    pc_step(ctx, &mut a, "tx7_inventory", |s| s.upload_inventory(bh, &slots(8, &[(0, ITEM_DIRT, 1)])).unwrap());
    pc_step(ctx, &mut a, "resend7", |s| s.portal_chest(7, true).unwrap());
    pc_step(ctx, &mut a, "resend8", |s| s.portal_chest(8, true).unwrap());
    pc_step(ctx, &mut a, "tx9_disconnect_pending", |s| s.portal_chest(9, false).unwrap());
    absorb(&mut a, &mut ctx.obs, "done");
    a.disconnect();
}

fn portal_chest_reload(ctx: &mut Ctx) {
    let Some((mut a, bh, _, _)) = spawn(ctx, "PORTA", "Pip", 0) else { return };
    pc_step(ctx, &mut a, "reload.resend7", |s| s.portal_chest(7, true).unwrap());
    pc_step(ctx, &mut a, "reload.resend9", |s| s.portal_chest(9, true).unwrap());
    pc_step(ctx, &mut a, "reload.inventory_after_disconnect", |s| s.upload_inventory(bh, &slots(8, &[])).unwrap());
    absorb(&mut a, &mut ctx.obs, "reload");
    a.disconnect();
}

// ------------------------------------------------------------------------------------------------ custom rules / welcome

/// Welcome message get/set (0x35 -> 0x36, 0x34 admin only, 0x01 summary), custom-rule updates (0x42 admin only,
/// forwarded to the others, logged), heartbeat with rules (0x44); reload.
pub fn rules_welcome() -> Scenario {
    Scenario { name: "rules_welcome", play: rules_play, reload: Some(rules_reload), save: true, console: true }
}

fn welcome(ctx: &mut Ctx, s: &mut Session, label: &str) {
    let mark = s.received().len();
    s.welcome_get().unwrap();
    s.wait_quiet(600, 3000).unwrap();
    let r: Vec<String> = packets_since(s, mark, 0x36)
        .iter()
        .map(|p| match blockheads_server::protocol::codec::plist_from(&p[1..]) {
            Some(pl) => {
                let rules = pl.object_for_key("customRules");
                format!(
                    "welcome={} rules={} health={} daynight={}",
                    pstr(&pl, "welcomeMessage"),
                    match rules {
                        Some(PV::Dict(dd)) => dd.map.len().to_string(),
                        _ => "none".into(),
                    },
                    rules.map(|r| pstr(r, "health")).unwrap_or_default(),
                    rules.map(|r| pstr(r, "day/night cycle")).unwrap_or_default()
                )
            }
            None => "undecodable".into(),
        })
        .collect();
    ctx.obs.add(format!("{}.0x36", label), r.join(" / "));
}

fn rule_update(ctx: &mut Ctx, from: &mut Session, other: &mut Session, label: &str, k: &str, v: &str) {
    let log = ctx.server.log().len();
    let (m1, m2) = (from.received().len(), other.received().len());
    let p = d(vec![(k, PV::string(v))]);
    from.send_raw(&blockheads_server::protocol::messages::client_gz_plist(0x42, &p)).unwrap();
    from.wait_quiet(800, 4000).unwrap();
    other.run_for(500).unwrap();
    ctx.obs.add(format!("{}.sender_0x42", label), packets_since(from, m1, 0x42).len());
    let fwd: Vec<String> = packets_since(other, m2, 0x42)
        .iter()
        .map(|p| gz_plist_from(&p[1..]).map(|x| pstr(&x, k)).unwrap_or_else(|| "undecodable".into()))
        .collect();
    ctx.obs.add(format!("{}.other_0x42", label), fwd.join(","));
    let lines: Vec<String> = ctx.log_since(log).into_iter().filter(|l| l.contains("ustom rule")).collect();
    ctx.obs.add(format!("{}.log", label), lines.join(" | "));
}

fn rules_play(ctx: &mut Ctx) {
    ctx.console("/admin rulesa");
    let mut a = ctx.join("RULESA");
    let mut b = ctx.join("RULESB");
    ctx.obs.add("summary.welcome", a.state.summary.as_ref().map(|p| pstr(p, "welcomeMessage")).unwrap_or_default());
    welcome(ctx, &mut a, "initial");
    b.welcome_set("set by a plain player").unwrap();
    b.wait_quiet(600, 3000).unwrap();
    welcome(ctx, &mut b, "after_plain_set");
    a.welcome_set("Hello from the admin").unwrap();
    a.wait_quiet(600, 3000).unwrap();
    welcome(ctx, &mut b, "after_admin_set");
    let c = ctx.join("RULESC");
    ctx.obs.add("summary_c.welcome", c.state.summary.as_ref().map(|p| pstr(p, "welcomeMessage")).unwrap_or_default());
    rule_update(ctx, &mut a, &mut b, "admin_health", "health", "easy");
    rule_update(ctx, &mut b, &mut a, "plain_health", "health", "hard");
    rule_update(ctx, &mut a, &mut b, "admin_daynight", "day/night cycle", "faster");
    welcome(ctx, &mut b, "after_rules");
    // the client's rule image check (0x44) right after the change
    let mark = a.received().len();
    a.heartbeat_request_with_rules(None).unwrap();
    a.wait_quiet(800, 4000).unwrap();
    ctx.obs.add("rules_heartbeat.0x17", packets_since(&a, mark, 0x17).len().min(1));
    ctx.obs.add("rules_heartbeat.disconnected", a.state.disconnected);
    absorb(&mut b, &mut ctx.obs, "b_rx");
    for mut s in [a, b, c] {
        s.disconnect();
    }
}

fn rules_reload(ctx: &mut Ctx) {
    let mut a = ctx.join("RULESA");
    ctx.obs
        .add("reload.summary.welcome", a.state.summary.as_ref().map(|p| pstr(p, "welcomeMessage")).unwrap_or_default());
    welcome(ctx, &mut a, "reload");
    a.disconnect();
}
