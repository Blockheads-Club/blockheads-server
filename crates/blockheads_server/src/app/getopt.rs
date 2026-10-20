// SPDX-License-Identifier: GPL-3.0-or-later AND LGPL-2.1-or-later
// Contains derived work; see THIRD_PARTY_NOTICES.md.
//! Byte-exact emulation of glibc 2.19 `getopt_long_only` (PERMUTE ordering, `opterr = 1`).
//!

/// One entry of the `longopts` table.
#[derive(Clone, Copy, Debug)]
pub struct LongOption {
    pub name: &'static str,
    pub has_arg: bool,
    pub val: u8,
}

const fn lo(name: &'static str, has_arg: bool, val: u8) -> LongOption {
    LongOption { name, has_arg, val }
}

/// The long option table; the order is part of the contract (ambiguity lists).
pub static LONGOPTS: [LongOption; 21] = [
    lo("list", false, b'l'),
    lo("help", false, b'h'),
    lo("new", true, b'n'),
    lo("load", true, b'o'),
    lo("port", true, b'p'),
    lo("max_players", true, b'm'),
    lo("rename-from", true, b'r'),
    lo("rename-to", true, b't'),
    lo("delete", true, b'd'),
    lo("force", false, b'f'),
    lo("save_delay", true, b's'),
    lo("world_width", true, b'w'),
    lo("world_id", true, b'i'),
    lo("credit", true, b'c'),
    lo("cloud_salt", true, b'k'),
    lo("owner_name", true, b'u'),
    lo("privacy", true, b'P'),
    lo("custom-rules", true, b'C'),
    lo("convert-to-custum-rules", false, b'V'),
    lo("expert-mode", false, b'e'),
    lo("no-exit", false, b'N'),
];

/// The short option string handed to `getopt_long_only`.
pub const OPTSTRING: &str = "n:o:p:m:r:d:s:t:w:i:u:k:c:P:C:lhfVeN";

/// `strchr(optstring, c) != NULL` (never true for NUL).
fn in_optstring(c: u8) -> bool {
    c != 0 && OPTSTRING.as_bytes().contains(&c)
}

/// glibc getopt state (`optind`, `optarg`, `optopt`, `nextchar`, `first_nonopt`, `last_nonopt`) plus `argv`.
pub struct Getopt {
    argv: Vec<Vec<u8>>,
    optind: usize,
    optarg: Option<Vec<u8>>,
    optopt: i32,
    /// `None` is the C NULL; `Some(empty)` is `""`.
    nextchar: Option<Vec<u8>>,
    first_nonopt: usize,
    last_nonopt: usize,
}

/// Concatenate byte pieces into one message.
fn msg(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

impl Getopt {
    /// `argv[0]` included; `optind = 1`, `optopt = 0`.
    pub fn new(argv: Vec<Vec<u8>>) -> Getopt {
        Getopt { argv, optind: 1, optarg: None, optopt: 0, nextchar: None, first_nonopt: 1, last_nonopt: 1 }
    }

    /// Carry a sticky `optopt` over (the C global survives across calls of a test driver).
    pub fn set_optopt(&mut self, v: i32) {
        self.optopt = v;
    }

    /// `optarg` after the last call.
    pub fn optarg(&self) -> Option<&[u8]> {
        self.optarg.as_deref()
    }

    /// `optopt`.
    pub fn optopt(&self) -> i32 {
        self.optopt
    }

    /// `optind`.
    pub fn optind(&self) -> usize {
        self.optind
    }

    /// The (permuted) argument vector.
    pub fn argv(&self) -> &[Vec<u8>] {
        &self.argv
    }

    /// glibc `exchange()`: rotate the non-option block and the option block.
    fn exchange(&mut self) {
        let bottom = self.first_nonopt;
        let middle = self.last_nonopt;
        let top = self.optind;
        self.argv[bottom..top].rotate_left(middle - bottom);
        self.first_nonopt += self.optind - self.last_nonopt;
        self.last_nonopt = self.optind;
    }

    /// `getopt_long_only`: -1 at the end, else the option value (`b'?'` for errors). Diagnostics go to `err` as one
    /// message without the trailing newline.
    pub fn next(&mut self, err: &mut dyn FnMut(&[u8])) -> i32 {
        let argc = self.argv.len();
        self.optarg = None;
        if self.nextchar.as_ref().map_or(true, |n| n.is_empty()) {
            if self.last_nonopt > self.optind {
                self.last_nonopt = self.optind;
            }
            if self.first_nonopt > self.optind {
                self.first_nonopt = self.optind;
            }
            if self.first_nonopt != self.last_nonopt && self.last_nonopt != self.optind {
                self.exchange();
            } else if self.last_nonopt != self.optind {
                self.first_nonopt = self.optind;
            }
            while self.optind < argc
                && (self.argv[self.optind].first() != Some(&b'-') || self.argv[self.optind].len() == 1)
            {
                self.optind += 1;
            }
            self.last_nonopt = self.optind;
            if self.optind != argc && self.argv[self.optind] == b"--" {
                self.optind += 1;
                if self.first_nonopt != self.last_nonopt && self.last_nonopt != self.optind {
                    self.exchange();
                } else if self.first_nonopt == self.last_nonopt {
                    self.first_nonopt = self.optind;
                }
                self.last_nonopt = argc;
                self.optind = argc;
            }
            if self.optind == argc {
                if self.first_nonopt != self.last_nonopt {
                    self.optind = self.first_nonopt;
                }
                return -1;
            }
            let cur = &self.argv[self.optind];
            let skip = 1 + usize::from(cur.get(1) == Some(&b'-'));
            self.nextchar = Some(cur[skip.min(cur.len())..].to_vec());
        }
        let a0 = self.argv[0].clone();
        let cur = self.argv[self.optind].clone();
        let c1 = cur.get(1).copied();
        // The long-option test runs on every call (also for the rest of a cluster).
        if c1 == Some(b'-') || (cur.len() > 2 || !c1.map_or(false, in_optstring)) {
            let nc = self.nextchar.clone().unwrap_or_default();
            let eq = nc.iter().position(|&b| b == b'=');
            let namelen = eq.unwrap_or(nc.len());
            let mut pfound: Option<usize> = None;
            let mut ambig: Vec<usize> = Vec::new();
            let mut exact = false;
            for (i, o) in LONGOPTS.iter().enumerate() {
                let nm = o.name.as_bytes();
                if nm.len() >= namelen && nm[..namelen] == nc[..namelen] {
                    if nm.len() == namelen {
                        pfound = Some(i);
                        exact = true;
                        break;
                    } else if pfound.is_none() {
                        pfound = Some(i);
                    } else {
                        ambig.insert(0, i);
                    }
                }
            }
            if !ambig.is_empty() && !exact {
                let mut m = msg(&[&a0, b": option '", &cur, b"' is ambiguous; possibilities:"]);
                let mut all = vec![pfound.unwrap()];
                all.extend(ambig);
                for i in all {
                    m.extend_from_slice(b" '--");
                    m.extend_from_slice(LONGOPTS[i].name.as_bytes());
                    m.push(b'\'');
                }
                err(&m);
                self.nextchar = Some(Vec::new());
                self.optind += 1;
                self.optopt = 0;
                return b'?' as i32;
            }
            if let Some(pi) = pfound {
                let o = LONGOPTS[pi];
                self.optind += 1;
                if let Some(e) = eq {
                    if o.has_arg {
                        self.optarg = Some(nc[e + 1..].to_vec());
                    } else {
                        let prev = &self.argv[self.optind - 1];
                        let m = if prev.get(1) == Some(&b'-') {
                            msg(&[&a0, b": option '--", o.name.as_bytes(), b"' doesn't allow an argument"])
                        } else {
                            msg(&[&a0, b": option '", &prev[..1], o.name.as_bytes(), b"' doesn't allow an argument"])
                        };
                        err(&m);
                        self.nextchar = Some(Vec::new());
                        self.optopt = o.val as i32;
                        return b'?' as i32;
                    }
                } else if o.has_arg {
                    if self.optind < argc {
                        self.optarg = Some(self.argv[self.optind].clone());
                        self.optind += 1;
                    } else {
                        err(&msg(&[&a0, b": option '--", o.name.as_bytes(), b"' requires an argument"]));
                        self.nextchar = Some(Vec::new());
                        self.optopt = o.val as i32;
                        return b'?' as i32;
                    }
                }
                self.nextchar = Some(Vec::new());
                return o.val as i32;
            }
            // no long match
            let first = nc.first().copied();
            if c1 == Some(b'-') || !first.map_or(false, in_optstring) {
                let m = if c1 == Some(b'-') {
                    msg(&[&a0, b": unrecognized option '--", &nc, b"'"])
                } else {
                    msg(&[&a0, b": unrecognized option '", &cur[..1], &nc, b"'"])
                };
                err(&m);
                self.nextchar = Some(Vec::new());
                self.optind += 1;
                self.optopt = 0;
                return b'?' as i32;
            }
        }
        // short option
        let nc = self.nextchar.take().unwrap_or_default();
        let c = nc[0];
        let rest = nc[1..].to_vec();
        self.nextchar = Some(rest.clone());
        if rest.is_empty() {
            self.optind += 1;
        }
        let idx = OPTSTRING.as_bytes().iter().position(|&x| x == c);
        let i = match idx {
            Some(i) if c != b':' => i,
            _ => {
                err(&msg(&[&a0, b": invalid option -- '", &[c], b"'"]));
                self.optopt = c as i32;
                return b'?' as i32;
            }
        };
        if OPTSTRING.as_bytes().get(i + 1) == Some(&b':') {
            if !rest.is_empty() {
                self.optarg = Some(rest);
                self.optind += 1;
            } else if self.optind == argc {
                err(&msg(&[&a0, b": option requires an argument -- '", &[c], b"'"]));
                self.optopt = c as i32;
                self.nextchar = None;
                return b'?' as i32;
            } else {
                self.optarg = Some(self.argv[self.optind].clone());
                self.optind += 1;
            }
            self.nextchar = None;
        }
        c as i32
    }
}

/// glibc's `fprintf(stderr, "%s\n")`: one `write(2,...)` with the message and a newline.
pub fn stderr_sink(m: &[u8]) {
    let mut v = Vec::with_capacity(m.len() + 1);
    v.extend_from_slice(m);
    v.push(b'\n');
    crate::foundation::sys::write_stderr(&v);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> String {
        crate::testenv::modules("app_cli_getopt")
    }

    /// Same output format as ``.
    fn run(corpus: &[u8]) -> String {
        let mut out: Vec<String> = Vec::new();
        let mut optopt = 0;
        let text = corpus;
        let mut lines: Vec<&[u8]> = text.split(|&b| b == b'\n').collect();
        lines.pop();
        let lossy = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
        for (n, line) in lines.iter().enumerate() {
            let args: Vec<Vec<u8>> =
                if line.is_empty() { vec![] } else { line.split(|&b| b == b'\t').map(|s| s.to_vec()).collect() };
            let mut argv = vec![b"/opt/blockheads/blockheads_server171".to_vec()];
            argv.extend(args.iter().cloned());
            let mut hdr = format!("### {}:", n);
            for a in &args {
                hdr.push_str(&format!(" [{}]", lossy(a)));
            }
            out.push(hdr);
            let mut g = Getopt::new(argv);
            g.set_optopt(optopt);
            loop {
                let mut diag: Vec<String> = Vec::new();
                let o = g.next(&mut |m| diag.push(lossy(m)));
                out.extend(diag);
                if o == -1 {
                    break;
                }
                let ch = if o > 32 && o < 127 { (o as u8 as char).to_string() } else { ".".to_string() };
                let oa = g.optarg().map(lossy).unwrap_or_else(|| "(null)".to_string());
                out.push(format!("opt={}('{}') optarg={} optopt={} optind={}", o, ch, oa, g.optopt(), g.optind()));
                if o == b'h' as i32 || o == b'?' as i32 {
                    break;
                }
            }
            optopt = g.optopt();
            let mut e = format!("end optind={} rest:", g.optind());
            for a in &g.argv()[1..] {
                e.push_str(&format!(" [{}]", lossy(a)));
            }
            out.push(e);
        }
        out.join("\n")
    }

    fn check(tsv: &str, expected: &str) {
        let (Ok(corpus), Ok(exp)) =
            (std::fs::read(format!("{}/{tsv}", dir())), std::fs::read_to_string(format!("{}/{expected}", dir())))
        else {
            eprintln!("SKIP: private getopt corpus unavailable (set BH_PRIVATE_DOCS)");
            return;
        };
        let got = run(&corpus);
        let (g, e): (Vec<&str>, Vec<&str>) =
            (got.split('\n').collect(), exp.trim_end_matches('\n').split('\n').collect());
        for (i, (a, b)) in g.iter().zip(e.iter()).enumerate() {
            assert_eq!(a, b, "{tsv} line {i}");
        }
        assert_eq!(g.len(), e.len(), "{tsv} line count");
    }

    /// 220 hand vectors against the real libc.
    #[test]
    fn corpus() {
        check("corpus.tsv", "corpus_expected.txt");
    }

    /// 3000 fuzz vectors against the real libc.
    #[test]
    fn fuzz() {
        check("fuzz.tsv", "fuzz_expected.txt");
    }
}
