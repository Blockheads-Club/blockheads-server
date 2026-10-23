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
    env_path("BH_REF_BINARY")
}
/// Reference environment directory: env `BH_REFENV` (private).
pub fn refenv_dir() -> PathBuf {
    env_path("BH_REFENV")
}
/// `types_structs_meta.json` path (env `BH_META_JSON` overrides).
pub fn meta_json_path() -> PathBuf {
    meta_json_path_in(std::env::var("BH_META_JSON").ok(), private_docs())
}

fn meta_json_path_in(var: Option<String>, docs: PathBuf) -> PathBuf {
    var.map_or_else(|| docs.join("types_structs_meta.json"), PathBuf::from)
}
/// Our server binary: env `BH_SERVER_BIN`, else newest of `<repo>/target/{release,debug}/blockheads_server`.
pub fn our_server_bin() -> Result<PathBuf> {
    our_server_bin_in(
        std::env::var("BH_SERVER_BIN").ok(),
        std::env::var("CARGO_TARGET_DIR").ok(),
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
    )
}

fn our_server_bin_in(bin: Option<String>, target_dir: Option<String>, ws: &Path) -> Result<PathBuf> {
    if let Some(v) = bin {
        return Ok(PathBuf::from(v));
    }
    let mut tdirs = vec![ws.join("target")];
    tdirs.splice(0..0, target_dir.map(PathBuf::from));
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
    parity_enabled_with(std::env::var("BH_PARITY").ok().as_deref(), refenv::docker_available, refenv::image_present)
}

fn parity_enabled_with(flag: Option<&str>, docker: fn() -> bool, image: fn(&str) -> bool) -> bool {
    flag == Some("1") && docker() && image("blockheads-ref:1.7.1")
}
/// `BH_PARITY_FULL=1`: also the 4x/16x widths and debug tests.
pub fn parity_full() -> bool {
    std::env::var("BH_PARITY_FULL").as_deref() == Ok("1")
}
/// Free UDP port whose TCP twin is also free.
pub fn free_udp_port() -> u16 {
    (0..200).find_map(|_| udp_port_with_free_tcp_twin()).unwrap_or(0)
}

fn udp_port_with_free_tcp_twin() -> Option<u16> {
    let udp = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    let port = udp.local_addr().ok()?.port();
    std::net::TcpListener::bind(("0.0.0.0", port)).ok().map(|_| port)
}

/// First port of `n` consecutive free UDP ports (0 when none was found).
pub fn free_udp_port_range(n: u16) -> u16 {
    (0..200).find_map(|_| range_if_free(n)).unwrap_or(0)
}

fn range_if_free(n: u16) -> Option<u16> {
    let base = free_udp_port();
    let end = base.checked_add(n)?;
    (base != 0 && (base..end).all(|p| std::net::UdpSocket::bind(("0.0.0.0", p)).is_ok())).then_some(base)
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

fn parse_error(e: impl ToString) -> HarnessError {
    HarnessError::Parse(e.to_string())
}

/// [`run_script`] with the n-th client bound to UDP port `port_base + n` (see `RunOptions::client_port_base`).
pub fn run_script_ports(
    script: &str,
    addr: std::net::SocketAddr,
    dir: &Path,
    port_base: Option<u16>,
) -> Result<Vec<normalize::TraceEvent>> {
    use crate::scenario::{RunOptions, Scenario};
    let mut sc = Scenario::parse(script).map_err(parse_error)?;
    sc.name = "script".to_string();
    let cfg = crate::ClientConfig { addr, ..Default::default() };
    let opts = RunOptions { trace_dir: Some(dir.to_path_buf()), client_port_base: port_base, ..Default::default() };
    let report = sc.run(&cfg, &opts).map_err(parse_error)?;
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
    #[test]
    fn error_texts_and_conversion() {
        let io = HarnessError::from(std::io::Error::other("disk"));
        assert_eq!(io.to_string(), "io error: disk");
        for (e, want) in [
            (HarnessError::Lmdb("a".into()), "lmdb error: a"),
            (HarnessError::Plist("b".into()), "plist error: b"),
            (HarnessError::Docker("c".into()), "docker error: c"),
            (HarnessError::Timeout("d".into()), "timeout: d"),
            (HarnessError::Parse("e".into()), "parse error: e"),
            (HarnessError::Usage("f".into()), "usage: f"),
        ] {
            assert_eq!(e.to_string(), want);
        }
    }

    #[test]
    fn configured_paths() {
        assert_eq!(meta_json_path_in(Some("/m.json".into()), PathBuf::from("/d")), PathBuf::from("/m.json"));
        assert_eq!(meta_json_path_in(None, PathBuf::from("/d")), PathBuf::from("/d/types_structs_meta.json"));
        assert!(!ref_binary().as_os_str().is_empty() && !refenv_dir().as_os_str().is_empty());
        let _ = (meta_json_path(), parity_full());
        let _ = parity_enabled();
        assert!(!parity_enabled_with(None, || true, |_| true));
        assert!(!parity_enabled_with(Some("0"), || true, |_| true));
        assert!(!parity_enabled_with(Some("1"), || false, |_| true));
        assert!(!parity_enabled_with(Some("1"), || true, |_| false));
        assert!(parity_enabled_with(Some("1"), || true, |tag| tag == "blockheads-ref:1.7.1"));
    }

    #[test]
    fn server_binary_lookup() {
        assert_eq!(our_server_bin_in(Some("/x/bin".into()), None, Path::new("/ws")).unwrap(), PathBuf::from("/x/bin"));
        let t = TempDir::new("bins").unwrap();
        let err = our_server_bin_in(None, None, &t.path().join("ws")).unwrap_err();
        assert!(err.to_string().contains("no blockheads_server binary found"));
        let a = t.path().join("a");
        std::fs::create_dir_all(a.join("debug")).unwrap();
        std::fs::write(a.join("debug/blockheads_server"), "old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let target = Some(a.display().to_string());
        assert_eq!(
            our_server_bin_in(None, target.clone(), &t.path().join("zz")).unwrap(),
            a.join("debug/blockheads_server")
        );
        // the workspace target dir is searched too; the newer file wins
        let ws = t.path().join("ws");
        std::fs::create_dir_all(ws.join("target/release")).unwrap();
        std::fs::write(ws.join("target/release/blockheads_server"), "newest").unwrap();
        assert_eq!(our_server_bin_in(None, target, &ws).unwrap(), ws.join("target/release/blockheads_server"));
        let _ = our_server_bin();
    }

    #[test]
    fn ports_and_temp_dirs() {
        assert_ne!(free_udp_port(), 0);
        assert_eq!(free_udp_port_range(u16::MAX), 0);
        let t = TempDir::new("keep").unwrap();
        let kept = t.keep();
        assert!(kept.is_dir());
        std::fs::remove_dir_all(&kept).unwrap();
    }

    #[test]
    fn nslog_prefix_rejections() {
        for line in [
            "2026-10-16 20:30:41.724 ",
            "2026-10-16 20:30:41.724  x[1] y",
            "2026-10-16 20:30:41.724 nobracket y",
            "2026-10-16 20:30:41.724 [1] y",
            "2026-10-16 20:30:41.724 x[1 y",
            "2026-10-16 20:30:41.724 x[] y",
            "2026-10-16 20:30:41.724 x[a1] y",
            "2026-10-16 20:30:41.724 x[1]",
            "2026x10-16 20:30:41.724 x[1] y",
        ] {
            assert_eq!(strip_nslog_prefix(line), line, "{line}");
        }
    }

    #[test]
    fn reading_optional_files() {
        let t = TempDir::new("opt").unwrap();
        std::fs::write(t.path().join("f"), b"x").unwrap();
        assert_eq!(read_opt(&t.path().join("f")).unwrap(), Some(b"x".to_vec()));
        assert_eq!(read_opt(&t.path().join("missing")).unwrap(), None);
        assert!(read_opt(t.path()).is_err());
    }

    #[test]
    fn flat_json_forms() {
        let j = Json::Num(1.0);
        assert_eq!((j.as_f64(), j.as_str(), j.as_bool()), (Some(1.0), None, None));
        assert_eq!((Json::Str("s".into()).as_f64(), Json::Bool(true).as_str()), (None, None));
        assert_eq!(Json::Null.as_bool(), None);
        assert_eq!(Json::Bool(true).as_bool(), Some(true));
        assert_eq!(to_hex(&[0, 255, 16]), "00ff10");
        let lone_high = format!("{{\"k\":\"{0}ud83d{0}u0041\"}}", '\\');
        assert_eq!(parse_flat_json_object(&lone_high).unwrap()["k"].as_str().unwrap(), "\u{fffd}A");
        let esc =
            format!("{{\"a\":false,\"b\":\"x{0}/{0}b{0}f{0}n{0}r{0}t{0}ud83d{0}ude00{0}ud83dZ\",\"c\":-2.5e1}}", '\\');
        let m = parse_flat_json_object(&esc).unwrap();
        assert_eq!(m["a"], Json::Bool(false));
        assert_eq!(m["b"].as_str().unwrap(), "x/\u{8}\u{c}\n\r\t\u{1f600}\u{fffd}Z");
        assert_eq!(m["c"], Json::Num(-25.0));
        assert!(parse_flat_json_object("{}").unwrap().is_empty());
        for bad in [
            "[]",
            r#"{"a" 1}"#,
            r#"{"a":[1]}"#,
            r#"{"a":1 "b":2}"#,
            r#"{"a":"\q"}"#,
            r#"{"a":"\u12"}"#,
            r#"{"a":"\ud83d\u12"}"#,
            r#"{a:1}"#,
            r#"{"a":-x}"#,
            r#"{"a":"unterminated"#,
        ] {
            assert!(parse_flat_json_object(bad).is_err(), "{bad}");
        }
        assert_eq!(json_escape("a\\b\r\t\u{1}"), "a\\\\b\\r\\t\\u0001");
        assert_eq!(from_hex("abc"), None);
        assert_eq!(from_hex("zz"), None);
        assert_eq!(from_hex(" 0a "), Some(vec![10]));
        assert!(glob_any("*", "") && glob_any("a*c", "abbbc") && !glob_any("a", "") && glob_any("", ""));
        assert!(glob_path("a/**", "a/b/c") && !glob_path("a/*", "a/b/c"));
    }
}
