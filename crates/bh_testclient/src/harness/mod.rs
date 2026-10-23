//! Parity diff harness: shared helpers, error type, paths, temp dirs, minimal JSON.
//!

pub mod gdb_rng;
pub mod inproc;
pub mod normalize;
pub mod refenv;
pub mod save_diff;
pub mod trace_diff;

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Error type of the whole harness.
#[derive(Debug)]
pub enum HarnessError {
    Io(std::io::Error),
    Lmdb(String),
    Plist(String),
    Docker(String),
    Timeout(String),
    Parse(String),
    Usage(String),
}
impl fmt::Display for HarnessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HarnessError::Io(e) => write!(f, "io error: {}", e),
            HarnessError::Lmdb(s) => write!(f, "lmdb error: {}", s),
            HarnessError::Plist(s) => write!(f, "plist error: {}", s),
            HarnessError::Docker(s) => write!(f, "docker error: {}", s),
            HarnessError::Timeout(s) => write!(f, "timeout: {}", s),
            HarnessError::Parse(s) => write!(f, "parse error: {}", s),
            HarnessError::Usage(s) => write!(f, "usage: {}", s),
        }
    }
}
impl std::error::Error for HarnessError {}
impl From<std::io::Error> for HarnessError {
    fn from(e: std::io::Error) -> Self {
        HarnessError::Io(e)
    }
}
/// Harness result alias.
pub type Result<T> = std::result::Result<T, HarnessError>;

const MISSING: &str = "/nonexistent/bh-private-material";
fn env_path(var: &str) -> PathBuf {
    std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(MISSING))
}
/// Optional fixtures directory: env `BH_PRIVATE_DOCS`. Not part of this repository; unset means "unavailable" (tests skip).
pub fn private_docs() -> PathBuf {
    env_path("BH_PRIVATE_DOCS")
}
/// gdb symbol file: env `BH_REF_BINARY` (private).
pub fn ref_binary() -> PathBuf {
    std::env::var_os("BH_REF_BINARY").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(MISSING))
}
/// Reference environment directory: env `BH_REFENV` (private).
pub fn refenv_dir() -> PathBuf {
    env_path("BH_REFENV")
}
/// `types_structs_meta.json` path (env `BH_META_JSON` overrides).
pub fn meta_json_path() -> PathBuf {
    if let Ok(v) = std::env::var("BH_META_JSON") {
        return PathBuf::from(v);
    }
    private_docs().join("types_structs_meta.json")
}
/// Our server binary: env `BH_SERVER_BIN`, else newest of `<repo>/target/{release,debug}/blockheads_server`.
pub fn our_server_bin() -> Result<PathBuf> {
    if let Ok(v) = std::env::var("BH_SERVER_BIN") {
        return Ok(PathBuf::from(v));
    }
    let ws = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut tdirs = vec![ws.join("target")];
    if let Ok(t) = std::env::var("CARGO_TARGET_DIR") {
        tdirs.insert(0, PathBuf::from(t));
    }
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for t in tdirs {
        for prof in ["release", "debug"] {
            let p = t.join(prof).join("blockheads_server");
            if let Ok(m) = std::fs::metadata(&p) {
                let mt = m.modified().unwrap_or(std::time::UNIX_EPOCH);
                if best.as_ref().map_or(true, |(b, _)| mt > *b) {
                    best = Some((mt, p));
                }
            }
        }
    }
    best.map(|(_, p)| p).ok_or_else(|| {
        HarnessError::Usage(
            "no blockheads_server binary found (cargo build -p blockheads_server, or set BH_SERVER_BIN)".into(),
        )
    })
}
/// True iff `BH_PARITY=1`, docker works and image `blockheads-ref:1.7.1` exists.
pub fn parity_enabled() -> bool {
    std::env::var("BH_PARITY").map(|v| v == "1").unwrap_or(false)
        && refenv::docker_available()
        && refenv::image_present("blockheads-ref:1.7.1")
}
/// `BH_PARITY_FULL=1`: also the 4x/16x widths and debug tests.
pub fn parity_full() -> bool {
    std::env::var("BH_PARITY_FULL").map(|v| v == "1").unwrap_or(false)
}
/// Free UDP port whose TCP twin is also free.
pub fn free_udp_port() -> u16 {
    for _ in 0..200 {
        if let Ok(s) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if let Ok(a) = s.local_addr() {
                let p = a.port();
                if std::net::TcpListener::bind(("0.0.0.0", p)).is_ok() {
                    return p;
                }
            }
        }
    }
    0
}

/// First port of `n` consecutive free UDP ports (0 when none was found).
pub fn free_udp_port_range(n: u16) -> u16 {
    for _ in 0..200 {
        let base = free_udp_port();
        if base != 0
            && base.checked_add(n).is_some()
            && (base..base + n).all(|p| std::net::UdpSocket::bind(("0.0.0.0", p)).is_ok())
        {
            return base;
        }
    }
    0
}

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);
/// Unique directory under `std::env::temp_dir()`, removed on drop.
pub struct TempDir(PathBuf);
impl TempDir {
    pub fn new(tag: &str) -> Result<TempDir> {
        let n = TMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
        let p = std::env::temp_dir().join(format!("bhdiff-{}-{}-{}-{}", tag, std::process::id(), n, nanos));
        std::fs::create_dir_all(&p)?;
        Ok(TempDir(p))
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
    /// Keep the directory (no removal on drop) and return its path.
    pub fn keep(self) -> PathBuf {
        let p = self.0.clone();
        std::mem::forget(self);
        p
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Strips the NSLog prefix `^\d{4}-\d\d-\d\d \d\d:\d\d:\d\d\.\d{3} \S+\[\d+\] `.
pub fn strip_nslog_prefix(line: &str) -> &str {
    let b = line.as_bytes();
    let pat = b"dddd-dd-dd dd:dd:dd.ddd ";
    if b.len() < pat.len() {
        return line;
    }
    for (i, p) in pat.iter().enumerate() {
        let ok = if *p == b'd' { b[i].is_ascii_digit() } else { b[i] == *p };
        if !ok {
            return line;
        }
    }
    let rest = &line[pat.len()..];
    // \S+ \[\d+ \] followed by a space
    let rb = rest.as_bytes();
    let mut i = 0;
    while i < rb.len() && !rb[i].is_ascii_whitespace() {
        i += 1;
    }
    if i == 0 {
        return line;
    }
    // backtrack to the last '[' within the non-space run
    let run = &rest[..i];
    let lb = match run.rfind('[') {
        Some(p) if p > 0 => p,
        _ => return line,
    };
    let inner = &run[lb + 1..];
    if !inner.ends_with(']') {
        return line;
    }
    let digits = &inner[..inner.len() - 1];
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return line;
    }
    if rb.get(i) != Some(&b' ') {
        return line;
    }
    &rest[i + 1..]
}

/// Read a file; "does not exist" gives `Ok(None)`.
pub fn read_opt(path: &Path) -> Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Flat JSON value of the trace records.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
}
impl Json {
    pub fn as_str(&self) -> Option<&str> {
        if let Json::Str(s) = self {
            Some(s)
        } else {
            None
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        if let Json::Num(n) = self {
            Some(*n)
        } else {
            None
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let Json::Bool(b) = self {
            Some(*b)
        } else {
            None
        }
    }
}

/// Parse one flat JSON object (string/number/bool/null members only).
pub fn parse_flat_json_object(line: &str) -> Result<BTreeMap<String, Json>> {
    let b: Vec<char> = line.chars().collect();
    let mut i = 0usize;
    let err = |m: &str, at: usize| HarnessError::Parse(format!("json: {} at {}", m, at));
    fn ws(b: &[char], i: &mut usize) {
        while *i < b.len() && b[*i].is_whitespace() {
            *i += 1;
        }
    }
    fn string(b: &[char], i: &mut usize) -> std::result::Result<String, usize> {
        if b.get(*i) != Some(&'"') {
            return Err(*i);
        }
        *i += 1;
        let mut s = String::new();
        loop {
            let c = *b.get(*i).ok_or(*i)?;
            *i += 1;
            match c {
                '"' => return Ok(s),
                '\\' => {
                    let e = *b.get(*i).ok_or(*i)?;
                    *i += 1;
                    match e {
                        '"' => s.push('"'),
                        '\\' => s.push('\\'),
                        '/' => s.push('/'),
                        'b' => s.push('\u{8}'),
                        'f' => s.push('\u{c}'),
                        'n' => s.push('\n'),
                        'r' => s.push('\r'),
                        't' => s.push('\t'),
                        'u' => {
                            let mut cp = 0u32;
                            for _ in 0..4 {
                                let h = b.get(*i).and_then(|c| c.to_digit(16)).ok_or(*i)?;
                                cp = cp * 16 + h;
                                *i += 1;
                            }
                            if (0xd800..0xdc00).contains(&cp) && b.get(*i) == Some(&'\\') && b.get(*i + 1) == Some(&'u')
                            {
                                let mut lo = 0u32;
                                let save = *i;
                                *i += 2;
                                for _ in 0..4 {
                                    let h = b.get(*i).and_then(|c| c.to_digit(16)).ok_or(*i)?;
                                    lo = lo * 16 + h;
                                    *i += 1;
                                }
                                if (0xdc00..0xe000).contains(&lo) {
                                    cp = 0x10000 + ((cp - 0xd800) << 10) + (lo - 0xdc00);
                                } else {
                                    *i = save;
                                }
                            }
                            s.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                        }
                        _ => return Err(*i),
                    }
                }
                c => s.push(c),
            }
        }
    }
    ws(&b, &mut i);
    if b.get(i) != Some(&'{') {
        return Err(err("expected '{'", i));
    }
    i += 1;
    let mut out = BTreeMap::new();
    ws(&b, &mut i);
    if b.get(i) == Some(&'}') {
        return Ok(out);
    }
    loop {
        ws(&b, &mut i);
        let k = string(&b, &mut i).map_err(|p| err("bad key", p))?;
        ws(&b, &mut i);
        if b.get(i) != Some(&':') {
            return Err(err("expected ':'", i));
        }
        i += 1;
        ws(&b, &mut i);
        let v = match b.get(i) {
            Some('"') => Json::Str(string(&b, &mut i).map_err(|p| err("bad string", p))?),
            Some('t') if b[i..].starts_with(&['t', 'r', 'u', 'e']) => {
                i += 4;
                Json::Bool(true)
            }
            Some('f') if b[i..].starts_with(&['f', 'a', 'l', 's', 'e']) => {
                i += 5;
                Json::Bool(false)
            }
            Some('n') if b[i..].starts_with(&['n', 'u', 'l', 'l']) => {
                i += 4;
                Json::Null
            }
            Some(c) if c.is_ascii_digit() || *c == '-' => {
                let st = i;
                while i < b.len() && (b[i].is_ascii_digit() || matches!(b[i], '-' | '+' | '.' | 'e' | 'E')) {
                    i += 1;
                }
                let t: String = b[st..i].iter().collect();
                Json::Num(t.parse::<f64>().map_err(|_| err("bad number", st))?)
            }
            _ => return Err(err("unsupported value (nested?)", i)),
        };
        out.insert(k, v);
        ws(&b, &mut i);
        match b.get(i) {
            Some(',') => i += 1,
            Some('}') => return Ok(out),
            _ => return Err(err("expected ',' or '}'", i)),
        }
    }
}

/// Escape `s` for a JSON string body (without quotes).
pub fn json_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o
}

/// Lower-case hex of `b`.
pub fn to_hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push_str(&format!("{:02x}", x));
    }
    s
}
/// Decode hex (even length) or `None`.
pub fn from_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len() / 2).map(|i| u8::from_str_radix(s.get(2 * i..2 * i + 2)?, 16).ok()).collect()
}

/// Glob match: `*` matches any run of characters including `/`. Used for record ids (`env/db/key`).
pub fn glob_any(pat: &str, text: &str) -> bool {
    glob(pat, text, true)
}
/// Glob match for plist paths: single `*` stays within one `/` segment, `**` crosses segments.
pub fn glob_path(pat: &str, text: &str) -> bool {
    glob(pat, text, false)
}
fn glob(pat: &str, text: &str, star_crosses: bool) -> bool {
    let p: Vec<char> = pat.chars().collect();
    let t: Vec<char> = text.chars().collect();
    fn rec(p: &[char], t: &[char], sc: bool) -> bool {
        if p.is_empty() {
            return t.is_empty();
        }
        if p[0] == '*' {
            let mut n = 1;
            let mut cross = sc;
            while p.get(n) == Some(&'*') {
                cross = true;
                n += 1;
            }
            let rest = &p[n..];
            let mut k = 0;
            loop {
                if rec(rest, &t[k..], sc) {
                    return true;
                }
                if k >= t.len() || (!cross && t[k] == '/') {
                    return false;
                }
                k += 1;
            }
        }
        !t.is_empty() && p[0] == t[0] && rec(&p[1..], &t[1..], sc)
    }
    rec(&p, &t, star_crosses)
}

/// Run a text script (test_client DSL,) against `addr` and return one trace per client alias.
///
/// This is the ONE place coupled to `test_client`. `dir` receives the per-client JSONL files; the
/// returned events are the `rx`/`tx` records of every client in file order (client alias in `TraceEvent.client`).
pub fn run_script(script: &str, addr: std::net::SocketAddr, dir: &Path) -> Result<Vec<normalize::TraceEvent>> {
    run_script_ports(script, addr, dir, None)
}

/// [`run_script`] with the n-th client bound to UDP port `port_base + n` (see `RunOptions::client_port_base`).
pub fn run_script_ports(
    script: &str,
    addr: std::net::SocketAddr,
    dir: &Path,
    port_base: Option<u16>,
) -> Result<Vec<normalize::TraceEvent>> {
    use crate::scenario::{RunOptions, Scenario};
    let mut sc = Scenario::parse(script).map_err(|e| HarnessError::Parse(e.to_string()))?;
    sc.name = "script".to_string();
    let cfg = crate::ClientConfig { addr, ..Default::default() };
    let opts = RunOptions { trace_dir: Some(dir.to_path_buf()), client_port_base: port_base, ..Default::default() };
    let report = sc.run(&cfg, &opts).map_err(|e| HarnessError::Parse(e.to_string()))?;
    let mut all = Vec::new();
    for (_, p) in &report.traces {
        all.extend(normalize::read_jsonl(p)?);
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// S9.
    #[test]
    fn s9_strip_prefix() {
        assert_eq!(
            strip_nslog_prefix("2026-10-16 20:30:41.724 blockheads_server171[6] World load complete."),
            "World load complete."
        );
        assert_eq!(strip_nslog_prefix("World load complete."), "World load complete.");
        assert_eq!(strip_nslog_prefix("  continuation line"), "  continuation line");
        assert_eq!(strip_nslog_prefix("2026-10-16 20:30:41.724 x[1]nospace"), "2026-10-16 20:30:41.724 x[1]nospace");
    }

    #[test]
    fn port_range_is_free_and_consecutive() {
        let base = free_udp_port_range(4);
        assert_ne!(base, 0);
        let socks: Vec<_> = (base..base + 4).map(|p| std::net::UdpSocket::bind(("0.0.0.0", p)).unwrap()).collect();
        assert_eq!(socks.len(), 4);
    }

    #[test]
    fn json_roundtrip() {
        let m = parse_flat_json_object(r#"{"seq":12,"t":2623,"dir":"rx","rel":true,"x":null,"s":"a\"b\\n\u00e9"}"#)
            .unwrap();
        assert_eq!(m["seq"], Json::Num(12.0));
        assert_eq!(m["rel"], Json::Bool(true));
        assert_eq!(m["x"], Json::Null);
        assert_eq!(m["s"].as_str().unwrap(), "a\"b\\n\u{e9}");
        assert_eq!(json_escape("a\"b\n"), "a\\\"b\\n");
        assert!(parse_flat_json_object(r#"{"meta":{"a":1}}"#).is_err());
    }

    #[test]
    fn globs_and_tempdir() {
        assert!(glob_any("world_db/*/worldv2", "world_db/main/worldv2"));
        assert!(glob_any("world_db/*", "world_db/dw/1_2/3"));
        assert!(glob_path("/a/*/c", "/a/b/c"));
        assert!(!glob_path("/a/*/c", "/a/b/x/c"));
        assert!(glob_path("/a/**/c", "/a/b/x/c"));
        let p;
        {
            let t = TempDir::new("t").unwrap();
            p = t.path().to_path_buf();
            assert!(p.is_dir());
        }
        assert!(!p.exists());
        assert!(from_hex("0aff").unwrap() == vec![10, 255]);
        assert!(!private_docs().as_os_str().is_empty());
    }
}
