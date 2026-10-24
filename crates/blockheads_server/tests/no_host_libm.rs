//! Checklist item 5: the crate must never call the host libm for functions the
//! original takes from glibc 2.19. Rust's `f32::sin`, `f64::powf`,... resolve to the host libm (whose
//! result differs from 2.19 in up to 15% of inputs,), so this scans the sources
//! under `src/` for the forbidden method calls. Everything goes through `fmath::libm`.

use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN: &[&str] = &[
    ".sin(",
    ".cos(",
    ".tan(",
    ".asin(",
    ".acos(",
    ".atan(",
    ".atan2(",
    ".sinh(",
    ".cosh(",
    ".tanh(",
    ".powf(",
    ".powi(",
    ".exp(",
    ".exp2(",
    ".exp_m1(",
    ".ln(",
    ".ln_1p(",
    ".log(",
    ".log2(",
    ".log10(",
    ".sin_cos(",
    ".cbrt(",
    ".hypot(",
    ".mul_add(",
    ".rem_euclid_f",
    ".to_degrees(",
    ".to_radians(",
];

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Strip `//` comments (good enough for this scan: no `//` inside string literals in the checked patterns).
fn code_part(line: &str) -> &str {
    match line.find("//") {
        Some(i) => &line[..i],
        None => line,
    }
}

#[test]
fn no_host_libm_calls_in_src() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rs_files(&root, &mut files);
    assert!(!files.is_empty());
    let mut bad = Vec::new();
    for f in files {
        let text = fs::read_to_string(&f).unwrap();
        for (n, line) in text.lines().enumerate() {
            let code = code_part(line);
            for pat in FORBIDDEN {
                if code.contains(pat) {
                    bad.push(format!("{}:{}: {}", f.display(), n + 1, line.trim()));
                }
            }
        }
    }
    assert!(bad.is_empty(), "host libm calls found (use crate::fmath::libm):\n{}", bad.join("\n"));
}

/// `extern "C"` links to libm functions are equally forbidden.
#[test]
fn no_extern_libm_links() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rs_files(&root, &mut files);
    let names = ["sinf", "cosf", "tanf", "powf", "atan2f", "sin", "cos", "pow", "atan2"];
    for f in files {
        let text = fs::read_to_string(&f).unwrap();
        for line in text.lines() {
            let l = code_part(line).trim();
            if l.starts_with("pub fn ") || l.starts_with("fn ") {
                for n in names {
                    if l.contains(&format!("fn {n}(")) && l.ends_with(';') {
                        panic!("extern libm declaration in {}: {}", f.display(), l);
                    }
                }
            }
        }
    }
}
