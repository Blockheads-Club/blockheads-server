//! `bh_savediff`: record-level diff of two Blockheads saves (+ summary/block/regzip/dump helpers).
//!
//!  Exit codes: 0 clean, 1 differences, 2 usage/IO error.

use std::path::{Path, PathBuf};
use std::process::exit;

use bh_testclient::harness::save_diff::*;

const USAGE: &str = "usage:
  bh_savediff A B [--mask SPEC]... [--no-default-masks] [--no-strict-gzip] [--json] [--max N]
  bh_savediff summary SAVE | block SAVE X Y [FIELD] | regzip SAVE | dump SAVE OUT
mask SPEC: dates | plist:<record glob>#<path glob> | record:<glob> | tile:13,23,38";

fn die(msg: &str) -> ! {
    eprintln!("{}", msg);
    exit(2)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        die(USAGE);
    }
    let r = match args[0].as_str() {
        "summary" if args.len() == 2 => summary(Path::new(&args[1])).map(|s| print!("{}", s)).map(|_| 0),
        "block" if args.len() == 4 || args.len() == 5 => {
            let (x, y): (u32, u32) = match (args[2].parse(), args[3].parse()) {
                (Ok(x), Ok(y)) => (x, y),
                _ => die(USAGE),
            };
            render_block_grid(Path::new(&args[1]), x, y, args.get(4).map(|s| s.as_str()).unwrap_or("typeIndex"))
                .map(|s| print!("{}", s))
                .map(|_| 0)
        }
        "regzip" if args.len() == 2 => regzip_check(Path::new(&args[1])).map(|(ok, bad)| {
            for b in &bad {
                println!("mismatch {}", b);
            }
            println!("regzip identical: {}, mismatched: {}", ok, bad.len());
            if bad.is_empty() && ok > 0 {
                0
            } else {
                1
            }
        }),
        "dump" if args.len() == 3 => dump(Path::new(&args[1]), Path::new(&args[2])).map(|_| {
            println!("dumped to {}", args[2]);
            0
        }),
        _ => diff_cmd(&args),
    };
    match r {
        Ok(code) => exit(code),
        Err(e) => {
            eprintln!("error: {}", e);
            exit(2)
        }
    }
}

fn diff_cmd(args: &[String]) -> Result<i32, bh_testclient::harness::HarnessError> {
    let mut pos: Vec<PathBuf> = vec![];
    let mut extra = vec![];
    let mut default_masks = true;
    let mut strict = true;
    let mut json = false;
    let mut max = 200usize;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--mask" => {
                i += 1;
                extra.push(Mask::parse(args.get(i).unwrap_or_else(|| die(USAGE)))?);
            }
            "--no-default-masks" => default_masks = false,
            "--no-strict-gzip" => strict = false,
            "--json" => json = true,
            "--max" => {
                i += 1;
                max = args.get(i).and_then(|s| s.parse().ok()).unwrap_or_else(|| die(USAGE));
            }
            s if s.starts_with("--") => die(USAGE),
            s => pos.push(PathBuf::from(s)),
        }
        i += 1;
    }
    if pos.len() != 2 {
        die(USAGE);
    }
    let mut o = if default_masks {
        SaveDiffOptions::parity_default()
    } else {
        SaveDiffOptions { masks: vec![], ..Default::default() }
    };
    o.masks.extend(extra);
    o.strict_gzip = strict;
    o.max_detail = max;
    let rep = diff_saves(&pos[0], &pos[1], &o)?;
    if json {
        println!("{}", rep.render_json());
    } else {
        print!("{}", rep.render(max));
        for (k, (a, b)) in &rep.per_db {
            println!("db {}: A={} B={}", k, a, b);
        }
    }
    Ok(if rep.is_clean() { 0 } else { 1 })
}
