//! JSONL trace recorder / reader and the tiny hand-written JSON writer/parser (no `serde`).
//!

use std::cell::RefCell;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::rc::Rc;

use crate::decode::type_name;

/// Direction of a traced packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    /// client -> server
    Tx,
    /// server -> client
    Rx,
}

impl Dir {
    /// `"tx"` / `"rx"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Dir::Tx => "tx",
            Dir::Rx => "rx",
        }
    }
}

/// One traced packet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceRecord {
    pub seq: u64,
    pub t_ms: u64,
    pub dir: Dir,
    pub client: String,
    pub channel: u8,
    pub reliable: bool,
    /// first byte of the packet (0 for an empty packet)
    pub ty: u8,
    pub len: usize,
    pub data: Vec<u8>,
}

/// Lowercase hex of `b`.
pub fn to_hex(b: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(b.len() * 2);
    for &x in b {
        s.push(H[(x >> 4) as usize] as char);
        s.push(H[(x & 15) as usize] as char);
    }
    s
}

/// Parse a hex string (no spaces, even length).
pub fn from_hex(s: &str) -> Option<Vec<u8>> {
    let b = s.as_bytes();
    if b.len() % 2 != 0 {
        return None;
    }
    let nib = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity(b.len() / 2);
    for p in b.chunks(2) {
        out.push((nib(p[0])? << 4) | nib(p[1])?);
    }
    Some(out)
}

/// JSON string literal with escapes.
pub fn json_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
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
    o.push('"');
    o
}

/// Write one record as a JSON line with the fixed key order of.
pub fn write_record(w: &mut impl Write, r: &TraceRecord) -> io::Result<()> {
    writeln!(
        w,
        "{{\"seq\":{},\"t\":{},\"dir\":\"{}\",\"client\":{},\"ch\":{},\"rel\":{},\"type\":{},\"name\":{},\"len\":{},\"hex\":\"{}\"}}",
        r.seq,
        r.t_ms,
        r.dir.as_str(),
        json_str(&r.client),
        r.channel,
        r.reliable,
        r.ty,
        json_str(type_name(r.ty)),
        r.len,
        to_hex(&r.data)
    )
}

// ------------------------------------------------------------------------------------------------
// minimal JSON value + parser
// ------------------------------------------------------------------------------------------------

/// Minimal JSON value (objects keep key order).
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    /// Object member lookup.
    pub fn get(&self, k: &str) -> Option<&Json> {
        match self {
            Json::Obj(m) => m.iter().find(|(kk, _)| kk == k).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Json::Num(n) if *n >= 0.0 => Some(*n as u64),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

struct JP<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> JP<'a> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\r' | b'\n') {
            self.i += 1;
        }
    }
    fn eat(&mut self, c: u8) -> Option<()> {
        self.ws();
        if self.b.get(self.i) == Some(&c) {
            self.i += 1;
            Some(())
        } else {
            None
        }
    }
    fn lit(&mut self, s: &str) -> Option<()> {
        if self.b[self.i..].starts_with(s.as_bytes()) {
            self.i += s.len();
            Some(())
        } else {
            None
        }
    }
    fn hex4(&mut self) -> Option<u32> {
        let s = std::str::from_utf8(self.b.get(self.i..self.i + 4)?).ok()?;
        self.i += 4;
        u32::from_str_radix(s, 16).ok()
    }
    fn string(&mut self) -> Option<String> {
        self.eat(b'"')?;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let c = *self.b.get(self.i)?;
            self.i += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let e = *self.b.get(self.i)?;
                    self.i += 1;
                    let ch = match e {
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'/' => '/',
                        b'\\' => '\\',
                        b'"' => '"',
                        b'u' => {
                            let mut cp = self.hex4()?;
                            if (0xd800..0xdc00).contains(&cp) && self.b[self.i..].starts_with(b"\\u") {
                                self.i += 2;
                                let lo = self.hex4()?;
                                cp = 0x10000 + ((cp - 0xd800) << 10) + (lo.wrapping_sub(0xdc00) & 0x3ff);
                            }
                            char::from_u32(cp).unwrap_or('\u{fffd}')
                        }
                        _ => return None,
                    };
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                }
                c => out.push(c),
            }
        }
        String::from_utf8(out).ok()
    }
    fn value(&mut self, depth: usize) -> Option<Json> {
        if depth > 64 {
            return None;
        }
        self.ws();
        match *self.b.get(self.i)? {
            b'{' => {
                self.i += 1;
                let mut m = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b'}') {
                    self.i += 1;
                    return Some(Json::Obj(m));
                }
                loop {
                    let k = self.string()?;
                    self.eat(b':')?;
                    let v = self.value(depth + 1)?;
                    m.push((k, v));
                    self.ws();
                    match *self.b.get(self.i)? {
                        b',' => self.i += 1,
                        b'}' => {
                            self.i += 1;
                            return Some(Json::Obj(m));
                        }
                        _ => return None,
                    }
                }
            }
            b'[' => {
                self.i += 1;
                let mut a = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b']') {
                    self.i += 1;
                    return Some(Json::Arr(a));
                }
                loop {
                    a.push(self.value(depth + 1)?);
                    self.ws();
                    match *self.b.get(self.i)? {
                        b',' => self.i += 1,
                        b']' => {
                            self.i += 1;
                            return Some(Json::Arr(a));
                        }
                        _ => return None,
                    }
                }
            }
            b'"' => Some(Json::Str(self.string()?)),
            b't' => self.lit("true").map(|_| Json::Bool(true)),
            b'f' => self.lit("false").map(|_| Json::Bool(false)),
            b'n' => self.lit("null").map(|_| Json::Null),
            _ => {
                let s = self.i;
                while self.i < self.b.len() && matches!(self.b[self.i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
                {
                    self.i += 1;
                }
                std::str::from_utf8(&self.b[s..self.i]).ok()?.parse::<f64>().ok().map(Json::Num)
            }
        }
    }
}

/// Parse one JSON document (trailing garbage is an error).
pub fn parse_json(s: &str) -> Option<Json> {
    let mut p = JP { b: s.as_bytes(), i: 0 };
    let v = p.value(0)?;
    p.ws();
    if p.i == p.b.len() {
        Some(v)
    } else {
        None
    }
}

// ------------------------------------------------------------------------------------------------
// sink
// ------------------------------------------------------------------------------------------------

struct SinkInner {
    w: Option<BufWriter<File>>,
    mem: Option<Vec<TraceRecord>>,
    seq: u64,
}

/// Clone-able trace handle shared by several clients.
#[derive(Clone)]
pub struct TraceSink(Rc<RefCell<SinkInner>>);

impl TraceSink {
    /// File sink; writes the `meta` first line.
    pub fn create(path: &Path, label: &str) -> io::Result<TraceSink> {
        if let Some(d) = path.parent() {
            if !d.as_os_str().is_empty() {
                std::fs::create_dir_all(d)?;
            }
        }
        let mut w = BufWriter::new(File::create(path)?);
        let started =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        writeln!(
            w,
            "{{\"meta\":{{\"label\":{},\"server\":\"\",\"started_unix\":{},\"tool\":\"bh_client 1\"}}}}",
            json_str(label),
            started
        )?;
        w.flush()?;
        Ok(TraceSink(Rc::new(RefCell::new(SinkInner { w: Some(w), mem: None, seq: 0 }))))
    }

    /// In-memory sink (tests, CLI printing).
    pub fn memory() -> TraceSink {
        TraceSink(Rc::new(RefCell::new(SinkInner { w: None, mem: Some(Vec::new()), seq: 0 })))
    }

    /// Records held by a memory sink (empty for file sinks).
    pub fn records(&self) -> Vec<TraceRecord> {
        self.0.borrow().mem.clone().unwrap_or_default()
    }

    /// Append one packet record; returns the assigned record.
    pub fn record(&self, dir: Dir, client: &str, t_ms: u64, channel: u8, reliable: bool, data: &[u8]) -> TraceRecord {
        let mut s = self.0.borrow_mut();
        let rec = TraceRecord {
            seq: s.seq,
            t_ms,
            dir,
            client: client.to_string(),
            channel,
            reliable,
            ty: data.first().copied().unwrap_or(0),
            len: data.len(),
            data: data.to_vec(),
        };
        s.seq += 1;
        if let Some(w) = s.w.as_mut() {
            let _ = write_record(w, &rec);
            let _ = w.flush();
        }
        if let Some(m) = s.mem.as_mut() {
            m.push(rec.clone());
        }
        rec
    }

    /// Marker line `{"mark":..,"outcome":..}` (ignored by diffs).
    pub fn mark(&self, mark: &str, outcome: &str) {
        let mut s = self.0.borrow_mut();
        if let Some(w) = s.w.as_mut() {
            let _ = writeln!(w, "{{\"mark\":{},\"outcome\":{}}}", json_str(mark), json_str(outcome));
            let _ = w.flush();
        }
    }
}

/// Read a trace file; skips meta/marker lines and tolerates unknown keys.
pub fn read_trace(path: &Path) -> io::Result<Vec<TraceRecord>> {
    let text = std::fs::read_to_string(path)?;
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some(j) = parse_json(line) else { continue };
        let (Some(seq), Some(dir), Some(hex)) = (j.get("seq"), j.get("dir"), j.get("hex")) else { continue };
        let dir = match dir.as_str() {
            Some("tx") => Dir::Tx,
            Some("rx") => Dir::Rx,
            _ => continue,
        };
        let Some(data) = hex.as_str().and_then(from_hex) else { continue };
        out.push(TraceRecord {
            seq: seq.as_u64().unwrap_or(0),
            t_ms: j.get("t").and_then(Json::as_u64).unwrap_or(0),
            dir,
            client: j.get("client").and_then(Json::as_str).unwrap_or("").to_string(),
            channel: j.get("ch").and_then(Json::as_u64).unwrap_or(0) as u8,
            reliable: j.get("rel").and_then(Json::as_bool).unwrap_or(true),
            ty: data.first().copied().unwrap_or(0),
            len: data.len(),
            data,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(seq: u64, data: Vec<u8>) -> TraceRecord {
        TraceRecord {
            seq,
            t_ms: 5,
            dir: Dir::Rx,
            client: "TCA".into(),
            channel: 0,
            reliable: true,
            ty: data.first().copied().unwrap_or(0),
            len: data.len(),
            data,
        }
    }

    /// U1: round trip, key order, skipping of unknown keys and markers.
    #[test]
    fn u1_round_trip() {
        let recs = vec![
            rec(0, vec![]),
            rec(1, vec![0x0e]),
            rec(
                2,
                (0..70_000u32)
                    .map(|i| (i % 251) as u8)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .enumerate()
                    .map(|(i, b)| if i == 0 { 4 } else { b })
                    .collect(),
            ),
        ];
        let mut buf = Vec::new();
        for r in &recs {
            write_record(&mut buf, r).unwrap();
        }
        let text = String::from_utf8(buf).unwrap();
        let first = text.lines().next().unwrap();
        assert_eq!(first, "{\"seq\":0,\"t\":5,\"dir\":\"rx\",\"client\":\"TCA\",\"ch\":0,\"rel\":true,\"type\":0,\"name\":\"UNDEFINED\",\"len\":0,\"hex\":\"\"}");
        let mut with_extra =
            String::from("{\"meta\":{\"label\":\"x\"}}\n{\"mark\":\"join\",\"outcome\":\"approved\"}\n");
        with_extra.push_str(&text.replace("\"len\"", "\"zzz\":[1,{\"a\":null}],\"len\""));
        let p = std::env::temp_dir().join(format!("bh_trace_test_{}.jsonl", std::process::id()));
        std::fs::write(&p, with_extra).unwrap();
        let back = read_trace(&p).unwrap();
        let _ = std::fs::remove_file(&p);
        assert_eq!(back, recs);
    }

    #[test]
    fn json_basics() {
        let j = parse_json("{\"a\":[1,2.5,\"x\\u00e9\\n\"],\"b\":true,\"c\":null}").unwrap();
        assert_eq!(j.get("b"), Some(&Json::Bool(true)));
        assert_eq!(j.get("a"), Some(&Json::Arr(vec![Json::Num(1.0), Json::Num(2.5), Json::Str("x\u{e9}\n".into())])));
        assert!(parse_json("{\"a\":1} x").is_none());
    }

    #[test]
    fn memory_sink() {
        let s = TraceSink::memory();
        s.record(Dir::Tx, "A", 1, 0, true, &[3, 0, 0]);
        s.record(Dir::Rx, "A", 2, 1, true, &[0x23, 0x26]);
        let r = s.records();
        assert_eq!(r.len(), 2);
        assert_eq!((r[1].seq, r[1].ty, r[1].channel), (1, 0x23, 1));
    }
}
