//! `bh_client`: CLI of the protocol test client (hand-written argument parsing).
//!
//!  Exit codes: 0 ok, 1 scenario failure, 2 usage, 3 connect failure.

use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::process::exit;

use bh_testclient::decode::{decode_packet, pretty};
use bh_testclient::scenario::{builtin, BUILTIN_NAMES};
use bh_testclient::trace::{read_trace, Dir, TraceRecord, TraceSink};
use bh_testclient::{
    ClientConfig, Identity, JoinOutcome, LoaderOptions, ModClient, RunOptions, Scenario, ScenarioError, Session,
};

const USAGE: &str = "usage:
  bh_client join ADDR --alias A [--udid U] [--password P] [--secs N] [--trace FILE] [--modded CACHE_DIR]
  bh_client run  ADDR --script FILE [--trace-dir DIR] [--timeout MS]
  bh_client probe ADDR [--secs N]
  bh_client decode FILE.jsonl [--json] [--type 0x04]
  bh_client list
  bh_client builtin NAME ADDR [--trace-dir DIR]";

fn usage() -> ! {
    eprintln!("{}", USAGE);
    exit(2)
}

struct Args {
    pos: Vec<String>,
    opts: Vec<(String, Option<String>)>,
}

impl Args {
    fn parse(a: &[String], flags: &[&str]) -> Args {
        let mut pos = Vec::new();
        let mut opts = Vec::new();
        let mut i = 0;
        while i < a.len() {
            if let Some(k) = a[i].strip_prefix("--") {
                if flags.contains(&k) {
                    opts.push((k.to_string(), None));
                } else {
                    i += 1;
                    if i >= a.len() {
                        eprintln!("option --{} needs a value", k);
                        usage();
                    }
                    opts.push((k.to_string(), Some(a[i].clone())));
                }
            } else {
                pos.push(a[i].clone());
            }
            i += 1;
        }
        Args { pos, opts }
    }
    fn get(&self, k: &str) -> Option<&str> {
        self.opts.iter().find(|(kk, _)| kk == k).and_then(|(_, v)| v.as_deref())
    }
    fn has(&self, k: &str) -> bool {
        self.opts.iter().any(|(kk, _)| kk == k)
    }
}

fn resolve(s: &str) -> SocketAddr {
    match s.to_socket_addrs().ok().and_then(|mut i| i.next()) {
        Some(a) => a,
        None => {
            eprintln!("bad address '{}' (expected host:port)", s);
            exit(2)
        }
    }
}

fn cfg_for(addr: &str) -> ClientConfig {
    ClientConfig { addr: resolve(addr), ..Default::default() }
}

fn print_new(sink: &TraceSink, printed: &mut usize) {
    let recs = sink.records();
    for r in &recs[*printed..] {
        println!("{}", pretty(r));
    }
    *printed = recs.len();
}

fn cmd_join(a: &Args) -> i32 {
    let (Some(addr), Some(alias)) = (a.pos.first(), a.get("alias")) else { usage() };
    let mut id = Identity::new(alias);
    if let Some(u) = a.get("udid") {
        id = id.udid(u);
    }
    if let Some(p) = a.get("password") {
        id = id.password(p);
    }
    let secs: u64 = a.get("secs").and_then(|s| s.parse().ok()).unwrap_or(10);
    let mut s = match Session::connect(cfg_for(addr), id) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("connect failed: {}", e);
            return 3;
        }
    };
    let mem = TraceSink::memory();
    s.set_trace(Some(mem.clone()));
    let file = a.get("trace").map(|p| {
        TraceSink::create(&PathBuf::from(p), "join").unwrap_or_else(|e| {
            eprintln!("cannot create trace: {}", e);
            exit(2)
        })
    });
    let mut printed = 0usize;
    let mut file_done = 0usize;
    let flush_file = |mem: &TraceSink, done: &mut usize| {
        if let Some(f) = &file {
            let recs = mem.records();
            for r in &recs[*done..] {
                f.record(r.dir, &r.client, r.t_ms, r.channel, r.reliable, &r.data);
            }
            *done = recs.len();
        }
    };
    // Modded mode: the 0xBC mod protocol (HELLO, mod list, downloads into CACHE_DIR, READY, bhcModLoader).
    let mut modded = a.get("modded").map(|d| ModClient::new(LoaderOptions::new(std::path::Path::new(d))));
    let joined = match &mut modded {
        Some(mc) => mc.join(&mut s, 5000),
        None => s.join(5000),
    };
    if let Some(mc) = &modded {
        if let Some(h) = &mc.server {
            let hash = bhmod_proto::hex(&h.mods_hash);
            eprintln!("mods: server hello modsHash={hash} clientRequired={} count={}", h.client_required, h.count);
        }
        for m in mc.list.iter().flat_map(|l| &l.mods) {
            eprintln!("mods: {} {} sides={:?} required={}", m.id, m.version, m.sides, m.client_required);
        }
        for d in &mc.downloads {
            eprintln!("mods: download {} from {} complete={}", bhmod_proto::hex(&d.sha256), d.resumed_from, d.complete);
        }
        if let Some(r) = &mc.reject {
            eprintln!("mods: rejected: {}", r.text);
        }
    }
    let outcome = match joined {
        Ok(o) => o,
        Err(e) => {
            eprintln!("io error: {}", e);
            return 1;
        }
    };
    print_new(&mem, &mut printed);
    eprintln!("join: {:?}", outcome);
    if let Some(f) = &file {
        f.mark("join", &format!("{:?}", outcome));
    }
    if matches!(outcome, JoinOutcome::NoContact | JoinOutcome::Disconnected) {
        flush_file(&mem, &mut file_done);
        return 1;
    }
    if outcome == JoinOutcome::Approved {
        let _ = s.request_initial_data();
    }
    let end = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    while std::time::Instant::now() < end && !s.state.disconnected {
        if s.run_for(100).is_err() {
            break;
        }
        print_new(&mem, &mut printed);
    }
    print_new(&mem, &mut printed);
    flush_file(&mem, &mut file_done);
    for n in &s.state.notes {
        eprintln!("note: {}", n);
    }
    s.disconnect();
    0
}

fn report_error(e: &ScenarioError) -> i32 {
    eprintln!("{}", e);
    if e.is_connect_failure() {
        3
    } else {
        1
    }
}

fn run_scenario(sc: &Scenario, addr: &str, trace_dir: Option<&str>, timeout: Option<u64>) -> i32 {
    let opts = RunOptions {
        trace_dir: trace_dir.map(PathBuf::from),
        default_timeout_ms: timeout.unwrap_or(5000),
        ..Default::default()
    };
    match sc.run(&cfg_for(addr), &opts) {
        Ok(rep) => {
            for n in &rep.notes {
                eprintln!("note: {}", n);
            }
            for (alias, ty, c) in &rep.received_counts {
                println!("{} rx 0x{:02x} {} x{}", alias, ty, bh_testclient::type_name(*ty), c);
            }
            0
        }
        Err(e) => report_error(&e),
    }
}

fn cmd_run(a: &Args) -> i32 {
    let (Some(addr), Some(script)) = (a.pos.first(), a.get("script")) else { usage() };
    let text = match std::fs::read_to_string(script) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cannot read {}: {}", script, e);
            return 2;
        }
    };
    let mut sc = match Scenario::parse(&text) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{}", e);
            return 2;
        }
    };
    sc.name = std::path::Path::new(script)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "script".into());
    run_scenario(&sc, addr, a.get("trace-dir"), a.get("timeout").and_then(|s| s.parse().ok()))
}

fn cmd_probe(a: &Args) -> i32 {
    let Some(addr) = a.pos.first() else { usage() };
    let secs: u64 = a.get("secs").and_then(|s| s.parse().ok()).unwrap_or(3);
    let mut s = match Session::connect(cfg_for(addr), Identity::new("probe")) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("connect failed: {}", e);
            return 3;
        }
    };
    let mem = TraceSink::memory();
    s.set_trace(Some(mem.clone()));
    let end = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    let mut printed = 0;
    while std::time::Instant::now() < end {
        if s.run_for(50).is_err() {
            break;
        }
        print_new(&mem, &mut printed);
    }
    s.disconnect();
    0
}

fn cmd_decode(a: &Args) -> i32 {
    let Some(file) = a.pos.first() else { usage() };
    let filter = a.get("type").map(|t| u8::from_str_radix(t.trim_start_matches("0x"), 16).unwrap_or_else(|_| usage()));
    let recs = match read_trace(&PathBuf::from(file)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("cannot read {}: {}", file, e);
            return 2;
        }
    };
    for r in recs.iter().filter(|r| filter.map_or(true, |t| r.ty == t)) {
        if a.has("json") {
            let d = decode_packet(r.dir, &r.data);
            println!(
                "{{\"seq\":{},\"t\":{},\"dir\":\"{}\",\"client\":{},\"type\":{},\"name\":\"{}\",\"len\":{},\"decoded\":{}}}",
                r.seq,
                r.t_ms,
                r.dir.as_str(),
                bh_testclient::trace::json_str(&r.client),
                r.ty,
                d.name,
                r.len,
                d.json
            );
        } else {
            println!("{}", pretty(r));
        }
    }
    0
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = argv.first() else { usage() };
    let rest = &argv[1..];
    let code = match cmd.as_str() {
        "join" => cmd_join(&Args::parse(rest, &[])),
        "run" => cmd_run(&Args::parse(rest, &[])),
        "probe" => cmd_probe(&Args::parse(rest, &[])),
        "decode" => cmd_decode(&Args::parse(rest, &["json"])),
        "list" => {
            for n in BUILTIN_NAMES {
                println!("{}", n);
            }
            0
        }
        "builtin" => {
            let a = Args::parse(rest, &[]);
            let (Some(name), Some(addr)) = (a.pos.first(), a.pos.get(1)) else { usage() };
            match builtin(name) {
                Some(sc) => run_scenario(&sc, addr, a.get("trace-dir"), None),
                None => {
                    eprintln!("unknown builtin '{}' (see `bh_client list`)", name);
                    2
                }
            }
        }
        "-h" | "--help" | "help" => {
            println!("{}", USAGE);
            0
        }
        _ => usage(),
    };
    exit(code);
}

#[allow(dead_code)]
fn _unused(_: &TraceRecord, _: Dir) {}
