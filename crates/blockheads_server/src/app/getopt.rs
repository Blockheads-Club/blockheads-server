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

macro_rules! lo {
    ($name:expr, $has_arg:expr, $val:expr) => {
        LongOption { name: $name, has_arg: $has_arg, val: $val }
    };
}

/// The long option table; the order is part of the contract (ambiguity lists).
pub static LONGOPTS: [LongOption; 21] = [
    lo!("list", false, b'l'),
    lo!("help", false, b'h'),
    lo!("new", true, b'n'),
    lo!("load", true, b'o'),
    lo!("port", true, b'p'),
    lo!("max_players", true, b'm'),
    lo!("rename-from", true, b'r'),
    lo!("rename-to", true, b't'),
    lo!("delete", true, b'd'),
    lo!("force", false, b'f'),
    lo!("save_delay", true, b's'),
    lo!("world_width", true, b'w'),
    lo!("world_id", true, b'i'),
    lo!("credit", true, b'c'),
    lo!("cloud_salt", true, b'k'),
    lo!("owner_name", true, b'u'),
    lo!("privacy", true, b'P'),
    lo!("custom-rules", true, b'C'),
    lo!("convert-to-custum-rules", false, b'V'),
    lo!("expert-mode", false, b'e'),
    lo!("no-exit", false, b'N'),
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
            // glibc also clamps `first_nonopt` to `optind` here; that only matters when a caller rewinds `optind`
            // and there is no setter, so `first_nonopt <= optind` always holds.
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
#[path = "getopt_golden.rs"]
mod golden;

#[cfg(test)]
mod tests {
    use super::*;

    type Outcome = (Vec<(i32, Option<String>)>, Vec<String>, Vec<String>, usize);

    /// Run `getopt_long_only` to the end (or the first `?`/`h`): options with optarg, diagnostics, final argv, optind.
    fn collect(args: &[&str]) -> Outcome {
        let mut argv = vec![b"prog".to_vec()];
        argv.extend(args.iter().map(|a| a.as_bytes().to_vec()));
        let mut g = Getopt::new(argv);
        let (mut opts, mut errs) = (Vec::new(), Vec::new());
        loop {
            let mut diag = Vec::new();
            let o = g.next(&mut |m| diag.push(String::from_utf8_lossy(m).into_owned()));
            errs.extend(diag);
            if o == -1 {
                break;
            }
            opts.push((o, g.optarg().map(|a| String::from_utf8_lossy(a).into_owned())));
            if o == b'?' as i32 {
                break;
            }
        }
        let rest = g.argv()[1..].iter().map(|a| String::from_utf8_lossy(a).into_owned()).collect();
        (opts, errs, rest, g.optind())
    }

    fn o(c: u8, a: Option<&str>) -> (i32, Option<String>) {
        (c as i32, a.map(str::to_string))
    }

    #[test]
    fn long_options_exact_abbreviated_and_with_values() {
        let (opts, errs, rest, ind) =
            collect(&["--list", "-help", "--port=1234", "--new", "w1", "-load=x", "--max_p", "8"]);
        assert_eq!(
            opts,
            vec![
                o(b'l', None),
                o(b'h', None),
                o(b'p', Some("1234")),
                o(b'n', Some("w1")),
                o(b'o', Some("x")),
                o(b'm', Some("8"))
            ]
        );
        assert!(errs.is_empty());
        assert_eq!(rest.len(), 8);
        assert_eq!(ind, 9);
    }

    #[test]
    fn non_options_are_permuted_to_the_end() {
        let (opts, _, rest, ind) = collect(&["a", "-l", "b", "-", "-h", "c"]);
        assert_eq!(opts, vec![o(b'l', None), o(b'h', None)]);
        assert_eq!(rest, ["-l", "-h", "a", "b", "-", "c"]);
        assert_eq!(ind, 3);
    }

    #[test]
    fn double_dash_ends_the_options() {
        let (opts, _, rest, ind) = collect(&["-l", "--", "-h", "x"]);
        assert_eq!(opts, vec![o(b'l', None)]);
        assert_eq!(rest, ["-l", "--", "-h", "x"].map(String::from));
        assert_eq!(ind, 3);
        let (opts, _, rest, ind) = collect(&["x", "--", "-h"]);
        assert!(opts.is_empty());
        assert_eq!(rest, ["--", "x", "-h"].map(String::from));
        assert_eq!(ind, 2);
        let (opts, _, _, ind) = collect(&["--"]);
        assert!(opts.is_empty());
        assert_eq!(ind, 2);
        let (_, _, rest, _) = collect(&["x", "-l", "--", "y"]);
        assert_eq!(rest, ["-l", "--", "x", "y"].map(String::from));
    }

    #[test]
    fn clusters_and_attached_arguments() {
        let (opts, errs, _, _) = collect(&["-lfe", "-p80", "-n", "w", "-NV"]);
        assert_eq!(
            opts,
            vec![
                o(b'l', None),
                o(b'f', None),
                o(b'e', None),
                o(b'p', Some("80")),
                o(b'n', Some("w")),
                o(b'N', None),
                o(b'V', None)
            ]
        );
        assert!(errs.is_empty());
    }

    #[test]
    fn short_option_diagnostics() {
        let (opts, errs, _, _) = collect(&["-n"]);
        assert_eq!(opts, vec![o(b'?', None)]);
        assert_eq!(errs, ["prog: option requires an argument -- 'n'"]);
        let (opts, errs, _, _) = collect(&["-lz"]);
        assert_eq!(opts, vec![o(b'l', None), o(b'?', None)]);
        assert_eq!(errs, ["prog: unrecognized option '-z'"]);
        let (_, errs, _, _) = collect(&["-:"]);
        assert_eq!(errs, ["prog: invalid option -- ':'"]);
    }

    #[test]
    fn long_option_diagnostics() {
        let (_, errs, _, _) = collect(&["--nosuch"]);
        assert_eq!(errs, ["prog: unrecognized option '--nosuch'"]);
        let (_, errs, _, _) = collect(&["-zzz"]);
        assert_eq!(errs, ["prog: unrecognized option '-zzz'"]);
        let (_, errs, _, _) = collect(&["--list=1"]);
        assert_eq!(errs, ["prog: option '--list' doesn't allow an argument"]);
        let (_, errs, _, _) = collect(&["-list=1"]);
        assert_eq!(errs, ["prog: option '-list' doesn't allow an argument"]);
        let (opts, errs, _, _) = collect(&["--port"]);
        assert_eq!(opts, vec![o(b'?', None)]);
        assert_eq!(errs, ["prog: option '--port' requires an argument"]);
        let (_, errs, _, _) = collect(&["--max"]);
        assert_eq!(errs.len(), 1);
        let (_, errs, _, _) = collect(&["--c"]);
        assert_eq!(
            errs,
            ["prog: option '--c' is ambiguous; possibilities: '--credit' '--convert-to-custum-rules' '--custom-rules' '--cloud_salt'"]
        );
        // An exact match wins over longer names sharing its prefix.
        let (opts, errs, _, _) = collect(&["--new", "x"]);
        assert_eq!(opts, vec![o(b'n', Some("x"))]);
        assert!(errs.is_empty());
        let (opts, _, _, _) = collect(&["--no-exit", "--co"]);
        assert_eq!(opts[0], o(b'N', None));
    }

    #[test]
    fn accessors_and_sticky_optopt() {
        let mut g = Getopt::new(vec![b"prog".to_vec(), b"-z".to_vec()]);
        g.set_optopt(7);
        assert_eq!(g.optopt(), 7);
        let mut seen = Vec::new();
        assert_eq!(g.next(&mut |m| seen.push(m.to_vec())), b'?' as i32);
        assert_eq!(g.optopt(), 0);
        assert_eq!(seen.len(), 1);
        assert_eq!(g.optarg(), None);
        assert_eq!(g.next(&mut |_| ()), -1);
        assert_eq!(g.optind(), 2);
    }

    #[test]
    fn calling_again_after_the_end_keeps_returning_the_end() {
        let mut g = Getopt::new(["prog", "-l", "--", "x", "y"].map(|a| a.as_bytes().to_vec()).to_vec());
        assert_eq!(g.next(&mut |_| ()), b'l' as i32);
        assert_eq!(g.next(&mut |_| ()), -1);
        assert_eq!(g.optind(), 3);
        // optind now sits below last_nonopt; the state is clamped, not corrupted.
        assert_eq!(g.next(&mut |_| ()), -1);
        assert_eq!(g.optind(), 3);
    }

    #[test]
    fn stderr_sink_writes_without_panicking() {
        stderr_sink(b"prog: test message from the getopt unit test");
    }
}
