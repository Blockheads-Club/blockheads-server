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
/// `DynamicObjectType` numbers and names (without the `DYNAMIC_OBJECT_TYPE_` prefix) of a `save_format_enums.txt`.
fn parse_object_classes(text: &str) -> BTreeMap<u32, String> {
    let mut m = BTreeMap::new();
    let mut inside = false;
    for l in text.lines() {
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
    m
}
fn object_classes() -> &'static BTreeMap<u32, String> {
    static C: OnceLock<BTreeMap<u32, String>> = OnceLock::new();
    C.get_or_init(|| {
        let text = std::fs::read_to_string(super::private_docs().join("save_format_enums.txt")).unwrap_or_default();
        parse_object_classes(&text)
    })
}
/// Candidate `*NetData` struct names for an object batch `(kind, objType)`.
fn struct_candidates(kind: u8, obj_type: u8) -> Vec<String> {
    struct_candidates_in(object_classes(), kind, obj_type)
}
fn struct_candidates_in(classes: &BTreeMap<u32, String>, kind: u8, obj_type: u8) -> Vec<String> {
    let suffix = match kind {
        0x07 | 0x0a | 0x46 | 0x45 => "CreationNetData",
        _ => "UpdateNetData",
    };
    let mut v = Vec::new();
    if let Some(n) = classes.get(&(obj_type as u32)) {
        let c = camel(n);
        v.push(format!("{}{}", c, suffix));
        if let Some(r) = n.strip_prefix("NPC_") {
            v.push(format!("{}{}", camel(r), suffix));
        }
    }
    v
}

/// Zero the padding ranges of every candidate struct inside `bytes`.
fn mask_padding(bytes: &mut [u8], cands: &[String], padding: &PaddingMeta) {
    for c in cands {
        for (off, len) in padding.ranges(c) {
            for j in *off..(*off + *len).min(bytes.len()) {
                bytes[j] = 0;
            }
        }
    }
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
    /// also mask `date` of relayed (client-written) 0x24 chat: scripts stamp their messages with the wall clock,
    /// so runs at different times differ there by construction
    pub mask_relayed_chat_date: bool,
}
impl Default for NormalizeOptions {
    fn default() -> Self {
        let padding = padding_or_empty(&super::meta_json_path());
        NormalizeOptions {
            keep_tx: false,
            keep_channel: true,
            padding,
            volatile: None,
            mask_plist_keys: vec!["worldTime".into(), "noRainTimer".into(), "date".into()],
            sort_bursts: false,
            burst_gap_ms: 5.0,
            extra_masks: vec![],
            mask_relayed_chat_date: false,
        }
    }
}

/// The padding metadata of `path`, or none (with a warning) when it cannot be read.
fn padding_or_empty(path: &Path) -> PaddingMeta {
    PaddingMeta::load(path).unwrap_or_else(|e| {
        eprintln!("warning: padding meta not loaded ({}); object blob padding is not statically masked", e);
        PaddingMeta::empty()
    })
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
            let skip: Vec<String> =
                if (server_generated || o.mask_relayed_chat_date) && o.mask_plist_keys.iter().any(|k| k == "date") {
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
                                mask_padding(&mut bytes, &cands, &o.padding);
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

    fn opts() -> NormalizeOptions {
        NormalizeOptions {
            padding: PaddingMeta::load(&super::super::meta_json_path()).unwrap_or_default(),
            ..Default::default()
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
        let o = NormalizeOptions { mask_relayed_chat_date: true, ..opts() };
        let nr = |b: Vec<u8>| normalize_trace(&[ev(b)], &o)[0].sections.clone();
        assert_eq!(nr(chat("", "2026-01-01T00:00:00Z")), nr(chat("", "2027-01-01T00:00:00Z")));
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

#[cfg(test)]
#[path = "normalize_golden.rs"]
mod golden;

#[cfg(test)]
mod unit {
    use super::*;
    use blockheads_server::foundation::gsdict::GsDict;
    use blockheads_server::foundation::gzip::gzip_deflate;
    use blockheads_server::plist::xml_write::write_xml;

    fn gz_plist(v: &PlistValue) -> Vec<u8> {
        gzip_deflate(&write_xml(v))
    }
    fn dict(pairs: Vec<(&str, PlistValue)>) -> PlistValue {
        let mut m = GsDict::new();
        for (k, v) in pairs {
            m.insert(k.to_string(), v);
        }
        PlistValue::dict(m)
    }
    fn ev(bytes: Vec<u8>) -> TraceEvent {
        TraceEvent { t_ms: 0.0, dir: Dir::Rx, channel: 2, reliable: true, client: "C".into(), bytes }
    }
    fn norm(bytes: Vec<u8>) -> NormMsg {
        normalize_trace(&[ev(bytes)], &opts()).remove(0)
    }
    fn opts() -> NormalizeOptions {
        NormalizeOptions { padding: PaddingMeta::empty(), ..NormalizeOptions::default() }
    }
    fn names(m: &NormMsg) -> Vec<&str> {
        m.sections.iter().map(|(n, _)| n.as_str()).collect()
    }

    #[test]
    fn jsonl_round_trip_and_errors() {
        let tmp = super::super::TempDir::new("jsonl-unit").unwrap();
        let p = tmp.path().join("t.jsonl");
        let evs = vec![
            TraceEvent {
                t_ms: 12.4,
                dir: Dir::Tx,
                channel: 1,
                reliable: false,
                client: "A\"b".into(),
                bytes: vec![0x24, 1],
            },
            ev(vec![]),
        ];
        write_jsonl(&p, &evs).unwrap();
        let mut text = std::fs::read_to_string(&p).unwrap();
        text = format!(
            "{{\"meta\":1}}\n\n{{\"mark\":\"x\"}}\n{{\"seq\":9,\"mark\":1,\"dir\":\"rx\",\"hex\":\"00\"}}\n{text}"
        );
        std::fs::write(&p, text).unwrap();
        let back = read_jsonl(&p).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!((back[0].dir, back[0].channel, back[0].reliable, back[0].t_ms), (Dir::Tx, 1, false, 12.0));
        assert_eq!(back[0].client, "A\"b");
        assert_eq!(back[1].bytes, Vec::<u8>::new());
        for (bad, what) in [
            ("{\"dir\":\"rx\"}", "hex"),
            ("{\"dir\":\"sideways\",\"hex\":\"00\"}", "dir"),
            ("{\"dir\":\"rx\",\"hex\":\"0\"}", "hex"),
            ("not json", "json"),
        ] {
            std::fs::write(&p, bad).unwrap();
            let e = read_jsonl(&p).unwrap_err().to_string();
            assert!(e.contains(what) || e.contains("expected"), "{bad}: {e}");
        }
        std::fs::write(&p, "{\"dir\":\"rx\",\"hex\":\"00\"}").unwrap();
        let minimal = read_jsonl(&p).unwrap();
        assert_eq!(
            (minimal[0].channel, minimal[0].reliable, minimal[0].client.as_str(), minimal[0].t_ms),
            (0, true, "", 0.0)
        );
        assert!(read_jsonl(&tmp.path().join("missing")).is_err());
    }

    #[test]
    fn static_masks_are_clipped() {
        assert_eq!(static_mask_ranges(0x17, 20), vec![1..9]);
        assert_eq!(static_mask_ranges(0x17, 5), vec![1..5]);
        assert_eq!(static_mask_ranges(0x13, 20), vec![2..8]);
        assert_eq!(static_mask_ranges(0x3f, 20), vec![3..9]);
        assert_eq!(static_mask_ranges(0x2c, 20), vec![5..9]);
        assert_eq!(static_mask_ranges(0x1a, 20), vec![10..17]);
        assert_eq!(static_mask_ranges(0x29, 20), vec![19..20]);
        assert_eq!(static_mask_ranges(0x2a, 20), vec![14..17]);
        assert!(static_mask_ranges(0x17, 1).is_empty() && static_mask_ranges(0x55, 20).is_empty());
    }

    #[test]
    fn padding_meta_reads_json() {
        let tmp = super::super::TempDir::new("meta-unit").unwrap();
        let p = tmp.path().join("meta.json");
        let u = '\\';
        let json = format!(
            "{{\"x\":[true,false,null,-1.5e1,\"a{u}n{u}t{u}r{u}\"{u}{u}{u}u0041{u}q\"],\"padding_ranges_own_level\":{{\"FooCreationNetData\":[[2,3],[10,1],[1],[\"a\",1]],\"Weird\":5,\"Empty\":[],\"Mixed\":[5,[0,1]]}},\"y\":{{}}}}"
        );
        std::fs::write(&p, json).unwrap();
        let m = PaddingMeta::load(&p).unwrap();
        assert_eq!(m.ranges("FooCreationNetData"), &[(2, 3), (10, 1)]);
        assert!(m.knows("Empty") && m.ranges("Empty").is_empty() && m.knows("Weird") && !m.knows("Nope"));
        assert!(m.ranges("Nope").is_empty());
        for (bad, what) in [
            ("{", "invalid JSON"),
            ("[1,2]", "no padding_ranges_own_level"),
            ("{\"a\":1}", "no padding"),
            ("{\"a\" 1}", "invalid JSON"),
            ("{\"a\":[1 2]}", "invalid JSON"),
            ("{\"a\":x}", "invalid JSON"),
            ("{\"a\":\"b", "invalid JSON"),
            ("{1:2}", "invalid JSON"),
            ("[1,", "invalid JSON"),
            ("{\"a\":nul}", "invalid JSON"),
        ] {
            std::fs::write(&p, bad).unwrap();
            let e = PaddingMeta::load(&p).unwrap_err().to_string();
            assert!(e.contains(what), "{bad}: {e}");
        }
        assert!(PaddingMeta::load(&tmp.path().join("missing")).is_err());
        assert!(padding_or_empty(&tmp.path().join("missing")).ranges("x").is_empty());
        std::fs::write(&p, "{\"padding_ranges_own_level\":5}").unwrap();
        assert!(PaddingMeta::load(&p).unwrap_err().to_string().contains("no padding"));
        std::fs::write(&p, "{\"padding_ranges_own_level\":{\"S\":[[0,1]]}}").unwrap();
        assert!(padding_or_empty(&p).knows("S"));
        let _ = NormalizeOptions::default();
    }

    #[test]
    fn json_value_parser_corner_cases() {
        for ok in ["[]", "{}", " [ 1 , 2 ] ", "{\"a\":{\"b\":[]}}", "\"s\"", "true", "false", "null", "-0.5E+2", "[{}]"]
        {
            assert!(jv_parse(ok).is_some(), "{ok}");
        }
        let u = '\\';
        let esc = format!("\"{u}n{u}t{u}r{u}{u}{u}x{u}u00e9{u}ud83d\"");
        assert!(matches!(jv_parse(&esc), Some(JV::Str(s)) if s == "\n\t\r\\x\u{e9}?"));
        for bad in [
            "",
            "[",
            "[1,",
            "[1 2]",
            "{\"a\"}",
            "{\"a\":1,}",
            "{,}",
            "\"x",
            "\"\\",
            "\"\\u12\"",
            "-",
            "@",
            "{\"a\":1 \"b\":2}",
        ] {
            assert!(jv_parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn object_class_tables() {
        let text = "## enum Other\n1\tDYNAMIC_OBJECT_TYPE_NOPE\n## enum DynamicObjectType\n5\tDYNAMIC_OBJECT_TYPE_NPC_DODO\n7\tDYNAMIC_OBJECT_TYPE_BLOCKHEAD\nbad\tDYNAMIC_OBJECT_TYPE_X\n8\tOTHER_PREFIX\nno tab line\n## enum Next\n9\tDYNAMIC_OBJECT_TYPE_LATE\n";
        let m = parse_object_classes(text);
        assert_eq!(m.len(), 2);
        assert_eq!(m[&5], "NPC_DODO");
        assert_eq!(camel("NPC_DODO_X"), "NpcDodoX");
        assert_eq!(camel(""), "");
        assert_eq!(struct_candidates_in(&m, 0x07, 5), vec!["NpcDodoCreationNetData", "DodoCreationNetData"]);
        assert_eq!(struct_candidates_in(&m, 0x08, 7), vec!["BlockheadUpdateNetData"]);
        assert!(struct_candidates_in(&m, 0x08, 99).is_empty());
        let _ = struct_candidates(0x07, 5);
        let mut padding = PaddingMeta::default();
        padding.map.insert("BlockheadUpdateNetData".into(), vec![(1, 2), (6, 100)]);
        let mut bytes = vec![9u8; 8];
        mask_padding(&mut bytes, &struct_candidates_in(&m, 0x08, 7), &padding);
        assert_eq!(bytes, [9, 0, 0, 9, 9, 9, 0, 0]);
    }

    #[test]
    fn volatile_bytes_are_learned_and_truncated() {
        let msg = |client: &str, bytes: Vec<u8>| norm_with(client, bytes);
        fn norm_with(client: &str, bytes: Vec<u8>) -> NormMsg {
            let mut e = ev(bytes);
            e.client = client.into();
            normalize_trace(&[e], &opts()).remove(0)
        }
        assert_eq!(VolatileMask::learn(&[vec![msg("A", vec![0x55, 1])]]).volatile_messages(), 0);
        let run3 = vec![msg("A", vec![0x55, 1, 2])];
        let run1 = vec![msg("A", vec![0x55, 1, 2, 3]), msg("A", vec![0x56, 1]), msg("B", vec![0x55, 9])];
        let run2 = vec![msg("A", vec![0x55, 1, 7, 3, 4]), msg("A", vec![0x56, 1]), msg("C", vec![0x55, 9])];
        let vm = VolatileMask::learn(&[run1.clone(), run2, run3]);
        assert_eq!((vm.volatile_messages(), vm.volatile_bytes()), (1, 1));
        let mut o = opts();
        o.volatile = Some(vm);
        let a = normalize_trace(&[ev(vec![0x55, 1, 2, 3, 4, 5])], &o);
        // the client name is part of the key, so only client "A" is affected
        assert_eq!(a[0].sections, norm(vec![0x55, 1, 2, 3, 4, 5]).sections);
        let mut e = ev(vec![0x55, 1, 2, 3, 4, 5]);
        e.client = "A".into();
        let masked = normalize_trace(&[e], &o);
        let body = &masked[0].sections[0].1;
        assert_eq!(body[..], [0x55, 1, 0][..]);
    }

    #[test]
    fn plist_flattening_covers_every_kind() {
        let v = dict(vec![
            ("b", PlistValue::int(-3)),
            ("a", PlistValue::UInt(u64::MAX)),
            ("c", PlistValue::real_f64(1.5)),
            ("d", PlistValue::Bool(true)),
            ("e", PlistValue::date(0.0)),
            ("f", PlistValue::data(vec![1, 2])),
            ("g", PlistValue::Null),
            ("h", PlistValue::array(vec![PlistValue::string("s")])),
            ("skip", PlistValue::int(1)),
        ]);
        let lines = plist_lines(&v, &["skip".to_string()]);
        assert_eq!(lines[0], " = dict");
        assert!(lines.contains(&"/a = unsigned:18446744073709551615".to_string()));
        assert!(lines.contains(&"/b = integer:-3".to_string()));
        assert!(lines.iter().any(|l| l.starts_with("/c = real:3ff8000000000000")));
        assert!(lines.contains(&"/d = bool:true".to_string()) && lines.contains(&"/e = date:0".to_string()));
        assert!(lines.contains(&"/f = data:0102".to_string()) && lines.contains(&"/g = null".to_string()));
        assert!(lines.contains(&"/h = array[1]".to_string()) && lines.contains(&"/h/[0] = string:\"s\"".to_string()));
        assert!(lines.contains(&"/skip = <masked>".to_string()));
    }

    #[test]
    fn handshake_and_chat_messages() {
        let summary = dict(vec![("worldTime", PlistValue::real_f64(5.0)), ("name", PlistValue::string("w"))]);
        let mut pk = vec![0x01];
        pk.extend(gz_plist(&summary));
        let m = norm(pk);
        assert_eq!(names(&m), ["summary.plist"]);
        assert!(String::from_utf8_lossy(&m.sections[0].1).contains("/worldTime = <masked>"));
        assert!(m.text.contains("ch2") && m.text.contains("[summary.plist]"));
        assert_eq!(names(&norm(vec![0x01, 1, 2, 3])), ["raw"]);
        let not_plist = {
            let mut v = vec![0x01];
            v.extend(gzip_deflate(b"<?xml junk"));
            v
        };
        assert_eq!(names(&norm(not_plist)), ["summary.raw"]);
        let chat = |extra: Vec<(&str, PlistValue)>| {
            let mut pairs = vec![("message", PlistValue::string("hi")), ("date", PlistValue::date(1.0))];
            pairs.extend(extra);
            let mut v = vec![0x24];
            v.extend(write_xml(&dict(pairs)));
            v
        };
        let owner = norm(chat(vec![("mod", PlistValue::string("owner"))]));
        assert!(String::from_utf8_lossy(&owner.sections[0].1).contains("/date = <masked>"));
        let relayed = norm(chat(vec![]));
        assert!(String::from_utf8_lossy(&relayed.sections[0].1).contains("/date = date:"));
        let mut array_chat = vec![0x24];
        array_chat.extend(write_xml(&PlistValue::array(vec![])));
        assert_eq!(names(&norm(array_chat)), ["chat.plist"]);
        let mut bad_chat = vec![0x24];
        bad_chat.extend_from_slice(b"<?xml junk");
        assert_eq!(names(&norm(bad_chat)), ["chat.raw"]);
        let mut o = opts();
        o.mask_relayed_chat_date = true;
        let m = normalize_trace(&[ev(chat(vec![]))], &o).remove(0);
        assert!(String::from_utf8_lossy(&m.sections[0].1).contains("/date = <masked>"));
        let o2 = NormalizeOptions { mask_plist_keys: vec![], keep_channel: false, ..opts() };
        let m = normalize_trace(&[ev(chat(vec![("mod", PlistValue::string("owner"))]))], &o2).remove(0);
        assert!(
            String::from_utf8_lossy(&m.sections[0].1).contains("/date = date:")
                && m.channel == 0
                && m.text.contains("ch0")
        );
    }

    fn block_msg(r: Option<Vec<u8>>, t: Option<Vec<u8>>, extra: bool) -> Vec<u8> {
        let mut pairs = vec![];
        if let Some(r) = r {
            pairs.push(("r", PlistValue::data(gzip_deflate(&r))));
        }
        if let Some(t) = t {
            pairs.push(("t", PlistValue::data(gzip_deflate(&t))));
        }
        if extra {
            pairs.push(("z", PlistValue::int(4)));
        }
        let mut v = vec![0x04, 0, 0, 0, 0];
        v.extend(write_xml(&dict(pairs)));
        v
    }

    #[test]
    fn terrain_block_messages() {
        let mut r = vec![7u8; 0x400];
        r.extend(write_xml(&dict(vec![("k", PlistValue::int(1))])));
        let full = norm(block_msg(Some(r), Some(vec![1; 12]), true));
        assert_eq!(names(&full), ["macroIndex", "r.light", "r.plist", "t.tiles", "t.trailer", "extra.plist"]);
        assert_eq!(full.sections[3].1.len(), 7);
        let short = norm(block_msg(Some(vec![1, 2]), Some(vec![9]), false));
        assert_eq!(names(&short), ["macroIndex", "r.raw", "t.raw"]);
        let missing = norm(block_msg(None, None, false));
        assert_eq!(names(&missing), ["macroIndex", "r.missing", "t.missing"]);
        let mut not_dict = vec![0x04, 0, 0, 0, 0];
        not_dict.extend(write_xml(&PlistValue::array(vec![])));
        assert_eq!(names(&norm(not_dict)), ["macroIndex", "plist.raw"]);
        assert_eq!(names(&norm(vec![0x04, 1, 2, 3, 4])), ["body"]);
    }

    #[test]
    fn object_batches_and_generic_masks() {
        let blobs = PlistValue::array(vec![PlistValue::data(vec![1, 2, 3]), PlistValue::string("odd")]);
        let mut pk = vec![0x07, 9];
        pk.extend(gz_plist(&blobs));
        let m = norm(pk);
        assert_eq!(names(&m), ["objType", "obj[0]", "obj[1].plist"]);
        assert!(m.text.contains("[obj[1].plist]"));
        assert_eq!(names(&norm(vec![0x08, 9, 1, 2, 3])), ["objType", "batch.raw"]);
        assert_eq!(names(&norm(vec![0x09, 9])), ["body"]);
        let mut o = opts();
        o.extra_masks = vec![(0x30, 1..3), (0x30, 50..60), (0x31, 1..2)];
        let m = normalize_trace(&[ev(vec![0x30, 1, 2, 3, 4])], &o).remove(0);
        assert_eq!(m.sections[0].1, [0x30, 0, 0, 3, 4]);
        let long = norm((0..100).collect());
        assert!(long.text.ends_with("..."));
        assert_eq!(norm(vec![]).ty, 0);
    }

    #[test]
    fn trace_filtering_and_burst_sorting() {
        let mk = |t: f64, client: &str, ty: u8, dir: Dir| TraceEvent {
            t_ms: t,
            dir,
            channel: 0,
            reliable: true,
            client: client.into(),
            bytes: vec![ty, 1],
        };
        let evs = vec![
            mk(0.0, "A", 0x30, Dir::Rx),
            mk(1.0, "A", 0x20, Dir::Rx),
            mk(2.0, "A", 0x10, Dir::Tx),
            mk(100.0, "A", 0x40, Dir::Rx),
            mk(101.0, "B", 0x41, Dir::Rx),
            mk(102.0, "B", 0x39, Dir::Rx),
        ];
        let plain = normalize_trace(&evs, &opts());
        assert_eq!(plain.iter().map(|m| m.ty).collect::<Vec<_>>(), [0x30, 0x20, 0x40, 0x41, 0x39]);
        let with_tx = normalize_trace(&evs, &NormalizeOptions { keep_tx: true, ..opts() });
        assert_eq!(with_tx.len(), 6);
        let sorted = normalize_trace(&evs, &NormalizeOptions { sort_bursts: true, ..opts() });
        assert_eq!(sorted.iter().map(|m| m.ty).collect::<Vec<_>>(), [0x20, 0x30, 0x40, 0x39, 0x41]);
    }
}
