//! `bh_tracediff`: compare two packet traces (reference vs ours) after normalisation.
//!
//!  Exit codes: 0 clean, 1 differences, 2 usage/IO error.

use std::path::{Path, PathBuf};
use std::process::exit;

use bh_testclient::harness::normalize::*;
use bh_testclient::harness::trace_diff::*;

const USAGE: &str = "usage: bh_tracediff REF.jsonl OURS.jsonl [--ref2 REF2.jsonl] [--per-type] [--keep-tx] [--meta PATH] [--mask-type 0xTT:LO-HI]... [--json]";

fn take(args: &[String], i: &mut usize) -> String {
    *i += 1;
    args.get(*i).cloned().unwrap_or_else(|| die(USAGE))
}

fn die(msg: &str) -> ! {
    eprintln!("{}", msg);
    exit(2)
}

fn parse_mask(s: &str) -> Option<(u8, std::ops::Range<usize>)> {
    let (t, r) = s.split_once(':')?;
    let ty = u8::from_str_radix(t.trim_start_matches("0x"), 16).ok()?;
    let (lo, hi) = r.split_once('-')?;
    Some((ty, lo.parse().ok()?..hi.parse().ok()?))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut pos: Vec<PathBuf> = vec![];
    let (mut ref2, mut meta) = (None::<PathBuf>, None::<PathBuf>);
    let (mut per_type, mut keep_tx, mut json) = (false, false, false);
    let mut masks = vec![];
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--ref2" => ref2 = Some(PathBuf::from(take(&args, &mut i))),
            "--meta" => meta = Some(PathBuf::from(take(&args, &mut i))),
            "--per-type" => per_type = true,
            "--keep-tx" => keep_tx = true,
            "--json" => json = true,
            "--mask-type" => masks.push(parse_mask(&take(&args, &mut i)).unwrap_or_else(|| die(USAGE))),
            s if s.starts_with("--") => die(USAGE),
            s => pos.push(PathBuf::from(s)),
        }
        i += 1;
    }
    if pos.len() != 2 {
        die(USAGE);
    }
    let rd = |p: &Path| read_jsonl(p).unwrap_or_else(|e| die(&format!("{}: {}", p.display(), e)));
    let mut o = NormalizeOptions::default();
    if let Some(m) = meta {
        o.padding = PaddingMeta::load(&m).unwrap_or_else(|e| die(&format!("{}", e)));
    }
    o.keep_tx = keep_tx;
    o.extra_masks = masks;
    let (r, u) = (rd(&pos[0]), rd(&pos[1]));
    if let Some(r2) = ref2 {
        let n1 = normalize_trace(&r, &o);
        let n2 = normalize_trace(&rd(&r2), &o);
        o.volatile = Some(VolatileMask::learn(&[n1, n2]));
    }
    let (na, nb) = (normalize_trace(&r, &o), normalize_trace(&u, &o));
    let rep = diff_traces(&na, &nb, if per_type { DiffMode::PerType } else { DiffMode::Strict });
    if json {
        println!("{}", rep.render_json());
    } else {
        print!("{}", rep.render());
    }
    exit(if rep.is_clean() { 0 } else { 1 });
}
