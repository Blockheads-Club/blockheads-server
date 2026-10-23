//! Scenario builder (Rust DSL), text-script parser and runner.
//!
//! 6.5 (built-in scenarios).

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use base64::Engine;
use blockheads_server::foundation::gsdict::GsDict;
use blockheads_server::plist::value::PlistValue as PV;

use crate::client::{ClientConfig, Rx};
use crate::session::{
    BlockheadSpec, FillNetRequest, Identity, JoinOutcome, PaintNetRequest, PlaceWorkbenchNetRequest, PlistDialect,
    RemoveNetRequest, Session,
};
use crate::trace::{from_hex, parse_json, Json, TraceSink};

/// Which blockhead an inventory upload targets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockheadRef {
    /// the last blockhead created by (announced to) the active client
    Last,
    Id(u64),
}

/// One scenario step.
#[derive(Clone, Debug)]
pub enum Step {
    /// connect + join (outcome recorded as a trace marker)
    Connect {
        id: Identity,
    },
    /// connect, wait 0x23, do not send 0x1f
    JoinRaw {
        alias: String,
    },
    UseClient(String),
    WaitFor {
        ty: u8,
        timeout_ms: Option<u64>,
    },
    WaitQuiet(u64),
    Sleep(u64),
    InitialData,
    RequestBlocks {
        idx: Vec<i32>,
        create: bool,
    },
    RequestBlockXY {
        mx: i32,
        my: i32,
        create: bool,
    },
    RequestBlockTile {
        tile_x: i32,
        tile_y: i32,
        create: bool,
    },
    UniqueId,
    CreateBlockhead(BlockheadSpec),
    UploadInventory {
        blockhead: BlockheadRef,
        slots: PV,
    },
    Fill(FillNetRequest),
    Remove(RemoveNetRequest),
    Paint(PaintNetRequest),
    PlaceWorkbench {
        r: PlaceWorkbenchNetRequest,
        tail: Option<PV>,
    },
    HeartbeatOld,
    HeartbeatRules(Option<[u8; 64]>),
    Chat(String),
    Command(String),
    Raw(Vec<u8>),
    Xml {
        ty: u8,
        text: String,
    },
    Gz {
        ty: u8,
        text: String,
    },
    Zoom {
        player_id: String,
        cycle: i32,
    },
    Expect(u8),
    ExpectNone {
        ty: u8,
        within_ms: u64,
    },
    ExpectRejection(u8),
    Disconnect,
}

/// Scenario failure.
#[derive(Debug)]
pub enum ScenarioError {
    Timeout { line: usize, what: String },
    Unexpected { line: usize, what: String },
    Io(io::Error),
    Parse { line: usize, msg: String },
}

impl fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScenarioError::Timeout { line, what } => write!(f, "FAIL {} timeout: {}", line, what),
            ScenarioError::Unexpected { line, what } => write!(f, "FAIL {} {}", line, what),
            ScenarioError::Io(e) => write!(f, "io error: {}", e),
            ScenarioError::Parse { line, msg } => write!(f, "parse error at line {}: {}", line, msg),
        }
    }
}
impl std::error::Error for ScenarioError {}
impl From<io::Error> for ScenarioError {
    fn from(e: io::Error) -> Self {
        ScenarioError::Io(e)
    }
}

impl ScenarioError {
    /// True for connect-level IO failures (CLI exit code 3).
    pub fn is_connect_failure(&self) -> bool {
        matches!(self, ScenarioError::Io(e) if e.kind() == io::ErrorKind::TimedOut || e.kind() == io::ErrorKind::ConnectionRefused)
    }
}

/// Run parameters.
#[derive(Clone, Debug)]
pub struct RunOptions {
    pub trace_dir: Option<PathBuf>,
    /// default 5000
    pub default_timeout_ms: u64,
    /// Bind the n-th client to UDP port `base + n` (the server keys connections by `host:port` and picks the send
    /// channel from their hash order, so runs only compare channel for channel with fixed client ports).
    pub client_port_base: Option<u16>,
}
impl Default for RunOptions {
    fn default() -> Self {
        RunOptions { trace_dir: None, default_timeout_ms: 5000, client_port_base: None }
    }
}

/// What a run produced.
#[derive(Clone, Debug, Default)]
pub struct RunReport {
    pub traces: Vec<(String, PathBuf)>,
    /// `(alias, type byte, count)` of received packets
    pub received_counts: Vec<(String, u8, usize)>,
    pub notes: Vec<String>,
}

/// A scripted multi-client session.
#[derive(Clone, Debug)]
pub struct Scenario {
    pub name: String,
    pub steps: Vec<Step>,
    /// source line of each step (0 for builder-made steps)
    pub lines: Vec<usize>,
}

macro_rules! b {
    ($(#[$m:meta])* $name:ident($($a:ident: $t:ty),*) => $step:expr) => {
        $(#[$m])*
        pub fn $name(self, $($a: $t),*) -> Self { self.push($step) }
    };
}

impl Scenario {
    pub fn new(name: &str) -> Scenario {
        Scenario { name: name.to_string(), steps: Vec::new(), lines: Vec::new() }
    }

    fn push(mut self, s: Step) -> Self {
        self.steps.push(s);
        self.lines.push(0);
        self
    }
    fn push_line(&mut self, s: Step, line: usize) {
        self.steps.push(s);
        self.lines.push(line);
    }

    /// `Session::connect` + `join` with the default identity.
    pub fn connect(self, alias: &str) -> Self {
        self.push(Step::Connect { id: Identity::new(alias) })
    }
    /// connect + join with an explicit identity.
    pub fn connect_with(self, id: Identity) -> Self {
        self.push(Step::Connect { id })
    }
    b!(/// connect and wait for 0x23 only
        join_raw(alias: &str) => Step::JoinRaw { alias: alias.to_string() });
    b!(/// wait for a packet type (default timeout)
        wait_for(ty: u8) => Step::WaitFor { ty, timeout_ms: None });
    b!(wait_for_ms(ty: u8, timeout_ms: u64) => Step::WaitFor { ty, timeout_ms: Some(timeout_ms) });
    b!(wait_quiet(quiet_ms: u64) => Step::WaitQuiet(quiet_ms));
    b!(/// pumps ENet while waiting
        sleep(ms: u64) => Step::Sleep(ms));
    b!(/// 0x05
        initial_data() => Step::InitialData);
    b!(request_blocks(idx: &[i32], create: bool) => Step::RequestBlocks { idx: idx.to_vec(), create });
    b!(request_block_at_tile(tile_x: i32, tile_y: i32, create: bool) => Step::RequestBlockTile { tile_x, tile_y, create });
    b!(request_block_xy(mx: i32, my: i32, create: bool) => Step::RequestBlockXY { mx, my, create });
    b!(/// 0x0e, waits for 0x0f
        unique_id() => Step::UniqueId);
    b!(create_blockhead(spec: BlockheadSpec) => Step::CreateBlockhead(spec));
    b!(upload_inventory(blockhead: BlockheadRef, slots: PV) => Step::UploadInventory { blockhead, slots });
    b!(fill(r: FillNetRequest) => Step::Fill(r));
    b!(remove(r: RemoveNetRequest) => Step::Remove(r));
    b!(paint(r: PaintNetRequest) => Step::Paint(r));
    b!(place_workbench(r: PlaceWorkbenchNetRequest, tail: Option<PV>) => Step::PlaceWorkbench { r, tail });
    b!(heartbeat_request() => Step::HeartbeatOld);
    b!(heartbeat_request_with_rules() => Step::HeartbeatRules(None));
    b!(chat(text: &str) => Step::Chat(text.to_string()));
    b!(command(line: &str) => Step::Command(line.to_string()));
    /// raw packet from a hex string (panics on bad hex; use the text DSL for error reporting)
    pub fn raw(self, hex: &str) -> Self {
        self.push(Step::Raw(from_hex(hex).expect("bad hex in Scenario::raw")))
    }
    b!(xml(ty: u8, plist: &str) => Step::Xml { ty, text: plist.to_string() });
    b!(/// assert that a packet of this type has been received since the scenario began
        expect(ty: u8) => Step::Expect(ty));
    b!(expect_none(ty: u8, within_ms: u64) => Step::ExpectNone { ty, within_ms });
    b!(expect_rejection(reason: u8) => Step::ExpectRejection(reason));
    b!(disconnect() => Step::Disconnect);
    b!(/// switch the active client
        then_client(alias: &str) => Step::UseClient(alias.to_string()));

    // ------------------------------------------------------------------ text DSL (6.2)

    /// Parse the text DSL of. Errors carry the 1-based line number.
    pub fn parse(text: &str) -> Result<Scenario, ScenarioError> {
        let mut sc = Scenario::new("script");
        for (n, raw) in text.lines().enumerate() {
            let line = n + 1;
            let l = raw.trim();
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            let step = parse_line(l).map_err(|msg| ScenarioError::Parse { line, msg })?;
            sc.push_line(step, line);
        }
        Ok(sc)
    }

    // ------------------------------------------------------------------ runner (6.1)

    /// Execute against `cfg.addr`. Traces go to `<trace_dir>/<scenario>_<alias>.jsonl`.
    pub fn run(&self, cfg: &ClientConfig, out: &RunOptions) -> Result<RunReport, ScenarioError> {
        let mut r = Runner { sc: self, cfg, out, clients: Vec::new(), active: None, report: RunReport::default() };
        let res = r.run_steps();
        // flush and close: every client still connected is disconnected, counts are collected
        let mut counts: BTreeMap<(String, u8), usize> = BTreeMap::new();
        for s in r.clients.iter_mut() {
            for rx in s.received() {
                if let Some(&ty) = rx.data.first() {
                    *counts.entry((s.id.alias.clone(), ty)).or_insert(0) += 1;
                }
            }
            r.report.notes.extend(s.state.notes.iter().map(|n| format!("{}: {}", s.id.alias, n)));
            if s.client.is_connected() {
                s.disconnect();
            }
        }
        r.report.received_counts = counts.into_iter().map(|((a, t), c)| (a, t, c)).collect();
        res.map(|_| r.report)
    }
}

struct Runner<'a> {
    sc: &'a Scenario,
    cfg: &'a ClientConfig,
    out: &'a RunOptions,
    clients: Vec<Session>,
    active: Option<usize>,
    report: RunReport,
}

impl<'a> Runner<'a> {
    fn err_unexpected(line: usize, what: impl Into<String>) -> ScenarioError {
        ScenarioError::Unexpected { line, what: what.into() }
    }

    fn active_idx(&self, line: usize) -> Result<usize, ScenarioError> {
        self.active.ok_or_else(|| Self::err_unexpected(line, "no active client (use connect first)"))
    }

    fn find(&self, alias: &str) -> Option<usize> {
        self.clients.iter().position(|c| c.id.alias == alias)
    }

    /// Pump every client except `skip` once.
    fn pump_others(&mut self, skip: usize) -> io::Result<()> {
        for (i, c) in self.clients.iter_mut().enumerate() {
            if i != skip {
                c.step()?;
            }
        }
        Ok(())
    }

    /// Pump all clients for `ms`.
    fn pump_all(&mut self, ms: u64) -> io::Result<()> {
        let end = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < end {
            let mut any = 0;
            for c in self.clients.iter_mut() {
                any += c.step()?;
            }
            if self.clients.is_empty() {
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = any;
        }
        Ok(())
    }

    fn wait_active(&mut self, a: usize, ty: u8, ms: u64) -> io::Result<Option<Rx>> {
        let end = Instant::now() + Duration::from_millis(ms);
        loop {
            if let Some(rx) = self.clients[a].wait_for(ty, 10)? {
                return Ok(Some(rx));
            }
            if Instant::now() >= end || self.clients[a].state.disconnected {
                return Ok(None);
            }
            self.pump_others(a)?;
        }
    }

    fn open(&mut self, id: Identity) -> Result<usize, ScenarioError> {
        let mut cfg = self.cfg.clone();
        if let Some(base) = self.out.client_port_base {
            cfg.local_bind = Some(SocketAddr::from(([0, 0, 0, 0], base + self.clients.len() as u16)));
        }
        let mut s = Session::connect(cfg, id)?;
        if let Some(dir) = &self.out.trace_dir {
            let path = dir.join(format!("{}_{}.jsonl", self.sc.name, s.id.alias));
            let sink = TraceSink::create(&path, &self.sc.name)?;
            s.set_trace(Some(sink));
            self.report.traces.push((s.id.alias.clone(), path));
        }
        self.clients.push(s);
        Ok(self.clients.len() - 1)
    }

    fn mark(&self, i: usize, mark: &str, outcome: &str) {
        let _ = (i, mark, outcome);
    }

    fn run_steps(&mut self) -> Result<(), ScenarioError> {
        let default_to = self.out.default_timeout_ms;
        for (si, step) in self.sc.steps.iter().enumerate() {
            let line = self.sc.lines[si];
            match step {
                Step::Connect { id } => {
                    if self.find(&id.alias).is_some() {
                        return Err(Self::err_unexpected(line, format!("client {} already exists", id.alias)));
                    }
                    let i = self.open(id.clone())?;
                    self.active = Some(i);
                    let outcome = self.clients[i].join(default_to)?;
                    let text = match &outcome {
                        JoinOutcome::Approved => "approved".to_string(),
                        JoinOutcome::Rejected { reason } => format!("rejected {}", reason),
                        JoinOutcome::NoContact => "no_contact".to_string(),
                        JoinOutcome::Disconnected => "disconnected".to_string(),
                    };
                    self.report.notes.push(format!("{}: join {}", id.alias, text));
                    self.mark(i, "join", &text);
                    match outcome {
                        JoinOutcome::NoContact => {
                            return Err(ScenarioError::Timeout { line, what: format!("{}: no 0x23 / 0x01", id.alias) })
                        }
                        JoinOutcome::Disconnected => {
                            return Err(Self::err_unexpected(line, format!("{}: disconnected during join", id.alias)))
                        }
                        _ => {}
                    }
                }
                Step::JoinRaw { alias } => {
                    let i = self.open(Identity::new(alias))?;
                    self.active = Some(i);
                    if self.wait_active(i, 0x23, default_to)?.is_none() {
                        return Err(ScenarioError::Timeout { line, what: format!("{}: no 0x23", alias) });
                    }
                }
                Step::UseClient(alias) => {
                    self.active = Some(
                        self.find(alias)
                            .ok_or_else(|| Self::err_unexpected(line, format!("unknown client {}", alias)))?,
                    );
                }
                Step::Sleep(ms) => self.pump_all(*ms)?,
                Step::WaitQuiet(ms) => {
                    let a = self.active_idx(line)?;
                    let end = Instant::now() + Duration::from_millis(default_to.max(*ms) * 4);
                    let mut last = Instant::now();
                    let before: usize = self.clients[a].received().len();
                    let mut seen = before;
                    while Instant::now() < end {
                        self.clients[a].step()?;
                        self.pump_others(a)?;
                        let n = self.clients[a].received().len();
                        if n != seen {
                            seen = n;
                            last = Instant::now();
                        }
                        if last.elapsed() >= Duration::from_millis(*ms) || self.clients[a].state.disconnected {
                            break;
                        }
                    }
                }
                Step::WaitFor { ty, timeout_ms } => {
                    let a = self.active_idx(line)?;
                    let to = timeout_ms.unwrap_or(default_to);
                    if self.wait_active(a, *ty, to)?.is_none() {
                        return Err(ScenarioError::Timeout {
                            line,
                            what: format!("wait for 0x{:02x} ({} ms)", ty, to),
                        });
                    }
                }
                Step::Expect(ty) => {
                    let a = self.active_idx(line)?;
                    self.clients[a].step()?;
                    if self.clients[a].count_of(*ty) == 0 {
                        return Err(Self::err_unexpected(
                            line,
                            format!("expected a 0x{:02x} packet, none received", ty),
                        ));
                    }
                }
                Step::ExpectNone { ty, within_ms } => {
                    let a = self.active_idx(line)?;
                    let before = self.clients[a].count_of(*ty);
                    self.pump_all(*within_ms)?;
                    if self.clients[a].count_of(*ty) != before {
                        return Err(Self::err_unexpected(
                            line,
                            format!("unexpected 0x{:02x} packet within {} ms", ty, within_ms),
                        ));
                    }
                }
                Step::ExpectRejection(reason) => {
                    let a = self.active_idx(line)?;
                    if self.clients[a].state.last_rejection.is_none() {
                        self.wait_active(a, 0x26, default_to)?;
                    }
                    match self.clients[a].state.last_rejection {
                        Some(r) if r == *reason => {}
                        other => {
                            return Err(Self::err_unexpected(
                                line,
                                format!("expected rejection {}, got {:?}", reason, other),
                            ))
                        }
                    }
                }
                Step::UniqueId => {
                    let a = self.active_idx(line)?;
                    if self.clients[a].request_unique_id()?.is_none() {
                        return Err(ScenarioError::Timeout { line, what: "unique id reply (0x0f)".into() });
                    }
                }
                Step::Command(l) => {
                    let a = self.active_idx(line)?;
                    let replies = self.clients[a].command(l)?;
                    for p in replies {
                        let m = p.object_for_key("message").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        self.report.notes.push(format!("cmd {} -> {}", l, m));
                    }
                }
                Step::Disconnect => {
                    let a = self.active_idx(line)?;
                    self.clients[a].disconnect();
                }
                other => {
                    let a = self.active_idx(line)?;
                    self.simple_step(a, line, other)?;
                }
            }
        }
        Ok(())
    }

    /// Steps that are a single send on the active client.
    fn simple_step(&mut self, a: usize, line: usize, step: &Step) -> Result<(), ScenarioError> {
        let s = &mut self.clients[a];
        match step {
            Step::InitialData => s.request_initial_data()?,
            Step::RequestBlocks { idx, create } => {
                for &i in idx {
                    s.request_block(i, *create)?;
                }
            }
            Step::RequestBlockXY { mx, my, create } => s.request_block_xy(*mx, *my, *create)?,
            Step::RequestBlockTile { tile_x, tile_y, create } => {
                let i = s.macro_index_for_tile(*tile_x, *tile_y);
                s.request_block(i, *create)?;
            }
            Step::CreateBlockhead(spec) => s.create_blockhead(spec)?,
            Step::UploadInventory { blockhead, slots } => {
                let id =
                    match blockhead {
                        BlockheadRef::Id(i) => *i,
                        BlockheadRef::Last => {
                            s.state.created.iter().rev().find(|c| c.object_type == 24).map(|c| c.unique_id).ok_or_else(
                                || Self::err_unexpected(line, "no created blockhead known (@LAST_BLOCKHEAD)"),
                            )?
                        }
                    };
                s.upload_inventory(id, slots)?;
            }
            Step::Fill(r) => s.fill(*r, None)?,
            Step::Remove(r) => s.remove(*r)?,
            Step::Paint(r) => s.paint(*r)?,
            Step::PlaceWorkbench { r, tail } => s.place_workbench(*r, tail.as_ref())?,
            Step::HeartbeatOld => s.heartbeat_request_old()?,
            Step::HeartbeatRules(r) => s.heartbeat_request_with_rules(*r)?,
            Step::Chat(t) => s.chat(t)?,
            Step::Raw(b) => s.send_raw(b)?,
            Step::Xml { ty, text } => s.send_xml(*ty, text)?,
            Step::Gz { ty, text } => {
                let mut p = vec![*ty];
                p.extend(blockheads_server::protocol::codec::gz(text.as_bytes()));
                s.send_raw(&p)?;
            }
            Step::Zoom { player_id, cycle } => s.zoom_request(player_id, *cycle)?,
            _ => {}
        }
        Ok(())
    }
}

// ----------------------------------------------------------------------------------------------
// text DSL parsing
// ----------------------------------------------------------------------------------------------

fn take_word(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    match s.find(char::is_whitespace) {
        Some(i) => (&s[..i], s[i..].trim_start()),
        None => (s, ""),
    }
}

fn num<T: TryFrom<i64>>(s: &str, what: &str) -> Result<T, String> {
    let (neg, body) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s),
    };
    let v = if let Some(h) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        i64::from_str_radix(h, 16)
    } else {
        body.parse::<i64>()
    }
    .map_err(|_| format!("bad {} '{}'", what, s))?;
    let v = if neg { -v } else { v };
    T::try_from(v).map_err(|_| format!("{} '{}' out of range", what, s))
}

fn hex_type(s: &str) -> Result<u8, String> {
    let h = s.strip_prefix("0x").unwrap_or(s);
    u8::from_str_radix(h, 16).map_err(|_| format!("bad hex type '{}'", s))
}

fn create_flag(s: Option<&str>) -> Result<bool, String> {
    match s {
        None => Ok(true),
        Some(w) => match w.strip_prefix("create=").unwrap_or(w) {
            "0" => Ok(false),
            "1" => Ok(true),
            _ => Err(format!("bad create flag '{}'", w)),
        },
    }
}

fn json_to_pv(j: &Json, in_array: bool) -> PV {
    match j {
        Json::Null => PV::Null,
        Json::Bool(b) => PV::boolean(*b),
        Json::Num(n) => {
            if n.fract() == 0.0 {
                PV::int(*n as i64)
            } else {
                PV::real_f64(*n)
            }
        }
        Json::Str(s) => {
            if in_array {
                if let Ok(b) = base64::engine::general_purpose::STANDARD.decode(s) {
                    return PV::data(b);
                }
            }
            PV::string(s.clone())
        }
        Json::Arr(a) => PV::array(a.iter().map(|x| json_to_pv(x, true)).collect()),
        Json::Obj(m) => {
            let mut d: GsDict<String, PV> = GsDict::new();
            for (k, v) in m {
                d.insert(k.clone(), json_to_pv(v, false));
            }
            PV::dict(d)
        }
    }
}

fn parse_line(l: &str) -> Result<Step, String> {
    let (cmd, rest) = take_word(l);
    let args: Vec<&str> = rest.split_whitespace().collect();
    let need = |n: usize| -> Result<(), String> {
        if args.len() < n {
            Err(format!("'{}' needs {} argument(s)", cmd, n))
        } else {
            Ok(())
        }
    };
    Ok(match cmd {
        "client" => {
            need(1)?;
            let mut id = Identity::new(args[0]);
            for kv in &args[1..] {
                let (k, v) = kv.split_once('=').ok_or_else(|| format!("bad option '{}'", kv))?;
                match k {
                    "udid" => id.udid = v.to_string(),
                    "password" => id.password = Some(v.to_string()),
                    "minor" => id.minor_version = num(v, "minor")?,
                    "dialect" => {
                        id.plist_dialect = match v {
                            "apple" => PlistDialect::Apple,
                            "gnustep" => PlistDialect::GnuStep,
                            _ => return Err(format!("bad dialect '{}'", v)),
                        }
                    }
                    _ => return Err(format!("unknown client option '{}'", k)),
                }
            }
            Step::Connect { id }
        }
        "use" => {
            need(1)?;
            Step::UseClient(args[0].to_string())
        }
        "join-raw" => {
            need(1)?;
            Step::JoinRaw { alias: args[0].to_string() }
        }
        "sleep" => {
            need(1)?;
            Step::Sleep(num(args[0], "ms")?)
        }
        "quiet" => {
            need(1)?;
            Step::WaitQuiet(num(args[0], "ms")?)
        }
        "wait" => {
            need(1)?;
            Step::WaitFor { ty: hex_type(args[0])?, timeout_ms: args.get(1).map(|s| num(s, "ms")).transpose()? }
        }
        "expect" => {
            need(1)?;
            Step::Expect(hex_type(args[0])?)
        }
        "expect-none" => {
            need(2)?;
            Step::ExpectNone { ty: hex_type(args[0])?, within_ms: num(args[1], "ms")? }
        }
        "expect-reject" => {
            need(1)?;
            Step::ExpectRejection(num(args[0], "reason")?)
        }
        "send" => {
            need(1)?;
            Step::Raw(from_hex(args[0]).ok_or_else(|| format!("bad hex '{}'", args[0]))?)
        }
        "xml" | "gz" => {
            let (ty, text) = take_word(rest);
            if ty.is_empty() || text.is_empty() {
                return Err(format!("'{}' needs HEXTYPE and plist text", cmd));
            }
            let ty = hex_type(ty)?;
            if cmd == "xml" {
                Step::Xml { ty, text: text.to_string() }
            } else {
                Step::Gz { ty, text: text.to_string() }
            }
        }
        "initial-data" => Step::InitialData,
        "block" => {
            need(1)?;
            Step::RequestBlocks { idx: vec![num(args[0], "index")?], create: create_flag(args.get(1).copied())? }
        }
        "block-xy" => {
            need(2)?;
            Step::RequestBlockXY {
                mx: num(args[0], "mx")?,
                my: num(args[1], "my")?,
                create: create_flag(args.get(2).copied())?,
            }
        }
        "block-tile" => {
            need(2)?;
            Step::RequestBlockTile {
                tile_x: num(args[0], "x")?,
                tile_y: num(args[1], "y")?,
                create: create_flag(args.get(2).copied())?,
            }
        }
        "blocks" => {
            need(1)?;
            let idx = args[0].split(',').map(|s| num::<i32>(s, "index")).collect::<Result<Vec<_>, _>>()?;
            Step::RequestBlocks { idx, create: create_flag(args.get(1).copied())? }
        }
        "uid" => Step::UniqueId,
        "blockhead" => {
            need(3)?;
            Step::CreateBlockhead(BlockheadSpec::new(args[0], num(args[1], "x")?, num(args[2], "y")?))
        }
        "inventory" => {
            let (who, json) = take_word(rest);
            if who.is_empty() || json.is_empty() {
                return Err("'inventory' needs @LAST_BLOCKHEAD|ID and SLOTS_JSON".into());
            }
            let blockhead = if who == "@LAST_BLOCKHEAD" {
                BlockheadRef::Last
            } else {
                BlockheadRef::Id(num::<i64>(who, "blockhead id").map(|v| v as u64)?)
            };
            let j = parse_json(json).ok_or_else(|| "bad JSON slots".to_string())?;
            let slots = json_to_pv(&j, false);
            Step::UploadInventory { blockhead, slots }
        }
        "fill" => {
            need(3)?;
            Step::Fill(FillNetRequest {
                pos_x: num::<i64>(args[0], "x")? as u32,
                pos_y: num::<i64>(args[1], "y")? as u32,
                type_: num(args[2], "item")?,
                data_a: args.get(3).map(|s| num(s, "A")).transpose()?.unwrap_or(0),
                data_b: args.get(4).map(|s| num(s, "B")).transpose()?.unwrap_or(0),
                z_index: args.get(5).map(|s| num(s, "Z")).transpose()?.unwrap_or(0),
                ..Default::default()
            })
        }
        "remove" => {
            need(2)?;
            Step::Remove(RemoveNetRequest {
                pos_x: num::<i64>(args[0], "x")? as u32,
                pos_y: num::<i64>(args[1], "y")? as u32,
                z_index: args.get(2).map(|s| num(s, "Z")).transpose()?.unwrap_or(0),
                ..Default::default()
            })
        }
        "paint" => {
            need(4)?;
            Step::Paint(PaintNetRequest {
                pos_x: num::<i64>(args[0], "x")? as u32,
                pos_y: num::<i64>(args[1], "y")? as u32,
                color_index: num(args[2], "color")?,
                face_index: num(args[3], "face")?,
                ..Default::default()
            })
        }
        "workbench" => {
            need(3)?;
            Step::PlaceWorkbench {
                r: PlaceWorkbenchNetRequest {
                    pos_x: num::<i64>(args[0], "x")? as u32,
                    pos_y: num::<i64>(args[1], "y")? as u32,
                    workbench_type: num(args[2], "type")?,
                    ..Default::default()
                },
                tail: None,
            }
        }
        "hb-old" => Step::HeartbeatOld,
        "hb-rules" => match args.first() {
            None => Step::HeartbeatRules(None),
            Some(h) => {
                let b = from_hex(h).ok_or_else(|| format!("bad hex '{}'", h))?;
                let arr: [u8; 64] = b.try_into().map_err(|_| "hb-rules needs exactly 64 bytes".to_string())?;
                Step::HeartbeatRules(Some(arr))
            }
        },
        "chat" => Step::Chat(rest.to_string()),
        "cmd" => {
            if rest.is_empty() {
                return Err("'cmd' needs a command line".into());
            }
            Step::Command(rest.to_string())
        }
        "zoom" => {
            need(1)?;
            Step::Zoom {
                player_id: args[0].to_string(),
                cycle: args.get(1).map(|s| num(s, "cycle")).transpose()?.unwrap_or(0),
            }
        }
        "disconnect" => Step::Disconnect,
        other => return Err(format!("unknown command '{}'", other)),
    })
}

// ----------------------------------------------------------------------------------------------
// built-in scenarios (6.5)
// ----------------------------------------------------------------------------------------------

/// Names of the built-in scenarios.
pub const BUILTIN_NAMES: &[&str] = &[
    "solo_join",
    "two_client_join",
    "block_requests",
    "fill_remove",
    "chat_relay",
    "rejections",
    "heartbeat",
    "blockhead_create",
    "malformed",
];

/// Script text of a built-in scenario (rows of and test).
pub fn builtin_script(name: &str) -> Option<&'static str> {
    Some(match name {
        "solo_join" => "client TCA\ninitial-data\nwait 06\nwait 1e 6000\nuid\nhb-old\nwait 17 6000\ndisconnect\n",
        "two_client_join" => "client A\nclient B\nwait 1e 6000\nquiet 1500\nuse A\nwait 1e 6000\nquiet 500\nuse B\ndisconnect\nuse A\ndisconnect\n",
        // Every index must be < W*32 of the sample world (16384 on w1). Indices >= W*32 are UB in the
        // reference (crash or 0x27) and are ours-only checks, never used in reference parity runs
        // .
        "block_requests" => {
            "client TCA\nblock 8646\nwait 04\nsleep 700\nblock 1 create=0\nwait 27\nblock 8646\nblock 8646\nquiet 1500\nblock 12345 create=1\nwait 04\nwait 04 2000\ndisconnect\n"
        }
        "fill_remove" => "client TCA\nblock-tile 6837 545\nwait 04\nfill 6837 545 0x0b\nwait 04\nremove 6837 545\nwait 04\ndisconnect\n",
        "chat_relay" => "client A\nclient B\nuse A\nchat hi\nuse B\nwait 24\nuse A\nexpect-none 24 1500\ndisconnect\nuse B\ndisconnect\n",
        "rejections" => {
            "client A\nclient ABCDEFGHIJKLMNOPQ\nexpect-reject 14\nclient Server\nexpect-reject 2\nuse A\ndisconnect\n"
        }
        "heartbeat" => "client TCA\nhb-old\nwait 17 6000\nhb-rules\nwait 17 6000\ndisconnect\n",
        "blockhead_create" => {
            "client TCA\nblock-tile 14557 517\nwait 04\nsleep 500\nuid\nblockhead Bob 14557 518\nwait 07\nwait 08\nuid\ndisconnect\n"
        }
        "malformed" => "client TCA\nsend 24\nsleep 1500\nuid\n",
        _ => return None,
    })
}

/// A built-in scenario by name.
pub fn builtin(name: &str) -> Option<Scenario> {
    let mut sc = Scenario::parse(builtin_script(name)?).ok()?;
    sc.name = name.to_string();
    Some(sc)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// r2_session_1: block_requests probes an in-range never-generated block (index 1), and all
    /// indices stay below W*32 = 16384 (w1).
    #[test]
    fn block_requests_indices_in_range() {
        let t = builtin_script("block_requests").unwrap();
        assert!(t.contains("block 1 create=0\nwait 27"));
        assert!(!t.contains("100000"));
        for l in t.lines().filter(|l| l.starts_with("block ")) {
            let i: u64 = l.split_whitespace().nth(1).unwrap().parse().unwrap();
            assert!(i < 16384, "{l}");
        }
        assert!(Scenario::parse(t).is_ok());
    }

    /// U8: two_clients/scriptA.txt parses, errors carry line numbers, builder == parser.
    #[test]
    fn u8_script_a() {
        let p = crate::harness::private_docs().join("protocol_vectors/two_clients/scriptA.txt");
        if let Ok(text) = std::fs::read_to_string(&p) {
            let sc = Scenario::parse(&text).unwrap();
            assert!(sc.steps.len() > 10);
            assert!(matches!(sc.steps[0], Step::Raw(_)));
            assert!(sc.steps.iter().any(|s| matches!(s, Step::Xml { ty: 0x24, .. })));
        }
    }

    #[test]
    fn u8_errors() {
        match Scenario::parse("# c\nsleep 1\nbogus 1\n") {
            Err(ScenarioError::Parse { line, .. }) => assert_eq!(line, 3),
            other => panic!("{:?}", other.map(|s| s.steps.len())),
        }
        match Scenario::parse("send zz\n") {
            Err(ScenarioError::Parse { line, msg }) => assert_eq!((line, msg.contains("bad hex")), (1, true)),
            _ => panic!(),
        }
        assert!(Scenario::parse("wait\n").is_err());
    }

    #[test]
    fn u8_builder_equals_parser() {
        let text = "client TCA\ninitial-data\nblock 8646\nblocks 1,2,3 create=0\nuid\nblockhead Bob 14557 518\nfill 6837 545 0x0b\nremove 6837 545\nhb-old\nchat hello world\ncmd /help\nsend 0e\nwait 04\nwait 07 1000\nquiet 300\nsleep 10\nexpect 04\nexpect-none 27 100\nexpect-reject 14\ndisconnect\n";
        let parsed = Scenario::parse(text).unwrap();
        let built = Scenario::new("script")
            .connect("TCA")
            .initial_data()
            .request_blocks(&[8646], true)
            .request_blocks(&[1, 2, 3], false)
            .unique_id()
            .create_blockhead(BlockheadSpec::new("Bob", 14557, 518))
            .fill(FillNetRequest { pos_x: 6837, pos_y: 545, type_: 0x0b, ..Default::default() })
            .remove(RemoveNetRequest { pos_x: 6837, pos_y: 545, ..Default::default() })
            .heartbeat_request()
            .chat("hello world")
            .command("/help")
            .raw("0e")
            .wait_for(4)
            .wait_for_ms(7, 1000)
            .wait_quiet(300)
            .sleep(10)
            .expect(4)
            .expect_none(0x27, 100)
            .expect_rejection(14)
            .disconnect();
        assert_eq!(format!("{:?}", parsed.steps), format!("{:?}", built.steps));
    }

    #[test]
    fn builtins_parse() {
        for n in BUILTIN_NAMES {
            assert!(builtin(n).is_some(), "{}", n);
        }
    }

    #[test]
    fn inventory_json() {
        let sc = Scenario::parse("inventory @LAST_BLOCKHEAD [[],[\"AAEC\"],{\"a\":1},[],[],[],[],[]]\n").unwrap();
        match &sc.steps[0] {
            Step::UploadInventory { blockhead: BlockheadRef::Last, slots: PV::Array(a) } => {
                assert_eq!(a.items.len(), 8);
                match &a.items[1] {
                    PV::Array(x) => assert!(matches!(&x.items[0], PV::Data(d) if d.bytes == vec![0, 1, 2])),
                    _ => panic!(),
                }
            }
            _ => panic!(),
        }
    }
}
