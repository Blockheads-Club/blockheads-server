//! `bh_rngdiff`: RNG call trace of the reference (gdb) versus ours, and their diff.
//!
//!  Exit codes: 0 clean, 1 differences, 2 usage/IO error.

use std::path::{Path, PathBuf};
use std::process::exit;

use bh_testclient::harness::gdb_rng::*;

const USAGE: &str = "usage:
  bh_rngdiff gen-script [-o FILE]
  bh_rngdiff ref --limit N --out FILE -- SERVER_ARGS...
  bh_rngdiff ours --out FILE --limit N [--clock UNIX] -- SERVER_ARGS...
  bh_rngdiff diff REF OURS [--context 8] [--require-equal-len]
  bh_rngdiff run --limit N -- SERVER_ARGS...";

fn take(args: &[String], i: &mut usize) -> String {
    *i += 1;
    args.get(*i).cloned().unwrap_or_else(|| die(USAGE))
}

fn die(msg: &str) -> ! {
    eprintln!("{}", msg);
    exit(2)
}

struct Opts {
    limit: u64,
    out: Option<PathBuf>,
    clock: Option<u64>,
    pos: Vec<String>,
    server_args: Vec<String>,
    require_equal_len: bool,
    o: Option<PathBuf>,
}

fn parse(args: &[String]) -> Opts {
    let mut o = Opts {
        limit: 20000,
        out: None,
        clock: None,
        pos: vec![],
        server_args: vec![],
        require_equal_len: false,
        o: None,
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--" => {
                o.server_args = args[i + 1..].to_vec();
                break;
            }
            "--limit" => o.limit = take(args, &mut i).parse().unwrap_or_else(|_| die(USAGE)),
            "--out" => o.out = Some(PathBuf::from(take(args, &mut i))),
            "--clock" => o.clock = Some(take(args, &mut i).parse().unwrap_or_else(|_| die(USAGE))),
            "--context" => {
                let _ = take(args, &mut i);
            }
            "-o" => o.o = Some(PathBuf::from(take(args, &mut i))),
            "--require-equal-len" => o.require_equal_len = true,
            s if s.starts_with("--") => die(USAGE),
            s => o.pos.push(s.to_string()),
        }
        i += 1;
    }
    o
}

fn read(p: &str) -> Vec<RngEvent> {
    let t = std::fs::read_to_string(p).unwrap_or_else(|e| die(&format!("{}: {}", p, e)));
    parse_trace(&t).unwrap_or_else(|e| die(&format!("{}: {}", p, e)))
}

fn report(r: &[RngEvent], u: &[RngEvent], req: bool) -> ! {
    let d = diff_rng(r, u, req);
    print!("{}", d.render());
    exit(if d.is_clean() { 0 } else { 1 })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        die(USAGE);
    }
    let o = parse(&args[1..]);
    let fail = |e: bh_testclient::harness::HarnessError| -> ! { die(&format!("error: {}", e)) };
    match args[0].as_str() {
        "gen-script" => match o.o {
            Some(p) => std::fs::write(&p, gdb_script_py()).unwrap_or_else(|e| die(&e.to_string())),
            None => print!("{}", gdb_script_py()),
        },
        "ref" => {
            let out = o.out.unwrap_or_else(|| die(USAGE));
            let t = run_reference_trace(&o.server_args, o.limit, &out).unwrap_or_else(|e| fail(e));
            println!("wrote {} events to {}", t.len(), out.display());
        }
        "ours" => {
            let out = o.out.unwrap_or_else(|| die(USAGE));
            let t = run_our_trace(&o.server_args, o.clock, o.limit, &out).unwrap_or_else(|e| fail(e));
            println!("wrote {} events to {}", t.len(), out.display());
        }
        "diff" => {
            if o.pos.len() != 2 {
                die(USAGE);
            }
            report(&read(&o.pos[0]), &read(&o.pos[1]), o.require_equal_len)
        }
        "run" => {
            let tmp = bh_testclient::harness::TempDir::new("rngrun").unwrap_or_else(|e| fail(e));
            let (rp, up): (PathBuf, PathBuf) = (tmp.path().join("ref.log"), tmp.path().join("ours.log"));
            let r = run_reference_trace(&o.server_args, o.limit, &rp).unwrap_or_else(|e| fail(e));
            let seed = first_srand_seed(&r).map(|s| s as u64);
            let u = run_our_trace(&o.server_args, seed, o.limit, &up).unwrap_or_else(|e| fail(e));
            let _ = Path::new("");
            report(&r, &u, false)
        }
        _ => die(USAGE),
    }
}
