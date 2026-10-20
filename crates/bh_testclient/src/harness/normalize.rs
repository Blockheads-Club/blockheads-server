//! Trace reading/writing and canonicalisation of packet traces (padding and time masking).
//!

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::Path;
use std::sync::OnceLock;

use blockheads_server::foundation::gzip::gzip_inflate;
use blockheads_server::plist::helpers::property_list_from_data;
use blockheads_server::plist::value::PlistValue;

use super::{from_hex, json_escape, parse_flat_json_object, to_hex, HarnessError, Json, Result};

/// Direction of a traced packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Rx,
    Tx,
}
/// One application packet of a trace file.
#[derive(Clone, Debug)]
pub struct TraceEvent {
    pub t_ms: f64,
    pub dir: Dir,
    pub channel: u8,
    pub reliable: bool,
    pub client: String,
    /// whole ENet payload, byte 0 = BHNDT type
    pub bytes: Vec<u8>,
}

/// Read a JSONL trace written by `test_client::trace` (keys `seq,t,dir,client,ch,rel,type,name,len,hex`; `meta`/`mark` lines skipped).
pub fn read_jsonl(path: &Path) -> Result<Vec<TraceEvent>> {
    let text = std::fs::read_to_string(path)?;
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let l = line.trim();
        if l.is_empty() || l.starts_with("{\"meta\"") || l.starts_with("{\"mark\"") {
            continue;
        }
        let m = parse_flat_json_object(l)
            .map_err(|e| HarnessError::Parse(format!("{}:{}: {}", path.display(), n + 1, e)))?;
        if m.contains_key("meta") || m.contains_key("mark") {
            continue;
        }
        let bad = |k: &str| HarnessError::Parse(format!("{}:{}: missing/bad key '{}'", path.display(), n + 1, k));
        let hex = m.get("hex").and_then(Json::as_str).ok_or_else(|| bad("hex"))?;
        let dir = match m.get("dir").and_then(Json::as_str) {
            Some("rx") => Dir::Rx,
            Some("tx") => Dir::Tx,
            _ => return Err(bad("dir")),
        };
        out.push(TraceEvent {
            t_ms: m.get("t").and_then(Json::as_f64).unwrap_or(0.0),
            dir,
            channel: m.get("ch").and_then(Json::as_f64).unwrap_or(0.0) as u8,
            reliable: m.get("rel").and_then(Json::as_bool).unwrap_or(true),
            client: m.get("client").and_then(Json::as_str).unwrap_or("").to_string(),
            bytes: from_hex(hex).ok_or_else(|| bad("hex"))?,
        });
    }
    Ok(out)
}

/// Write events in the schema of `read_jsonl`.
pub fn write_jsonl(path: &Path, ev: &[TraceEvent]) -> Result<()> {
    let mut s = String::new();
    for (i, e) in ev.iter().enumerate() {
        let ty = e.bytes.first().copied().unwrap_or(0);
        s.push_str(&format!(
            "{{\"seq\":{},\"t\":{},\"dir\":\"{}\",\"client\":\"{}\",\"ch\":{},\"rel\":{},\"type\":{},\"name\":\"{}\",\"len\":{},\"hex\":\"{}\"}}\n",
            i,
            e.t_ms.round() as u64,
            if e.dir == Dir::Rx { "rx" } else { "tx" },
            json_escape(&e.client),
            e.channel,
            e.reliable,
            ty,
            json_escape(crate::decode::type_name(ty)),
            e.bytes.len(),
            to_hex(&e.bytes)
        ));
    }
    std::fs::write(path, s)?;
    Ok(())
}

/// Packet-offset byte ranges (byte 0 = type, half-open) of padding/time fields per message type, clipped to `packet_len`.
///  Types 0x01/0x24/0x04/object batches are masked at plist level instead and return no ranges.
pub fn static_mask_ranges(ty: u8, packet_len: usize) -> Vec<Range<usize>> {
    let r: &[Range<usize>] = match ty {
        0x17 => &[1..9],
        0x13 => &[2..8],
        0x3f => &[3..9],
        0x2c => &[5..9],
        0x1a => &[10..17],
        0x29 => &[19..21],
        0x2a => &[14..17],
        _ => &[],
    };
    r.iter().filter(|x| x.start < packet_len).map(|x| x.start..x.end.min(packet_len)).collect()
}

// ---------------------------------------------------------------------------------------------- padding metadata

/// Tiny general JSON value, only used to read `types_structs_meta.json`.
#[derive(Debug, Clone)]
enum JV {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<JV>),
    Obj(Vec<(String, JV)>),
}
fn jv_parse(s: &str) -> Option<JV> {
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
    let v = jv_value(&b, &mut i)?;
    Some(v)
}
fn jv_ws(b: &[char], i: &mut usize) {
    while *i < b.len() && b[*i].is_whitespace() {
        *i += 1;
    }
}
fn jv_str(b: &[char], i: &mut usize) -> Option<String> {
    if b.get(*i) != Some(&'"') {
        return None;
    }
    *i += 1;
    let mut s = String::new();
    loop {
        let c = *b.get(*i)?;
        *i += 1;
        match c {
            '"' => return Some(s),
            '\\' => {
                let e = *b.get(*i)?;
                *i += 1;
                match e {
                    'n' => s.push('\n'),
                    't' => s.push('\t'),
                    'r' => s.push('\r'),
                    'u' => {
                        let h: String = b.get(*i..*i + 4)?.iter().collect();
                        *i += 4;
                        s.push(char::from_u32(u32::from_str_radix(&h, 16).ok()?).unwrap_or('?'));
                    }
                    c => s.push(c),
                }
            }
            c => s.push(c),
        }
    }
}
fn jv_value(b: &[char], i: &mut usize) -> Option<JV> {
    jv_ws(b, i);
    match *b.get(*i)? {
        '{' => {
            *i += 1;
            let mut m = Vec::new();
            jv_ws(b, i);
            if b.get(*i) == Some(&'}') {
                *i += 1;
                return Some(JV::Obj(m));
            }
            loop {
                jv_ws(b, i);
                let k = jv_str(b, i)?;
                jv_ws(b, i);
                if b.get(*i) != Some(&':') {
                    return None;
                }
                *i += 1;
                let v = jv_value(b, i)?;
                m.push((k, v));
                jv_ws(b, i);
                match b.get(*i)? {
                    ',' => *i += 1,
                    '}' => {
                        *i += 1;
                        return Some(JV::Obj(m));
                    }
                    _ => return None,
                }
            }
        }
        '[' => {
            *i += 1;
            let mut a = Vec::new();
            jv_ws(b, i);
            if b.get(*i) == Some(&']') {
                *i += 1;
                return Some(JV::Arr(a));
            }
            loop {
                a.push(jv_value(b, i)?);
                jv_ws(b, i);
                match b.get(*i)? {
                    ',' => *i += 1,
                    ']' => {
                        *i += 1;
                        return Some(JV::Arr(a));
                    }
                    _ => return None,
                }
            }
        }
        '"' => Some(JV::Str(jv_str(b, i)?)),
        't' => {
            *i += 4;
            Some(JV::Bool(true))
        }
        'f' => {
            *i += 5;
            Some(JV::Bool(false))
        }
        'n' => {
            *i += 4;
            Some(JV::Null)
        }
        _ => {
            let st = *i;
            while *i < b.len() && (b[*i].is_ascii_digit() || matches!(b[*i], '-' | '+' | '.' | 'e' | 'E')) {
                *i += 1;
            }
            let t: String = b[st..*i].iter().collect();
            t.parse().ok().map(JV::Num)
        }
    }
}

/// Struct name -> padding `(offset, len)` ranges (own level) from `types_structs_meta.json` `padding_ranges_own_level`.
#[derive(Clone, Debug, Default)]
pub struct PaddingMeta {
    map: BTreeMap<String, Vec<(usize, usize)>>,
}
impl PaddingMeta {
    pub fn load(path: &Path) -> Result<PaddingMeta> {
        let text = std::fs::read_to_string(path)?;
        let v = jv_parse(&text).ok_or_else(|| HarnessError::Parse(format!("{}: invalid JSON", path.display())))?;
        let mut map = BTreeMap::new();
        if let JV::Obj(top) = v {
            for (k, val) in top {
                if k != "padding_ranges_own_level" {
                    continue;
                }
                if let JV::Obj(structs) = val {
                    for (name, ranges) in structs {
                        let mut out = Vec::new();
                        if let JV::Arr(rs) = ranges {
                            for r in rs {
                                if let JV::Arr(p) = r {
                                    if let (Some(JV::Num(o)), Some(JV::Num(l))) = (p.first(), p.get(1)) {
                                        out.push((*o as usize, *l as usize));
                                    }
                                }
                            }
                        }
                        map.insert(name, out);
                    }
                }
            }
        }
        if map.is_empty() {
            return Err(HarnessError::Parse(format!("{}: no padding_ranges_own_level", path.display())));
        }
        Ok(PaddingMeta { map })
    }
    pub fn empty() -> Self {
        PaddingMeta::default()
    }
    pub fn ranges(&self, struct_name: &str) -> &[(usize, usize)] {
        self.map.get(struct_name).map(|v| v.as_slice()).unwrap_or(&[])
    }
    /// Whether the struct is known at all.
    pub fn knows(&self, struct_name: &str) -> bool {
        self.map.contains_key(struct_name)
    }
}

fn camel(s: &str) -> String {
    s.split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let l = p.to_ascii_lowercase();
            let mut c = l.chars();
            c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
        })
        .collect()
}
fn object_classes() -> &'static BTreeMap<u32, String> {
    static C: OnceLock<BTreeMap<u32, String>> = OnceLock::new();
    C.get_or_init(|| {
        let mut m = BTreeMap::new();
        if let Ok(t) = std::fs::read_to_string(super::private_docs().join("save_format_enums.txt")) {
            let mut inside = false;
            for l in t.lines() {
                if l.starts_with("## enum ") {
                    inside = l.trim() == "## enum DynamicObjectType";
                    continue;
                }
                if !inside {
                    continue;
                }
                if let Some((n, name)) = l.split_once('\t') {
                    if let (Ok(n), Some(rest)) = (n.parse::<u32>(), name.trim().strip_prefix("DYNAMIC_OBJECT_TYPE_")) {
                        m.insert(n, rest.to_string());
                    }
                }
            }
        }
        m
    })
}
/// Candidate `*NetData` struct names for an object batch `(kind, objType)`.
fn struct_candidates(kind: u8, obj_type: u8) -> Vec<String> {
    let suffix = match kind {
        0x07 | 0x0a | 0x46 | 0x45 => "CreationNetData",
        _ => "UpdateNetData",
    };
    let mut v = Vec::new();
    if let Some(n) = object_classes().get(&(obj_type as u32)) {
        let c = camel(n);
        v.push(format!("{}{}", c, suffix));
        if let Some(r) = n.strip_prefix("NPC_") {
            v.push(format!("{}{}", camel(r), suffix));
        }
    }
    v
}

// ---------------------------------------------------------------------------------------------- normalised messages

/// Canonical form of one message.
#[derive(Clone, Debug)]
pub struct NormMsg {
    pub ty: u8,
    pub channel: u8,
    pub client: String,
    pub sections: Vec<(String, Vec<u8>)>,
    pub text: String,
    pub t_ms: f64,
}
impl NormMsg {
    /// Concatenation of all sections (the "canonical byte" space of `VolatileMask`).
    pub fn canonical(&self) -> Vec<u8> {
        self.sections.iter().flat_map(|(_, b)| b.iter().copied()).collect()
    }
}

/// Bytes that differ between runs of the reference.
#[derive(Clone, Debug, Default)]
pub struct VolatileMask {
    /// (client, type, nth occurrence) -> (canonical byte indices, truncate-to length if run lengths differed)
    map: BTreeMap<(String, u8, usize), (BTreeSet<usize>, Option<usize>)>,
}
fn occurrence_keys(run: &[NormMsg]) -> Vec<(String, u8, usize)> {
    let mut cnt: BTreeMap<(String, u8), usize> = BTreeMap::new();
    run.iter()
        .map(|m| {
            let c = cnt.entry((m.client.clone(), m.ty)).or_insert(0);
            let k = (m.client.clone(), m.ty, *c);
            *c += 1;
            k
        })
        .collect()
}
impl VolatileMask {
    /// Every canonical byte that differs between any two runs is volatile.
    pub fn learn(runs: &[Vec<NormMsg>]) -> VolatileMask {
        let mut vm = VolatileMask::default();
        if runs.len() < 2 {
            return vm;
        }
        let keyed: Vec<BTreeMap<(String, u8, usize), Vec<u8>>> = runs
            .iter()
            .map(|r| occurrence_keys(r).into_iter().zip(r.iter().map(|m| m.canonical())).collect())
            .collect();
        for (k, base) in &keyed[0] {
            for other in &keyed[1..] {
                if let Some(o) = other.get(k) {
                    let e = vm.map.entry(k.clone()).or_default();
                    let n = base.len().min(o.len());
                    for i in 0..n {
                        if base[i] != o[i] {
                            e.0.insert(i);
                        }
                    }
                    if base.len() != o.len() {
                        e.1 = Some(e.1.map_or(n, |x| x.min(n)));
                    }
                }
            }
        }
        vm.map.retain(|_, v| !v.0.is_empty() || v.1.is_some());
        vm
    }
    /// Number of messages with at least one volatile byte.
    pub fn volatile_messages(&self) -> usize {
        self.map.len()
    }
    /// Total number of volatile bytes.
    pub fn volatile_bytes(&self) -> usize {
        self.map.values().map(|v| v.0.len()).sum()
    }
    fn apply(&self, msgs: &mut [NormMsg]) {
        let keys = occurrence_keys(msgs);
        for (m, k) in msgs.iter_mut().zip(keys) {
            if let Some((idx, trunc)) = self.map.get(&k) {
                let mut off = 0usize;
                let mut remaining = trunc.unwrap_or(usize::MAX);
                for (_, sec) in m.sections.iter_mut() {
                    let orig = sec.len();
                    for (i, b) in sec.iter_mut().enumerate() {
                        if idx.contains(&(off + i)) {
                            *b = 0;
                        }
                    }
                    if remaining != usize::MAX {
                        let keep = remaining.min(sec.len());
                        sec.truncate(keep);
                        remaining -= keep;
                    }
                    off += orig;
                }
            }
        }
    }
}

/// Normalisation options.
#[derive(Clone, Debug)]
pub struct NormalizeOptions {
    /// default false: compare server -> client only
    pub keep_tx: bool,
    pub keep_channel: bool,
    pub padding: PaddingMeta,
    pub volatile: Option<VolatileMask>,
    /// `date` applies to server-generated 0x24 chat, every other key to the 0x01 world summary
    pub mask_plist_keys: Vec<String>,
    pub sort_bursts: bool,
    pub burst_gap_ms: f64,
    /// extra packet-offset masks `(type, range)` (CLI `--mask-type`)
    pub extra_masks: Vec<(u8, Range<usize>)>,
}
impl Default for NormalizeOptions {
    fn default() -> Self {
        let padding = match PaddingMeta::load(&super::meta_json_path()) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("warning: padding meta not loaded ({}); object blob padding is not statically masked", e);
                PaddingMeta::empty()
            }
        };
        NormalizeOptions {
            keep_tx: false,
            keep_channel: true,
            padding,
            volatile: None,
            mask_plist_keys: vec!["worldTime".into(), "noRainTimer".into(), "date".into()],
            sort_bursts: false,
            burst_gap_ms: 5.0,
            extra_masks: vec![],
        }
    }
}

const MASKED: &str = "<masked>";

fn flat_lines(v: &PlistValue, path: &str, skip: &[String], out: &mut Vec<String>) {
    let line = |k: &str, val: String| format!("{} = {}", k, val);
    match v {
        PlistValue::String(s) => out.push(line(path, format!("string:{:?}", s.s))),
        PlistValue::Int(i) => out.push(line(path, format!("integer:{}", i))),
        PlistValue::UInt(i) => out.push(line(path, format!("unsigned:{}", i))),
        PlistValue::Real { value, .. } => out.push(line(path, format!("real:{:016x}({})", value.to_bits(), value))),
        PlistValue::Bool(b) => out.push(line(path, format!("bool:{}", b))),
        PlistValue::Date(d) => out.push(line(path, format!("date:{}", d))),
        PlistValue::Data(d) => out.push(line(path, format!("data:{}", to_hex(&d.bytes)))),
        PlistValue::Null => out.push(line(path, "null".into())),
        PlistValue::Array(a) => {
            out.push(line(path, format!("array[{}]", a.items.len())));
            for (i, it) in a.items.iter().enumerate() {
                flat_lines(it, &format!("{}/[{}]", path, i), skip, out);
            }
        }
        PlistValue::Dict(d) => {
            out.push(line(path, "dict".into()));
            let mut keys: Vec<(&String, &PlistValue)> = d.map.iter().collect();
            keys.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            for (k, it) in keys {
                let p = format!("{}/{}", path, k);
                if path.is_empty() && skip.iter().any(|s| s == k) {
                    out.push(line(&p, MASKED.into()));
                } else {
                    flat_lines(it, &p, skip, out);
                }
            }
        }
    }
}
/// Flatten a plist to sorted `path = kind:value` lines (top-level keys in `skip` become `<masked>`).
pub fn plist_lines(v: &PlistValue, skip: &[String]) -> Vec<String> {
    let mut o = Vec::new();
    flat_lines(v, "", skip, &mut o);
    o
}
fn lines_bytes(l: &[String]) -> Vec<u8> {
    l.join("\n").into_bytes()
}

fn plist_section(name: &str, data: &[u8], skip: &[String]) -> (String, Vec<u8>) {
    match property_list_from_data(data) {
        Some(p) => (format!("{}.plist", name), lines_bytes(&plist_lines(&p, skip))),
        None => (format!("{}.raw", name), data.to_vec()),
    }
}

fn normalize_one(e: &TraceEvent, o: &NormalizeOptions) -> NormMsg {
    let b = &e.bytes;
    let ty = b.first().copied().unwrap_or(0);
    let body = if b.is_empty() { &b[..] } else { &b[1..] };
    let mut sections: Vec<(String, Vec<u8>)> = Vec::new();
    let skip01: Vec<String> = o.mask_plist_keys.iter().filter(|k| *k != "date").cloned().collect();
    match ty {
        0x01 => match gzip_inflate(body) {
            Some(raw) => sections.push(plist_section("summary", &raw, &skip01)),
            None => sections.push(("raw".into(), body.to_vec())),
        },
        0x24 => {
            let server_generated = property_list_from_data(body)
                .map(|p| match &p {
                    PlistValue::Dict(d) => {
                        d.map
                            .get(&"mod".to_string())
                            .map_or(false, |m| matches!(m, PlistValue::String(s) if s.s == "owner"))
                            && d.map.contains_key(&"date".to_string())
                    }
                    _ => false,
                })
                .unwrap_or(false);
            let skip: Vec<String> = if server_generated && o.mask_plist_keys.iter().any(|k| k == "date") {
                vec!["date".into()]
            } else {
                vec![]
            };
            sections.push(plist_section("chat", body, &skip));
        }
        0x04 if body.len() > 4 => {
            sections.push(("macroIndex".into(), body[..4].to_vec()));
            match property_list_from_data(&body[4..]) {
                Some(PlistValue::Dict(d)) => {
                    let get = |k: &str| match d.map.get(&k.to_string()) {
                        Some(PlistValue::Data(x)) => Some(x.bytes.clone()),
                        _ => None,
                    };
                    match get("r").and_then(|r| gzip_inflate(&r)) {
                        Some(r) if r.len() >= 0x400 => {
                            sections.push(("r.light".into(), r[..0x400].to_vec()));
                            sections.push(plist_section("r", &r[0x400..], &[]));
                        }
                        Some(r) => sections.push(("r.raw".into(), r)),
                        None => sections.push(("r.missing".into(), vec![])),
                    }
                    match get("t").and_then(|t| gzip_inflate(&t)) {
                        Some(t) if t.len() >= 5 => {
                            let n = t.len() - 5;
                            sections.push(("t.tiles".into(), t[..n].to_vec()));
                            sections.push(("t.trailer".into(), t[n..].to_vec()));
                        }
                        Some(t) => sections.push(("t.raw".into(), t)),
                        None => sections.push(("t.missing".into(), vec![])),
                    }
                    // other keys (unexpected): compare them as plist lines
                    let mut extra = Vec::new();
                    for (k, v) in d.map.iter() {
                        if k != "r" && k != "t" {
                            flat_lines(v, &format!("/{}", k), &[], &mut extra);
                        }
                    }
                    if !extra.is_empty() {
                        extra.sort();
                        sections.push(("extra.plist".into(), lines_bytes(&extra)));
                    }
                }
                _ => sections.push(("plist.raw".into(), body[4..].to_vec())),
            }
        }
        0x07 | 0x08 | 0x09 | 0x46 | 0x0a | 0x0b | 0x0c | 0x45 if body.len() > 1 => {
            sections.push(("objType".into(), vec![body[0]]));
            let blobs = gzip_inflate(&body[1..]).and_then(|raw| property_list_from_data(&raw));
            match blobs {
                Some(PlistValue::Array(a)) => {
                    let cands = struct_candidates(ty, body[0]);
                    for (i, it) in a.items.iter().enumerate() {
                        match it {
                            PlistValue::Data(x) => {
                                let mut bytes = x.bytes.clone();
                                for c in &cands {
                                    for (off, len) in o.padding.ranges(c) {
                                        for j in *off..(*off + *len).min(bytes.len()) {
                                            bytes[j] = 0;
                                        }
                                    }
                                }
                                sections.push((format!("obj[{}]", i), bytes));
                            }
                            other => {
                                sections.push((format!("obj[{}].plist", i), lines_bytes(&plist_lines(other, &[]))))
                            }
                        }
                    }
                }
                _ => sections.push(("batch.raw".into(), body[1..].to_vec())),
            }
        }
        _ => {
            let mut bytes = b.clone();
            let mut ranges = static_mask_ranges(ty, bytes.len());
            for (t, r) in &o.extra_masks {
                if *t == ty && r.start < bytes.len() {
                    ranges.push(r.start..r.end.min(bytes.len()));
                }
            }
            for r in ranges {
                for x in &mut bytes[r] {
                    *x = 0;
                }
            }
            sections.push(("body".into(), bytes));
        }
    }
    // extra masks on structured types: applied to the first byte-oriented section is ambiguous, so they only affect the generic path.
    let mut text = format!(
        "{} (0x{:02x}) ch{} client={} len={}",
        crate::decode::type_name(ty),
        ty,
        if o.keep_channel { e.channel } else { 0 },
        e.client,
        b.len()
    );
    for (n, s) in &sections {
        if n.ends_with(".plist") {
            text.push_str(&format!("\n  [{}]\n    {}", n, String::from_utf8_lossy(s).replace('\n', "\n    ")));
        } else {
            let hex = to_hex(&s[..s.len().min(48)]);
            text.push_str(&format!("\n  [{}] {}B {}{}", n, s.len(), hex, if s.len() > 48 { "..." } else { "" }));
        }
    }
    NormMsg {
        ty,
        channel: if o.keep_channel { e.channel } else { 0 },
        client: e.client.clone(),
        sections,
        text,
        t_ms: e.t_ms,
    }
}

/// Canonicalise a trace.
pub fn normalize_trace(ev: &[TraceEvent], o: &NormalizeOptions) -> Vec<NormMsg> {
    let mut out: Vec<NormMsg> =
        ev.iter().filter(|e| o.keep_tx || e.dir == Dir::Rx).map(|e| normalize_one(e, o)).collect();
    if let Some(v) = &o.volatile {
        v.apply(&mut out);
    }
    if o.sort_bursts {
        let mut res: Vec<NormMsg> = Vec::with_capacity(out.len());
        let mut group: Vec<NormMsg> = Vec::new();
        let flush = |g: &mut Vec<NormMsg>, res: &mut Vec<NormMsg>| {
            g.sort_by(|a, b| (a.ty, a.canonical()).cmp(&(b.ty, b.canonical())));
            res.append(g);
        };
        for m in out {
            if let Some(last) = group.last() {
                if m.t_ms - last.t_ms >= o.burst_gap_ms || m.client != last.client {
                    flush(&mut group, &mut res);
                }
            }
            group.push(m);
        }
        flush(&mut group, &mut res);
        out = res;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn vec_dir(name: &str) -> PathBuf {
        super::super::private_docs().join("protocol_vectors").join(name)
    }
    fn load_bins(name: &str, prefix_filter: &dyn Fn(&str) -> bool) -> Vec<TraceEvent> {
        let dir = vec_dir(name);
        let mut files: Vec<PathBuf> = match std::fs::read_dir(&dir) {
            Ok(r) => r.map(|e| e.unwrap().path()).filter(|p| p.extension().map_or(false, |x| x == "bin")).collect(),
            Err(_) => {
                eprintln!("SKIP: {} missing", dir.display());
                return vec![];
            }
        };
        files.sort();
        files
            .into_iter()
            .filter(|p| prefix_filter(p.file_name().unwrap().to_str().unwrap()))
            .enumerate()
            .map(|(i, p)| TraceEvent {
                t_ms: i as f64 * 100.0,
                dir: Dir::Rx,
                channel: 0,
                reliable: true,
                client: "TCA".into(),
                bytes: std::fs::read(p).unwrap(),
            })
            .collect()
    }
    fn opts() -> NormalizeOptions {
        NormalizeOptions {
            padding: PaddingMeta::load(&super::super::meta_json_path()).unwrap_or_default(),
            ..Default::default()
        }
    }

    /// S10 part 1: golden vectors normalise to themselves regardless of timestamps.
    #[test]
    fn s10_golden_time_independent() {
        for dir in ["solo", "two_clients"] {
            let ev = load_bins(dir, &|n| !n.starts_with("B_"));
            if ev.is_empty() {
                continue;
            }
            let a = normalize_trace(&ev, &opts());
            let mut ev2 = ev.clone();
            for (i, e) in ev2.iter_mut().enumerate() {
                e.t_ms = 5.0 + (i as f64) * 777.0;
            }
            let b = normalize_trace(&ev2, &opts());
            assert!(!a.is_empty());
            assert_eq!(a.len(), b.len());
            for (x, y) in a.iter().zip(b.iter()) {
                assert_eq!(x.sections, y.sections);
            }
            // block messages must have been split into sections
            if let Some(m) = a.iter().find(|m| m.ty == 0x04) {
                assert!(m.sections.iter().any(|(n, _)| n == "t.tiles"), "{}", m.text);
            }
        }
    }

    fn ev(bytes: Vec<u8>) -> TraceEvent {
        TraceEvent { t_ms: 0.0, dir: Dir::Rx, channel: 0, reliable: true, client: "C".into(), bytes }
    }

    /// S10 part 2: padding and time masks.
    #[test]
    fn s10_masks() {
        let o = opts();
        let n = |b: Vec<u8>| normalize_trace(&[ev(b)], &o).remove(0).sections;
        // 0x13 garbage vs zeros
        let mut a = vec![0x13, 0x01, 0xdf, 0x3b, 0, 0, 0, 0, 7, 7, 7];
        let mut b = a.clone();
        b[2] = 0;
        b[3] = 0;
        assert_eq!(n(a.clone()), n(b.clone()));
        a[8] = 9;
        assert_ne!(n(a), n(b));
        // 0x17: time bytes masked, flag byte not
        let h1 = vec![0x17, 1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut h2 = h1.clone();
        h2[3] = 99;
        assert_eq!(n(h1.clone()), n(h2));
        let mut h3 = h1.clone();
        h3[9] = 1;
        assert_ne!(n(h1), n(h3));
        // 0x24: server chat date masked, relayed chat date not
        let chat = |m: &str, d: &str| {
            let mut v = vec![0x24];
            v.extend_from_slice(
                format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?><plist version=\"1.0\"><dict><key>message</key><string>hi</string>{}<key>date</key><date>{}</date></dict></plist>",
                    m, d
                )
                .as_bytes(),
            );
            v
        };
        let srv = "<key>mod</key><string>owner</string>";
        assert_eq!(n(chat(srv, "2026-01-01T00:00:00Z")), n(chat(srv, "2027-01-01T00:00:00Z")));
        assert_ne!(n(chat("", "2026-01-01T00:00:00Z")), n(chat("", "2027-01-01T00:00:00Z")));
    }

    #[test]
    fn volatile_learn_and_roundtrip() {
        let mk = |x: u8| vec![ev(vec![0x0f, 1, 2, x, 4]), ev(vec![0x0f, 9, 9, 9, 9])];
        let o = NormalizeOptions::default();
        let r1 = normalize_trace(&mk(1), &o);
        let r2 = normalize_trace(&mk(2), &o);
        let vm = VolatileMask::learn(&[r1.clone(), r2]);
        assert_eq!(vm.volatile_bytes(), 1);
        let o2 = NormalizeOptions { volatile: Some(vm), ..Default::default() };
        assert_eq!(normalize_trace(&mk(3), &o2)[0].sections, normalize_trace(&mk(77), &o2)[0].sections);
        // jsonl
        let tmp = super::super::TempDir::new("jl").unwrap();
        let p = tmp.path().join("t.jsonl");
        let evs = mk(5);
        write_jsonl(&p, &evs).unwrap();
        let back = read_jsonl(&p).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].bytes, evs[0].bytes);
    }
}
