//! Record-level save diff (port of `tools/savetools/`), with plist-structural and tile-level reporting.
//!

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use blockheads_server::foundation::gzip::{gzip_deflate, gzip_inflate};
use blockheads_server::plist::helpers::property_list_from_data;
use blockheads_server::plist::value::PlistValue;

use super::{glob_any, glob_path, json_escape, HarnessError, Result};

/// The three LMDB environments of a save.
pub const ENVS: [&str; 3] = ["world_db", "server_db", "lightBlocks"];
/// `sizeof(Tile)`.
pub const TILE_SIZE: usize = 64;
/// tiles.
pub const TILES_BYTES: usize = 65536;
/// tiles + version u8 + updateIndex u32.
pub const BLOCK_BYTES: usize = 65541;

/// `(name, offset, size)` of every byte of `struct Tile` including padding.
pub const TILE_FIELDS: [(&str, usize, usize); 29] = [
    ("typeIndex", 0, 1),
    ("backWallTypeIndex", 1, 1),
    ("zoneTypeIndex", 2, 1),
    ("contents", 3, 1),
    ("partialContentLeft", 4, 1),
    ("gatherProgress", 5, 1),
    ("light", 6, 1),
    ("sunLight", 7, 1),
    ("seasonOffset", 8, 1),
    ("exploredFraction", 9, 1),
    ("terrainSlowFactor", 10, 1),
    ("foregroundContents", 11, 1),
    ("backgroundContents", 12, 1),
    ("pad@13", 13, 1),
    ("artificialLightR", 14, 2),
    ("artificialLightG", 16, 2),
    ("artificialLightB", 18, 2),
    ("artificialHeat", 20, 2),
    ("onFire", 22, 1),
    ("pad@23", 23, 1),
    ("dynamicObjectOwnerOld", 24, 4),
    ("paintFront", 28, 2),
    ("paintTop", 30, 2),
    ("paintRight", 32, 2),
    ("paintLeft", 34, 2),
    ("paintBot", 36, 2),
    ("pad@38", 38, 2),
    ("dynamicObjectOwner", 40, 8),
    ("pad@48", 48, 16),
];

/// Name of the `Tile` field containing byte `offset_in_tile` (0..64).
pub fn tile_field_at(offset_in_tile: usize) -> &'static str {
    for (n, o, s) in TILE_FIELDS.iter() {
        if offset_in_tile >= *o && offset_in_tile < o + s {
            return n;
        }
    }
    "?"
}

/// Identity of one record. Files: env `"file"`, db `""`, key = file name.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct RecordId {
    pub env: String,
    pub db: String,
    pub key: Vec<u8>,
}
impl RecordId {
    /// Key as text, non-UTF-8 bytes as `\xNN`.
    pub fn key_string(&self) -> String {
        let mut s = String::new();
        for chunk in self.key.utf8_chunks() {
            s.push_str(chunk.valid());
            for b in chunk.invalid() {
                s.push_str(&format!("\\x{:02x}", b));
            }
        }
        s
    }
}
impl fmt::Display for RecordId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}/{}", self.env, self.db, self.key_string())
    }
}

/// Differences that are ignored.
#[derive(Clone, Debug, PartialEq)]
pub enum Mask {
    /// every `<date>` leaf: only "is a date" is compared
    AllDates,
    /// glob on `env/db/key` and on the plist path
    Plist { record: String, path: String },
    /// record ignored entirely (counted as masked)
    Record(String),
    /// byte offsets inside `Tile` ignored in every `blocks` record
    TileBytes(Vec<usize>),
}
impl Mask {
    /// Parse the CLI spec: `dates` | `plist:<record glob>#<path glob>` | `record:<glob>` | `tile:13,23,38`.
    pub fn parse(spec: &str) -> Result<Mask> {
        if spec == "dates" {
            return Ok(Mask::AllDates);
        }
        if let Some(r) = spec.strip_prefix("plist:") {
            let (rec, path) = r
                .split_once('#')
                .ok_or_else(|| HarnessError::Usage(format!("mask '{}': expected plist:<record>#<path>", spec)))?;
            return Ok(Mask::Plist { record: rec.to_string(), path: path.to_string() });
        }
        if let Some(r) = spec.strip_prefix("record:") {
            return Ok(Mask::Record(r.to_string()));
        }
        if let Some(r) = spec.strip_prefix("tile:") {
            let mut v = Vec::new();
            for t in r.split(',').filter(|t| !t.is_empty()) {
                let n: usize = t
                    .trim()
                    .parse()
                    .map_err(|_| HarnessError::Usage(format!("mask '{}': bad offset '{}'", spec, t)))?;
                if n >= TILE_SIZE {
                    return Err(HarnessError::Usage(format!("mask '{}': offset {} >= 64", spec, n)));
                }
                v.push(n);
            }
            return Ok(Mask::TileBytes(v));
        }
        Err(HarnessError::Usage(format!("unknown mask spec '{}'", spec)))
    }
}
impl fmt::Display for Mask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mask::AllDates => write!(f, "dates"),
            Mask::Plist { record, path } => write!(f, "plist:{}#{}", record, path),
            Mask::Record(g) => write!(f, "record:{}", g),
            Mask::TileBytes(v) => write!(f, "tile:{}", v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")),
        }
    }
}

/// Comparison options.
#[derive(Clone, Debug)]
pub struct SaveDiffOptions {
    pub masks: Vec<Mask>,
    /// gzip-wrapped records must also match in COMPRESSED bytes
    pub strict_gzip: bool,
    pub max_tile_diffs_per_block: usize,
    pub max_detail: usize,
}
impl Default for SaveDiffOptions {
    fn default() -> Self {
        SaveDiffOptions { masks: vec![], strict_gzip: true, max_tile_diffs_per_block: 8, max_detail: 200 }
    }
}
impl SaveDiffOptions {
    /// masks = `[AllDates, Plist{"world_db/main/worldv2","/hostPort"}]`, strict gzip.
    pub fn parity_default() -> Self {
        SaveDiffOptions {
            masks: vec![
                Mask::AllDates,
                Mask::Plist { record: "world_db/main/worldv2".into(), path: "/hostPort".into() },
            ],
            ..Default::default()
        }
    }
}

/// One differing tile field.
#[derive(Clone, Debug)]
pub struct TileDiff {
    pub tile_index: usize,
    pub local_x: usize,
    pub local_y: usize,
    pub world_x: i64,
    pub world_y: i64,
    pub field: &'static str,
    pub a: u64,
    pub b: u64,
}
/// Kind of a structural plist difference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlistDiffKind {
    OnlyInA,
    OnlyInB,
    TypeMismatch,
    ValueMismatch,
    LengthMismatch,
}
/// One structural plist difference.
#[derive(Clone, Debug)]
pub struct PlistDiff {
    pub path: String,
    pub kind: PlistDiffKind,
    pub a: String,
    pub b: String,
}

/// One differing record.
#[derive(Debug)]
pub enum RecordDiff {
    OnlyInA(RecordId),
    OnlyInB(RecordId),
    KindMismatch {
        id: RecordId,
        a: &'static str,
        b: &'static str,
    },
    Block {
        id: RecordId,
        trailer_a: (u8, u32),
        trailer_b: (u8, u32),
        differing_tiles: usize,
        differing_bytes: usize,
        sample: Vec<TileDiff>,
    },
    Plist {
        id: RecordId,
        diffs: Vec<PlistDiff>,
    },
    PlistRawOnly {
        id: RecordId,
        first_diff_offset: usize,
        len_a: usize,
        len_b: usize,
    },
    GzipRawOnly {
        id: RecordId,
        len_a: usize,
        len_b: usize,
    },
    Bytes {
        id: RecordId,
        len_a: usize,
        len_b: usize,
        differing_bytes: usize,
        first_diff_offset: usize,
    },
    Text {
        id: RecordId,
        first_diff_line: usize,
        a: String,
        b: String,
    },
}
impl RecordDiff {
    /// The record this diff is about.
    pub fn id(&self) -> &RecordId {
        match self {
            RecordDiff::OnlyInA(id) | RecordDiff::OnlyInB(id) => id,
            RecordDiff::KindMismatch { id, .. }
            | RecordDiff::Block { id, .. }
            | RecordDiff::Plist { id, .. }
            | RecordDiff::PlistRawOnly { id, .. }
            | RecordDiff::GzipRawOnly { id, .. }
            | RecordDiff::Bytes { id, .. }
            | RecordDiff::Text { id, .. } => id,
        }
    }
    /// Human-readable lines (first one is the headline).
    pub fn lines(&self) -> Vec<String> {
        match self {
            RecordDiff::OnlyInA(id) => vec![format!("only in A: {}", id)],
            RecordDiff::OnlyInB(id) => vec![format!("only in B: {}", id)],
            RecordDiff::KindMismatch { id, a, b } => vec![format!("differs: {} value kind A={} B={}", id, a, b)],
            RecordDiff::Block { id, trailer_a, trailer_b, differing_tiles, differing_bytes, sample } => {
                let mut v = vec![format!(
                    "differs: {} differing_tiles={} differing_bytes={} trailer A=(ver {}, upd {}) B=(ver {}, upd {})",
                    id, differing_tiles, differing_bytes, trailer_a.0, trailer_a.1, trailer_b.0, trailer_b.1
                )];
                let name = id.key_string();
                for t in sample {
                    v.push(format!(
                        "  blocks {} tile({},{}) world({},{}) {} a={} b={}",
                        name, t.local_x, t.local_y, t.world_x, t.world_y, t.field, t.a, t.b
                    ));
                }
                v
            }
            RecordDiff::Plist { id, diffs } => {
                let mut v = vec![format!("differs: {} plist ({} differences)", id, diffs.len())];
                for d in diffs {
                    v.push(format!("  {} {:?} a={} b={}", d.path, d.kind, d.a, d.b));
                }
                v
            }
            RecordDiff::PlistRawOnly { id, first_diff_offset, len_a, len_b } => {
                vec![format!(
                    "differs: {} plist structurally equal but bytes differ (first at {}, lenA={} lenB={})",
                    id, first_diff_offset, len_a, len_b
                )]
            }
            RecordDiff::GzipRawOnly { id, len_a, len_b } => {
                vec![format!(
                    "differs: {} gzip content equal but compressed bytes differ (lenA={} lenB={})",
                    id, len_a, len_b
                )]
            }
            RecordDiff::Bytes { id, len_a, len_b, differing_bytes, first_diff_offset } => {
                vec![format!(
                    "differs: {} lenA={} lenB={} differing_bytes={} first_diff_offset={}",
                    id, len_a, len_b, differing_bytes, first_diff_offset
                )]
            }
            RecordDiff::Text { id, first_diff_line, a, b } => {
                vec![format!("differs: {} text line {}: A={:?} B={:?}", id, first_diff_line, a, b)]
            }
        }
    }
}

/// Result of a save comparison.
#[derive(Debug)]
pub struct SaveDiffReport {
    pub compared_records: usize,
    pub identical: usize,
    pub masked: usize,
    pub per_db: BTreeMap<String, (usize, usize)>,
    pub diffs: Vec<RecordDiff>,
}
impl SaveDiffReport {
    /// No differences AND something was compared (a harness bug must never print "identical").
    pub fn is_clean(&self) -> bool {
        self.diffs.is_empty() && self.compared_records > 0
    }
    /// Text report: one headline per diff, then `identical records: N masked: M differing: K`.
    pub fn render(&self, max_detail: usize) -> String {
        let mut out = String::new();
        let mut n = 0usize;
        'o: for d in &self.diffs {
            for l in d.lines() {
                if n >= max_detail {
                    out.push_str(&format!("... output truncated at {} lines\n", max_detail));
                    break 'o;
                }
                out.push_str(&l);
                out.push('\n');
                n += 1;
            }
        }
        out.push_str(&format!(
            "identical records: {}  masked: {}  differing: {}\n",
            self.identical,
            self.masked,
            self.diffs.len()
        ));
        out
    }
    /// JSON report (hand-written).
    pub fn render_json(&self) -> String {
        let mut s = format!(
            "{{\"compared_records\":{},\"identical\":{},\"masked\":{},\"differing\":{},\"clean\":{},\"per_db\":{{",
            self.compared_records,
            self.identical,
            self.masked,
            self.diffs.len(),
            self.is_clean()
        );
        let mut first = true;
        for (k, (a, b)) in &self.per_db {
            if !first {
                s.push(',');
            }
            first = false;
            s.push_str(&format!("\"{}\":[{},{}]", json_escape(k), a, b));
        }
        s.push_str("},\"diffs\":[");
        for (i, d) in self.diffs.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let l = d.lines();
            s.push_str(&format!("{{\"record\":\"{}\",\"lines\":[", json_escape(&d.id().to_string())));
            s.push_str(&l.iter().map(|x| format!("\"{}\"", json_escape(x))).collect::<Vec<_>>().join(","));
            s.push_str("]}");
        }
        s.push_str("]}");
        s
    }
}

// ---------------------------------------------------------------------------------------------- loading

/// Read-only, lock-free open of an LMDB env directory (does not touch `lock.mdb`).
pub fn open_env_readonly(dir: &Path) -> Result<heed::Env> {
    use heed::{EnvFlags, EnvOpenOptions};
    // SAFETY: the env is opened read-only with locking disabled; the caller never opens a save a server is still writing.
    unsafe {
        EnvOpenOptions::new().max_dbs(16).map_size(1 << 30).flags(EnvFlags::READ_ONLY | EnvFlags::NO_LOCK).open(dir)
    }
    .map_err(|e| HarnessError::Lmdb(format!("{}: {}", dir.display(), e)))
}

fn lm<E: fmt::Display>(e: E) -> HarnessError {
    HarnessError::Lmdb(e.to_string())
}

/// Raw values of every record: the three LMDB envs (every named sub-db) + every regular file of the save dir.
pub fn load_records(save: &Path) -> Result<BTreeMap<RecordId, Vec<u8>>> {
    use heed::types::Bytes;
    // heed refuses concurrent opens of one path with differing handles inside a process: serialise open..close.
    static OPEN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = OPEN_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut out = BTreeMap::new();
    for e in ENVS {
        let p = save.join(e);
        if !p.is_dir() {
            continue;
        }
        let env = open_env_readonly(&p)?;
        {
            let rtxn = env.read_txn().map_err(lm)?;
            for root in env.open_database::<Bytes, Bytes>(&rtxn, None).map_err(lm)? {
                let mut names: Vec<Vec<u8>> = Vec::new();
                for kv in root.iter(&rtxn).map_err(lm)? {
                    let (k, _) = kv.map_err(lm)?;
                    names.push(k.to_vec());
                }
                for name in names {
                    let nm = String::from_utf8_lossy(&name).to_string();
                    let Some(db) = env.open_database::<Bytes, Bytes>(&rtxn, Some(&nm)).map_err(lm)? else { continue };
                    for kv in db.iter(&rtxn).map_err(lm)? {
                        let (k, v) = kv.map_err(lm)?;
                        out.insert(RecordId { env: e.to_string(), db: nm.clone(), key: k.to_vec() }, v.to_vec());
                    }
                }
            }
        }
        env.prepare_for_closing().wait();
    }
    for ent in std::fs::read_dir(save)? {
        let ent = ent?;
        if ent.file_type()?.is_file() {
            let name = ent.file_name().to_string_lossy().to_string();
            if name == "lock.mdb" || name == "data.mdb" {
                continue;
            }
            out.insert(
                RecordId { env: "file".into(), db: String::new(), key: name.into_bytes() },
                std::fs::read(ent.path())?,
            );
        }
    }
    Ok(out)
}

fn resolve_save(p: &Path) -> Result<PathBuf> {
    if p.join("worldv2").exists() {
        return Ok(p.to_path_buf());
    }
    let mut kids = Vec::new();
    for e in std::fs::read_dir(p)? {
        let e = e?;
        if e.file_type()?.is_dir() {
            kids.push(e.path());
        }
    }
    if kids.len() == 1 && kids[0].join("worldv2").exists() {
        return Ok(kids.remove(0));
    }
    Err(HarnessError::Usage(format!(
        "{}: not a save dir (no worldv2) and not a saves dir with exactly one child",
        p.display()
    )))
}

// ---------------------------------------------------------------------------------------------- classification

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Gzip,
    XmlPlist,
    Bplist,
    Raw,
}
fn classify(v: &[u8]) -> Kind {
    if v.starts_with(&[0x1f, 0x8b]) {
        Kind::Gzip
    } else if v.starts_with(b"<?xml") {
        Kind::XmlPlist
    } else if v.starts_with(b"bplist00") {
        Kind::Bplist
    } else {
        Kind::Raw
    }
}
fn kind_name(k: Kind, inner: Option<Kind>) -> &'static str {
    match (k, inner) {
        (Kind::Gzip, Some(Kind::XmlPlist)) => "gzip+xml-plist",
        (Kind::Gzip, Some(Kind::Bplist)) => "gzip+bplist",
        (Kind::Gzip, Some(Kind::Raw)) => "gzip+raw",
        (Kind::Gzip, _) => "gzip",
        (Kind::XmlPlist, _) => "xml-plist",
        (Kind::Bplist, _) => "bplist",
        (Kind::Raw, _) => "raw",
    }
}

// ---------------------------------------------------------------------------------------------- plist flattening

/// One flattened plist node.
#[derive(Clone, Debug, PartialEq)]
enum Node {
    Str(String),
    Int(i64),
    UInt(u64),
    Real(f64),
    Bool(bool),
    Date(f64),
    Data(Vec<u8>),
    Null,
    Dict,
    Array(usize),
}
impl Node {
    fn type_name(&self) -> &'static str {
        match self {
            Node::Str(_) => "string",
            Node::Int(_) => "integer",
            Node::UInt(_) => "unsigned",
            Node::Real(_) => "real",
            Node::Bool(_) => "bool",
            Node::Date(_) => "date",
            Node::Data(_) => "data",
            Node::Null => "null",
            Node::Dict => "dict",
            Node::Array(_) => "array",
        }
    }
    fn show(&self) -> String {
        match self {
            Node::Str(s) => format!("string:{:?}", s),
            Node::Int(i) => format!("integer:{}", i),
            Node::UInt(i) => format!("unsigned:{}", i),
            Node::Real(r) => format!("real:{}", r),
            Node::Bool(b) => format!("bool:{}", b),
            Node::Date(d) => format!("date:{}", d),
            Node::Data(d) => {
                let pre: String = d.iter().take(16).map(|b| format!("{:02x}", b)).collect();
                format!("data:{}B:{}{}", d.len(), pre, if d.len() > 16 { "..." } else { "" })
            }
            Node::Null => "null".into(),
            Node::Dict => "dict".into(),
            Node::Array(n) => format!("array[{}]", n),
        }
    }
}
fn flatten(v: &PlistValue, path: &str, out: &mut BTreeMap<String, Node>) {
    match v {
        PlistValue::String(s) => {
            out.insert(path.to_string(), Node::Str(s.s.clone()));
        }
        PlistValue::Int(i) => {
            out.insert(path.to_string(), Node::Int(*i));
        }
        PlistValue::UInt(i) => {
            out.insert(path.to_string(), Node::UInt(*i));
        }
        PlistValue::Real { value, .. } => {
            out.insert(path.to_string(), Node::Real(*value));
        }
        PlistValue::Bool(b) => {
            out.insert(path.to_string(), Node::Bool(*b));
        }
        PlistValue::Date(d) => {
            out.insert(path.to_string(), Node::Date(*d));
        }
        PlistValue::Data(d) => {
            out.insert(path.to_string(), Node::Data(d.bytes.clone()));
        }
        PlistValue::Null => {
            out.insert(path.to_string(), Node::Null);
        }
        PlistValue::Array(a) => {
            out.insert(path.to_string(), Node::Array(a.items.len()));
            for (i, it) in a.items.iter().enumerate() {
                flatten(it, &format!("{}/[{}]", path, i), out);
            }
        }
        PlistValue::Dict(d) => {
            out.insert(path.to_string(), Node::Dict);
            for (k, it) in d.map.iter() {
                flatten(it, &format!("{}/{}", path, k), out);
            }
        }
    }
}

struct Masks<'a> {
    all_dates: bool,
    plist: Vec<(&'a str, &'a str)>,
    records: Vec<&'a str>,
    tile: BTreeSet<usize>,
}
impl<'a> Masks<'a> {
    fn new(m: &'a [Mask]) -> Self {
        let mut r = Masks { all_dates: false, plist: vec![], records: vec![], tile: BTreeSet::new() };
        for x in m {
            match x {
                Mask::AllDates => r.all_dates = true,
                Mask::Plist { record, path } => r.plist.push((record, path)),
                Mask::Record(g) => r.records.push(g),
                Mask::TileBytes(v) => r.tile.extend(v.iter().copied()),
            }
        }
        r
    }
    fn plist_masked(&self, rec: &str, path: &str) -> bool {
        self.plist.iter().any(|(r, p)| glob_any(r, rec) && glob_path(p, path))
    }
}

fn first_diff(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b.iter()).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()))
}
fn count_diff(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b.iter()).filter(|(x, y)| x != y).count() + a.len().abs_diff(b.len())
}

fn compare_plist(id: &RecordId, a: &[u8], b: &[u8], m: &Masks) -> (Vec<PlistDiff>, bool) {
    let rec = id.to_string();
    let pa = property_list_from_data(a);
    let pb = property_list_from_data(b);
    let (pa, pb) = match (pa, pb) {
        (Some(x), Some(y)) => (x, y),
        (x, y) => {
            return (
                vec![PlistDiff {
                    path: String::new(),
                    kind: PlistDiffKind::TypeMismatch,
                    a: if x.is_some() { "parsed".into() } else { "PARSE FAILURE".into() },
                    b: if y.is_some() { "parsed".into() } else { "PARSE FAILURE".into() },
                }],
                false,
            )
        }
    };
    let mut fa = BTreeMap::new();
    let mut fb = BTreeMap::new();
    flatten(&pa, "", &mut fa);
    flatten(&pb, "", &mut fb);
    let mut diffs = Vec::new();
    let mut masked_hit = false;
    let keys: BTreeSet<&String> = fa.keys().chain(fb.keys()).collect();
    for k in keys {
        let disp = if k.is_empty() { "/".to_string() } else { k.clone() };
        if m.plist_masked(&rec, k) {
            if fa.get(k) != fb.get(k) {
                masked_hit = true;
            }
            continue;
        }
        match (fa.get(k), fb.get(k)) {
            (Some(x), None) => {
                diffs.push(PlistDiff { path: disp, kind: PlistDiffKind::OnlyInA, a: x.show(), b: String::new() })
            }
            (None, y) => diffs.extend(y.map(|y| PlistDiff {
                path: disp,
                kind: PlistDiffKind::OnlyInB,
                a: String::new(),
                b: y.show(),
            })),
            (Some(Node::Date(_)), Some(Node::Date(_))) if m.all_dates => {
                if fa.get(k) != fb.get(k) {
                    masked_hit = true;
                }
            }
            (Some(Node::Array(x)), Some(Node::Array(y))) => {
                if x != y {
                    diffs.push(PlistDiff {
                        path: disp,
                        kind: PlistDiffKind::LengthMismatch,
                        a: x.to_string(),
                        b: y.to_string(),
                    });
                }
            }
            (Some(x), Some(y)) => {
                if x.type_name() != y.type_name() {
                    diffs.push(PlistDiff { path: disp, kind: PlistDiffKind::TypeMismatch, a: x.show(), b: y.show() });
                } else {
                    let same = match (x, y) {
                        (Node::Real(p), Node::Real(q)) => p.to_bits() == q.to_bits(),
                        _ => x == y,
                    };
                    if !same {
                        diffs.push(PlistDiff {
                            path: disp,
                            kind: PlistDiffKind::ValueMismatch,
                            a: x.show(),
                            b: y.show(),
                        });
                    }
                }
            }
        }
    }
    (diffs, masked_hit)
}

fn parse_block_key(key: &str) -> Option<(i64, i64)> {
    let (x, y) = key.split_once('_')?;
    Some((x.parse().ok()?, y.parse().ok()?))
}

fn le_value(b: &[u8]) -> u64 {
    let mut v = 0u64;
    for (i, x) in b.iter().take(8).enumerate() {
        v |= (*x as u64) << (8 * i);
    }
    v
}

fn compare_block(id: &RecordId, a: &[u8], b: &[u8], m: &Masks, opts: &SaveDiffOptions) -> Option<RecordDiff> {
    let trailer = |x: &[u8]| {
        (
            x[TILES_BYTES],
            u32::from_le_bytes([x[TILES_BYTES + 1], x[TILES_BYTES + 2], x[TILES_BYTES + 3], x[TILES_BYTES + 4]]),
        )
    };
    let (ta, tb) = (trailer(a), trailer(b));
    let (mx, my) = parse_block_key(&id.key_string()).unwrap_or((0, 0));
    let mut tiles = 0usize;
    let mut bytes = 0usize;
    let mut sample = Vec::new();
    for t in 0..1024 {
        let (sa, sb) = (&a[t * TILE_SIZE..(t + 1) * TILE_SIZE], &b[t * TILE_SIZE..(t + 1) * TILE_SIZE]);
        if sa == sb {
            continue;
        }
        let mut any = false;
        for (name, off, size) in TILE_FIELDS.iter() {
            let range = *off..off + size;
            let diff_bytes = range.clone().filter(|i| sa[*i] != sb[*i] && !m.tile.contains(i)).count();
            if diff_bytes == 0 {
                continue;
            }
            any = true;
            bytes += diff_bytes;
            if sample.len() < opts.max_tile_diffs_per_block {
                sample.push(TileDiff {
                    tile_index: t,
                    local_x: t % 32,
                    local_y: t / 32,
                    world_x: mx * 32 + (t % 32) as i64,
                    world_y: my * 32 + (t / 32) as i64,
                    field: name,
                    a: le_value(&sa[range.clone()]),
                    b: le_value(&sb[range]),
                });
            }
        }
        if any {
            tiles += 1;
        }
    }
    if tiles == 0 && ta == tb {
        return None;
    }
    Some(RecordDiff::Block {
        id: id.clone(),
        trailer_a: ta,
        trailer_b: tb,
        differing_tiles: tiles,
        differing_bytes: bytes,
        sample,
    })
}

/// Compare two inner (already unwrapped) values. `None` = equal under masks. Second tuple member: a mask changed the outcome.
fn compare_inner(
    id: &RecordId,
    a: &[u8],
    b: &[u8],
    ka: Kind,
    kb: Kind,
    m: &Masks,
    opts: &SaveDiffOptions,
) -> (Option<RecordDiff>, bool) {
    if ka != kb {
        return (
            Some(RecordDiff::KindMismatch { id: id.clone(), a: kind_name(ka, None), b: kind_name(kb, None) }),
            false,
        );
    }
    if id.env == "world_db" && id.db == "blocks" && a.len() == BLOCK_BYTES && b.len() == BLOCK_BYTES {
        let r = compare_block(id, a, b, m, opts);
        let hit = r.is_none() && a != b;
        return (r, hit);
    }
    match ka {
        Kind::XmlPlist => {
            let (diffs, hit) = compare_plist(id, a, b, m);
            if !diffs.is_empty() {
                return (Some(RecordDiff::Plist { id: id.clone(), diffs }), hit);
            }
            if a != b && !hit {
                return (
                    Some(RecordDiff::PlistRawOnly {
                        id: id.clone(),
                        first_diff_offset: first_diff(a, b),
                        len_a: a.len(),
                        len_b: b.len(),
                    }),
                    false,
                );
            }
            (None, hit)
        }
        _ => {
            if a == b {
                return (None, false);
            }
            let name = id.key_string();
            if id.env == "file" && name.ends_with(".txt") {
                let (sa, sb) = (String::from_utf8_lossy(a).to_string(), String::from_utf8_lossy(b).to_string());
                let (la, lb): (Vec<&str>, Vec<&str>) = (sa.split('\n').collect(), sb.split('\n').collect());
                let n = la.len().max(lb.len());
                for i in 0..n {
                    if la.get(i) != lb.get(i) {
                        return (
                            Some(RecordDiff::Text {
                                id: id.clone(),
                                first_diff_line: i + 1,
                                a: la.get(i).unwrap_or(&"<missing>").to_string(),
                                b: lb.get(i).unwrap_or(&"<missing>").to_string(),
                            }),
                            false,
                        );
                    }
                }
            }
            (
                Some(RecordDiff::Bytes {
                    id: id.clone(),
                    len_a: a.len(),
                    len_b: b.len(),
                    differing_bytes: count_diff(a, b),
                    first_diff_offset: first_diff(a, b),
                }),
                false,
            )
        }
    }
}

fn unwrap_gzip(v: &[u8]) -> (Kind, Option<Vec<u8>>) {
    let k = classify(v);
    if k == Kind::Gzip {
        (k, gzip_inflate(v))
    } else {
        (k, None)
    }
}

/// Compare two records. `Ok(None)` = same (possibly under masks).
fn compare_record(id: &RecordId, a: &[u8], b: &[u8], m: &Masks, opts: &SaveDiffOptions) -> Option<RecordDiff> {
    if a == b {
        return None;
    }
    let (ka, ia) = unwrap_gzip(a);
    let (kb, ib) = unwrap_gzip(b);
    match (ka == Kind::Gzip, kb == Kind::Gzip) {
        (true, true) => {
            let (da, db) = match (ia, ib) {
                (Some(x), Some(y)) => (x, y),
                _ => {
                    // corrupt gzip on at least one side: byte compare
                    return Some(RecordDiff::Bytes {
                        id: id.clone(),
                        len_a: a.len(),
                        len_b: b.len(),
                        differing_bytes: count_diff(a, b),
                        first_diff_offset: first_diff(a, b),
                    });
                }
            };
            let (ika, ikb) = (classify(&da), classify(&db));
            let (r, _hit) = compare_inner(id, &da, &db, ika, ikb, m, opts);
            if r.is_some() {
                return r;
            }
            if opts.strict_gzip && da == db {
                return Some(RecordDiff::GzipRawOnly { id: id.clone(), len_a: a.len(), len_b: b.len() });
            }
            None
        }
        (false, false) => compare_inner(id, a, b, ka, kb, m, opts).0,
        _ => {
            let ik = |_k: Kind, i: &Option<Vec<u8>>| i.as_ref().map(|x| classify(x));
            Some(RecordDiff::KindMismatch {
                id: id.clone(),
                a: kind_name(ka, ik(ka, &ia)),
                b: kind_name(kb, ik(kb, &ib)),
            })
        }
    }
}

fn db_key(id: &RecordId) -> String {
    if id.env == "file" {
        "file".to_string()
    } else {
        format!("{}/{}", id.env, id.db)
    }
}

/// Compare two saves (dirs containing `worldv2`, or `saves` dirs with a single child).
pub fn diff_saves(a: &Path, b: &Path, o: &SaveDiffOptions) -> Result<SaveDiffReport> {
    let (pa, pb) = (resolve_save(a)?, resolve_save(b)?);
    let (ra, rb) = (load_records(&pa)?, load_records(&pb)?);
    Ok(diff_record_sets(&ra, &rb, o))
}

/// Compare two already loaded record sets.
pub fn diff_record_sets(
    ra: &BTreeMap<RecordId, Vec<u8>>,
    rb: &BTreeMap<RecordId, Vec<u8>>,
    o: &SaveDiffOptions,
) -> SaveDiffReport {
    let m = Masks::new(&o.masks);
    let mut rep =
        SaveDiffReport { compared_records: 0, identical: 0, masked: 0, per_db: BTreeMap::new(), diffs: vec![] };
    for id in ra.keys() {
        rep.per_db.entry(db_key(id)).or_insert((0, 0)).0 += 1;
    }
    for id in rb.keys() {
        rep.per_db.entry(db_key(id)).or_insert((0, 0)).1 += 1;
    }
    let ids: BTreeSet<&RecordId> = ra.keys().chain(rb.keys()).collect();
    rep.compared_records = ids.len();
    for id in ids {
        let s = id.to_string();
        if m.records.iter().any(|g| glob_any(g, &s)) {
            rep.masked += 1;
            continue;
        }
        match (ra.get(id), rb.get(id)) {
            (Some(x), Some(y)) => match compare_record(id, x, y, &m, o) {
                None => rep.identical += 1,
                Some(d) => rep.diffs.push(d),
            },
            (only_a, _) => rep.diffs.push(if only_a.is_some() {
                RecordDiff::OnlyInA(id.clone())
            } else {
                RecordDiff::OnlyInB(id.clone())
            }),
        }
    }
    rep
}

// ---------------------------------------------------------------------------------------------- other commands

/// ` regzip`: deflate(inflate(v)) == v for every `blocks` record. Returns (ok, mismatched).
pub fn regzip_check(save: &Path) -> Result<(usize, Vec<RecordId>)> {
    let recs = load_records(&resolve_save(save)?)?;
    let mut ok = 0;
    let mut bad = Vec::new();
    for (id, v) in recs.iter().filter(|(i, _)| i.env == "world_db" && i.db == "blocks") {
        match gzip_inflate(v) {
            Some(raw) if gzip_deflate(&raw) == *v => ok += 1,
            _ => bad.push(id.clone()),
        }
    }
    Ok((ok, bad))
}

/// ` block`: 32x32 grid of one tile field, top row first.
pub fn render_block_grid(save: &Path, x: u32, y: u32, field: &str) -> Result<String> {
    let (_, off, size) = *TILE_FIELDS
        .iter()
        .find(|(n, _, _)| *n == field)
        .ok_or_else(|| HarnessError::Usage(format!("unknown tile field '{}'", field)))?;
    let recs = load_records(&resolve_save(save)?)?;
    let id = RecordId { env: "world_db".into(), db: "blocks".into(), key: format!("{}_{}", x, y).into_bytes() };
    let v = match recs.get(&id) {
        Some(v) => v,
        None => return Ok("no such block\n".into()),
    };
    let raw = gzip_inflate(v).ok_or_else(|| HarnessError::Parse("block is not valid gzip".into()))?;
    if raw.len() != BLOCK_BYTES {
        return Err(HarnessError::Parse(format!("block length {} != {}", raw.len(), BLOCK_BYTES)));
    }
    let upd =
        u32::from_le_bytes([raw[TILES_BYTES + 1], raw[TILES_BYTES + 2], raw[TILES_BYTES + 3], raw[TILES_BYTES + 4]]);
    let mut s = format!(
        "block {}_{}: version={} updateIndex={} field={} (top row printed first)\n",
        x, y, raw[TILES_BYTES], upd, field
    );
    for ly in (0..32).rev() {
        let row: Vec<String> = (0..32)
            .map(|lx| {
                let o = (ly * 32 + lx) * TILE_SIZE + off;
                format!("{:3}", le_value(&raw[o..o + size]))
            })
            .collect();
        s.push_str(&row.join(" "));
        s.push('\n');
    }
    Ok(s)
}

/// ` summary`.
pub fn summary(save: &Path) -> Result<String> {
    let save = resolve_save(save)?;
    let recs = load_records(&save)?;
    let mut s = String::new();
    for (id, v) in recs.iter().filter(|(i, _)| i.env == "file") {
        let name = id.key_string();
        s.push_str(&format!("file {}: {} bytes\n", name, v.len()));
        if name == "worldv2" {
            if let Some(p) = property_list_from_data(v) {
                let mut f = BTreeMap::new();
                flatten(&p, "", &mut f);
                for (k, n) in f {
                    if !matches!(n, Node::Dict | Node::Array(_)) {
                        s.push_str(&format!("    {} = {}\n", k, n.show()));
                    }
                }
            }
        } else if name.ends_with(".txt") {
            let t: String = String::from_utf8_lossy(v).chars().take(200).collect();
            s.push_str(&format!("    {}\n", t.replace('\n', "\\n")));
        }
    }
    for e in ENVS {
        let p = save.join(e);
        if !p.is_dir() {
            continue;
        }
        let size = std::fs::metadata(p.join("data.mdb")).map(|m| m.len()).unwrap_or(0);
        s.push_str(&format!("env {}: data.mdb={} bytes\n", e, size));
        let dbs: BTreeSet<&str> = recs.keys().filter(|i| i.env == e).map(|i| i.db.as_str()).collect();
        for db in dbs {
            let rows: Vec<(&RecordId, &Vec<u8>)> = recs.iter().filter(|(i, _)| i.env == e && i.db == db).collect();
            let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
            for (_, v) in &rows {
                *kinds.entry(kind_name(classify(v), None)).or_insert(0) += 1;
            }
            s.push_str(&format!("  db {:?}: {} entries, value kinds {:?}\n", db, rows.len(), kinds));
            for (id, v) in rows.iter().take(5) {
                let pre: String = v.iter().take(12).map(|b| format!("{:02x}", b)).collect();
                s.push_str(&format!("    {}: {} bytes {}\n", id.key_string(), v.len(), pre));
            }
        }
    }
    Ok(s)
}

/// ` dump`: every record to a file, blocks gunzipped to `.bin`, plists to `.plist`.
pub fn dump(save: &Path, out: &Path) -> Result<()> {
    let recs = load_records(&resolve_save(save)?)?;
    for (id, v) in recs.iter().filter(|(i, _)| i.env != "file") {
        let dir = out.join(&id.env).join(&id.db);
        std::fs::create_dir_all(&dir)?;
        let mut name = id.key_string().replace('/', "__");
        let mut data = v.clone();
        match classify(v) {
            Kind::Gzip => {
                if let Some(x) = gzip_inflate(v) {
                    data = x;
                    name.push_str(".bin");
                }
            }
            Kind::XmlPlist => name.push_str(".plist"),
            _ => {}
        }
        std::fs::write(dir.join(name), data)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_fields_cover_64() {
        let mut n = 0;
        for (_, o, s) in TILE_FIELDS.iter() {
            assert_eq!(*o, n);
            n += s;
        }
        assert_eq!(n, 64);
        assert_eq!(tile_field_at(13), "pad@13");
        assert_eq!(tile_field_at(21), "artificialHeat");
        assert_eq!(tile_field_at(63), "pad@48");
    }

    /// S8.
    #[test]
    fn s8_mask_roundtrip() {
        for s in ["dates", "plist:world_db/main/worldv2#/hostPort", "record:world_db/blocks/*", "tile:13,23,38"] {
            let m = Mask::parse(s).unwrap();
            assert_eq!(m.to_string(), s);
        }
        assert!(Mask::parse("bogus").is_err());
        assert!(Mask::parse("tile:64").is_err());
    }

    #[test]
    fn synthetic_plist_and_blocks() {
        let id = RecordId { env: "world_db".into(), db: "dw".into(), key: b"1_2/3".to_vec() };
        let m = [Mask::AllDates];
        let masks = Masks::new(&m);
        let o = SaveDiffOptions::default();
        let hdr = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//GNUstep//DTD plist 0.9//EN\" \"http://www.gnustep.org/plist-0_9.xml\">\n<plist version=\"0.9\">\n";
        let mk = |body: &str| format!("{}<dict>\n    {}\n</dict>\n</plist>", hdr, body).into_bytes();
        let a = mk("<key>x</key>\n    <integer>255</integer>");
        let b = mk("<key>x</key>\n    <real>255</real>");
        let Some(RecordDiff::Plist { diffs, .. }) = compare_record(&id, &a, &b, &masks, &o) else { panic!() };
        assert_eq!(diffs[0].kind, PlistDiffKind::TypeMismatch);
        let c = mk("<key>x</key>\n        <integer>255</integer>");
        assert!(matches!(compare_record(&id, &a, &c, &masks, &o), Some(RecordDiff::PlistRawOnly { .. })));
        assert!(compare_record(&id, &a, &a, &masks, &o).is_none());
        // dates
        let d1 = mk("<key>d</key>\n    <date>2026-01-01T00:00:00Z</date>");
        let d2 = mk("<key>d</key>\n    <date>2026-02-01T00:00:00Z</date>");
        assert!(compare_record(&id, &d1, &d2, &masks, &o).is_none());
        assert!(compare_record(&id, &d1, &d2, &Masks::new(&[]), &o).is_some());

        // blocks
        let bid = RecordId { env: "world_db".into(), db: "blocks".into(), key: b"198_18".to_vec() };
        let mut ba = vec![0u8; BLOCK_BYTES];
        let mut bb = ba.clone();
        let t = 2 * 32 + 14;
        ba[t * 64] = 25;
        bb[t * 64] = 6;
        let (ga, gb) = (gzip_deflate(&ba), gzip_deflate(&bb));
        let d = compare_record(&bid, &ga, &gb, &masks, &o);
        let Some(RecordDiff::Block { differing_tiles: 1, sample, .. }) = d else { panic!() };
        let t = &sample[0];
        assert_eq!(
            (t.local_x, t.local_y, t.world_x, t.world_y, t.field, t.a, t.b),
            (14, 2, 6350, 578, "typeIndex", 25, 6)
        );
        // level-9 gzip -> GzipRawOnly, ignored when not strict
        let mut f = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        std::io::Write::write_all(&mut f, &ba).unwrap();
        let g9 = f.finish().unwrap();
        assert_ne!(g9, ga);
        assert!(matches!(compare_record(&bid, &ga, &g9, &masks, &o), Some(RecordDiff::GzipRawOnly { .. })));
        let lax = SaveDiffOptions { strict_gzip: false, ..Default::default() };
        assert!(compare_record(&bid, &ga, &g9, &masks, &lax).is_none());
        // tile mask
        let tmask = [Mask::TileBytes(vec![0])];
        let tm = Masks::new(&tmask);
        assert!(compare_record(&bid, &ga, &gb, &tm, &o).is_none());
        // text
        let tid = RecordId { env: "file".into(), db: String::new(), key: b"adminlist.txt".to_vec() };
        assert!(matches!(
            compare_record(&tid, b"hdr\n", b"HDR\n", &masks, &o),
            Some(RecordDiff::Text { first_diff_line: 1, .. })
        ));
        // empty sets are never "clean"
        let rep = diff_record_sets(&BTreeMap::new(), &BTreeMap::new(), &o);
        assert!(!rep.is_clean());
    }
}

#[cfg(test)]
#[path = "save_diff_golden.rs"]
mod golden;

#[cfg(test)]
mod unit {
    use super::*;
    use blockheads_server::foundation::gsdict::GsDict;
    use blockheads_server::plist::xml_write::write_xml;
    use heed::types::Bytes;

    type Rows<'a> = &'a [(&'a [u8], &'a [u8])];

    /// Writes an LMDB env with the given named databases.
    fn put_env(dir: &Path, dbs: &[(&str, Rows)]) {
        std::fs::create_dir_all(dir).unwrap();
        // SAFETY: private temp directory, nothing else opens it.
        let env = unsafe { heed::EnvOpenOptions::new().max_dbs(16).map_size(1 << 26).open(dir) }.unwrap();
        let mut w = env.write_txn().unwrap();
        for (name, rows) in dbs {
            let d = env.create_database::<Bytes, Bytes>(&mut w, Some(name)).unwrap();
            for (k, v) in rows.iter() {
                d.put(&mut w, k, v).unwrap();
            }
        }
        w.commit().unwrap();
        env.prepare_for_closing().wait();
    }

    fn dict(pairs: Vec<(&str, PlistValue)>) -> PlistValue {
        let mut m = GsDict::new();
        for (k, v) in pairs {
            m.insert(k.to_string(), v);
        }
        PlistValue::dict(m)
    }
    fn xml(v: PlistValue) -> Vec<u8> {
        write_xml(&v)
    }
    fn block(first_type: u8, ver: u8, upd: u32) -> Vec<u8> {
        let mut b = vec![0u8; BLOCK_BYTES];
        b[0] = first_type;
        b[TILES_BYTES] = ver;
        b[TILES_BYTES + 1..].copy_from_slice(&upd.to_le_bytes());
        b
    }
    fn id(env: &str, db: &str, key: &str) -> RecordId {
        RecordId { env: env.into(), db: db.into(), key: key.as_bytes().to_vec() }
    }

    #[test]
    fn ids_masks_and_options() {
        let bad = RecordId { env: "e".into(), db: "d".into(), key: vec![b'a', 0xff, b'b', 0xc3] };
        assert_eq!(bad.key_string(), "a\\xffb\\xc3");
        assert_eq!(bad.to_string(), "e/d/a\\xffb\\xc3");
        assert_eq!(tile_field_at(0), "typeIndex");
        assert_eq!(tile_field_at(64), "?");
        for s in ["tile:", "plist:a#b", "record:x"] {
            assert_eq!(Mask::parse(s).unwrap().to_string(), s);
        }
        for bad in ["plist:nohash", "tile:x", "tile:64", "nope"] {
            assert!(matches!(Mask::parse(bad), Err(HarnessError::Usage(_))), "{bad}");
        }
        let p = SaveDiffOptions::parity_default();
        assert!(p.strict_gzip && p.masks.len() == 2 && p.max_tile_diffs_per_block == 8 && p.max_detail == 200);
    }

    #[test]
    fn diff_lines_and_reports() {
        let i = id("world_db", "blocks", "1_2");
        let diffs = vec![
            RecordDiff::OnlyInA(id("e", "d", "a")),
            RecordDiff::OnlyInB(id("e", "d", "b")),
            RecordDiff::KindMismatch { id: i.clone(), a: "raw", b: "gzip" },
            RecordDiff::Block {
                id: i.clone(),
                trailer_a: (1, 2),
                trailer_b: (3, 4),
                differing_tiles: 1,
                differing_bytes: 2,
                sample: vec![TileDiff {
                    tile_index: 1,
                    local_x: 1,
                    local_y: 0,
                    world_x: 33,
                    world_y: 64,
                    field: "light",
                    a: 5,
                    b: 6,
                }],
            },
            RecordDiff::Plist {
                id: i.clone(),
                diffs: vec![PlistDiff {
                    path: "/x".into(),
                    kind: PlistDiffKind::OnlyInA,
                    a: "1".into(),
                    b: String::new(),
                }],
            },
            RecordDiff::PlistRawOnly { id: i.clone(), first_diff_offset: 9, len_a: 10, len_b: 11 },
            RecordDiff::GzipRawOnly { id: i.clone(), len_a: 1, len_b: 2 },
            RecordDiff::Bytes { id: i.clone(), len_a: 1, len_b: 2, differing_bytes: 1, first_diff_offset: 0 },
            RecordDiff::Text { id: i.clone(), first_diff_line: 3, a: "x".into(), b: "y".into() },
        ];
        for d in &diffs {
            assert!(!d.id().env.is_empty());
            assert!(d.lines()[0].contains("e/d/") || d.lines()[0].contains("world_db/blocks/1_2"));
        }
        assert!(diffs[3].lines()[1].contains("blocks 1_2 tile(1,0) world(33,64) light a=5 b=6"));
        let mut per_db = BTreeMap::new();
        per_db.insert("world_db/blocks".to_string(), (2, 3));
        per_db.insert("file".to_string(), (1, 1));
        let rep = SaveDiffReport { compared_records: 5, identical: 3, masked: 1, per_db, diffs };
        assert!(!rep.is_clean());
        let text = rep.render(1000);
        assert!(text.contains("only in A: e/d/a") && text.ends_with("identical records: 3  masked: 1  differing: 9\n"));
        let short = rep.render(2);
        assert!(short.contains("... output truncated at 2 lines"));
        let json = rep.render_json();
        assert!(json.starts_with("{\"compared_records\":5,\"identical\":3,\"masked\":1,\"differing\":9,\"clean\":false,\"per_db\":{\"file\":[1,1],\"world_db/blocks\":[2,3]},\"diffs\":[{\"record\":\"e/d/a\""));
        let empty =
            SaveDiffReport { compared_records: 0, identical: 0, masked: 0, per_db: BTreeMap::new(), diffs: vec![] };
        assert!(!empty.is_clean() && empty.render_json().contains("\"per_db\":{}"));
        let clean = SaveDiffReport { compared_records: 1, ..empty };
        assert!(clean.is_clean());
    }

    #[test]
    fn plist_comparison_covers_every_node_kind() {
        let m = [Mask::AllDates, Mask::Plist { record: "e/d/k".into(), path: "/masked".into() }];
        let masks = Masks::new(&m);
        let rec = id("e", "d", "k");
        let full = |n: i64, d: f64, extra: bool| {
            let mut pairs = vec![
                ("s", PlistValue::string("a")),
                ("i", PlistValue::int(n)),
                ("u", PlistValue::UInt(u64::MAX)),
                ("r", PlistValue::real_f64(1.5)),
                ("b", PlistValue::Bool(true)),
                ("d", PlistValue::date(d)),
                ("x", PlistValue::data((0..20).collect::<Vec<u8>>())),
                ("l", PlistValue::array(vec![PlistValue::int(1)])),
                ("masked", PlistValue::int(n)),
                ("o", dict(vec![])),
            ];
            if extra {
                pairs.push(("only", PlistValue::string("v")));
            }
            xml(dict(pairs))
        };
        let (diffs, hit) = compare_plist(&rec, &full(1, 0.0, false), &full(1, 0.0, false), &masks);
        assert!(diffs.is_empty() && !hit);
        let (diffs, hit) = compare_plist(&rec, &full(1, 0.0, false), &full(2, 9.0, false), &masks);
        assert_eq!(diffs.len(), 1);
        assert_eq!((diffs[0].path.as_str(), diffs[0].kind), ("/i", PlistDiffKind::ValueMismatch));
        assert!(hit);
        let (diffs, _) = compare_plist(&rec, &full(1, 0.0, true), &full(1, 0.0, false), &masks);
        assert_eq!((diffs[0].kind, diffs[0].a.as_str()), (PlistDiffKind::OnlyInA, "string:\"v\""));
        let (diffs, _) = compare_plist(&rec, &full(1, 0.0, false), &full(1, 0.0, true), &masks);
        assert_eq!(diffs[0].kind, PlistDiffKind::OnlyInB);
        // type and array length mismatches, reals compared bit for bit
        let a = xml(dict(vec![
            ("v", PlistValue::int(1)),
            ("l", PlistValue::array(vec![])),
            ("r", PlistValue::real_f64(0.0)),
        ]));
        let b = xml(dict(vec![
            ("v", PlistValue::string("1")),
            ("l", PlistValue::array(vec![PlistValue::Null])),
            ("r", PlistValue::real_f64(-0.0)),
        ]));
        let (diffs, _) = compare_plist(&rec, &a, &b, &masks);
        let kinds: Vec<_> = diffs.iter().map(|d| (d.path.as_str(), d.kind)).collect();
        assert!(
            kinds.contains(&("/v", PlistDiffKind::TypeMismatch))
                && kinds.contains(&("/l", PlistDiffKind::LengthMismatch))
        );
        assert!(
            kinds.contains(&("/r", PlistDiffKind::ValueMismatch))
                && kinds.contains(&("/l/[0]", PlistDiffKind::OnlyInB))
        );
        // a root that does not parse
        let (diffs, _) = compare_plist(&rec, b"<?xml junk", &a, &masks);
        assert_eq!((diffs[0].a.as_str(), diffs[0].b.as_str()), ("PARSE FAILURE", "parsed"));
        let (diffs, _) = compare_plist(&rec, &a, b"<?xml junk", &masks);
        assert_eq!((diffs[0].a.as_str(), diffs[0].b.as_str()), ("parsed", "PARSE FAILURE"));
        // every node can be shown and named
        let mut flat = BTreeMap::new();
        flatten(
            &PlistValue::array(vec![
                PlistValue::UInt(7),
                PlistValue::real_f64(0.5),
                PlistValue::data(vec![1; 20]),
                PlistValue::Null,
            ]),
            "",
            &mut flat,
        );
        let shown: Vec<String> = flat.values().map(Node::show).collect();
        assert!(shown.contains(&"unsigned:7".to_string()) && shown.contains(&"real:0.5".to_string()));
        assert!(
            shown.iter().any(|s| s.starts_with("data:20B:0101") && s.ends_with("..."))
                && shown.contains(&"array[4]".to_string())
        );
        let types: Vec<&str> = flat.values().map(Node::type_name).collect();
        assert!(
            types.contains(&"unsigned")
                && types.contains(&"real")
                && types.contains(&"data")
                && types.contains(&"null")
        );
        assert_eq!((Node::Dict.show(), Node::Dict.type_name()), ("dict".to_string(), "dict"));
        assert_eq!(Node::Int(1).show(), "integer:1");
        assert_eq!(Node::Str("s".into()).show(), "string:\"s\"");
        assert_eq!(
            (Node::Bool(true).type_name(), Node::Date(1.0).type_name(), Node::Str(String::new()).type_name()),
            ("bool", "date", "string")
        );
        assert_eq!(Node::Date(2.0).show(), "date:2");
        assert_eq!(Node::Bool(false).show(), "bool:false");
        assert_eq!(Node::UInt(1).type_name(), "unsigned");
        assert_eq!(Node::Int(1).type_name(), "integer");
    }

    #[test]
    fn record_comparison_by_kind() {
        let masks = Masks::new(&[]);
        let o = SaveDiffOptions::default();
        let k = id("world_db", "main", "worldv2");
        let gz = |b: &[u8]| gzip_deflate(b);
        let plist_a = xml(dict(vec![("a", PlistValue::int(1))]));
        let plist_b = xml(dict(vec![("a", PlistValue::int(2))]));
        // gzip against raw, with and without a recognisable inner kind
        let m = compare_record(&k, &gz(&plist_a), b"raw bytes", &masks, &o).unwrap();
        assert!(matches!(m, RecordDiff::KindMismatch { a: "gzip+xml-plist", b: "raw", .. }));
        let m = compare_record(&k, b"bplist00x", &gz(b"\x00\x01plain"), &masks, &o).unwrap();
        assert!(matches!(m, RecordDiff::KindMismatch { a: "bplist", b: "gzip+raw", .. }));
        let m = compare_record(&k, b"<?xml a", &gz(b"bplist00xx"), &masks, &o).unwrap();
        assert!(matches!(m, RecordDiff::KindMismatch { a: "xml-plist", b: "gzip+bplist", .. }));
        // corrupt gzip on one side
        let mut corrupt = gz(&plist_a);
        let n = corrupt.len();
        corrupt[n - 6] ^= 0xff;
        assert!(matches!(compare_record(&k, &gz(&plist_a), &corrupt, &masks, &o), Some(RecordDiff::Bytes { .. })));
        // inner kinds that differ, plist diffs through gzip, equal content under lax gzip
        assert!(matches!(
            compare_record(&k, &gz(&plist_a), &gz(b"bplist00zz"), &masks, &o),
            Some(RecordDiff::KindMismatch { .. })
        ));
        assert!(matches!(compare_record(&k, &gz(&plist_a), &gz(&plist_b), &masks, &o), Some(RecordDiff::Plist { .. })));
        assert!(compare_record(&k, &gz(&plist_a), &gz(&plist_a), &masks, &o).is_none());
        // same content, different compression: only reported when the compressed bytes must match too
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        std::io::Write::write_all(&mut enc, &[7u8; 4000]).unwrap();
        let (best, normal) = (enc.finish().unwrap(), gz(&[7u8; 4000]));
        assert_ne!(best, normal);
        assert!(matches!(compare_record(&k, &best, &normal, &masks, &o), Some(RecordDiff::GzipRawOnly { .. })));
        let lax = SaveDiffOptions { strict_gzip: false, ..Default::default() };
        assert!(compare_record(&k, &best, &normal, &masks, &lax).is_none());
        // raw values: bytes, and text files by line
        let r = compare_record(&id("e", "d", "raw"), b"abcd", b"abXdY", &masks, &o).unwrap();
        assert!(matches!(r, RecordDiff::Bytes { differing_bytes: 2, first_diff_offset: 2, .. }));
        let t = id("file", "", "admin.txt");
        let d = compare_record(&t, b"one\ntwo\nthree", b"one\ntwo", &masks, &o).unwrap();
        let RecordDiff::Text { first_diff_line, a, b, .. } = d else { panic!() };
        assert_eq!((first_diff_line, a.as_str(), b.as_str()), (3, "three", "<missing>"));
        let RecordDiff::Text { a, .. } = compare_record(&t, b"one", b"one\ntwo", &masks, &o).unwrap() else { panic!() };
        assert_eq!(a, "<missing>");
        // different bytes that read as the same text
        assert!(matches!(compare_record(&t, &[0xff, b'x'], &[0xfe, b'x'], &masks, &o), Some(RecordDiff::Bytes { .. })));
        assert_eq!((first_diff(b"abc", b"abc"), count_diff(b"ab", b"abcd")), (3, 2));
        assert_eq!(parse_block_key("12_-3"), Some((12, -3)));
        assert_eq!((parse_block_key("12"), parse_block_key("a_1")), (None, None));
        assert_eq!(le_value(&[1, 2]), 0x0201);
    }

    #[test]
    fn block_comparison() {
        let o = SaveDiffOptions { max_tile_diffs_per_block: 1, ..Default::default() };
        let bid = id("world_db", "blocks", "bad-key");
        let mut a = block(1, 1, 1);
        let mut b = block(1, 1, 1);
        a[64 * 5] = 9;
        a[64 * 5 + 1] = 9;
        a[64 * 6 + 20] = 4;
        let gm = Masks::new(&[]);
        let d = compare_block(&bid, &a, &b, &gm, &o).unwrap();
        let RecordDiff::Block { differing_tiles, differing_bytes, sample, .. } = d else { panic!() };
        assert_eq!((differing_tiles, differing_bytes, sample.len()), (2, 3, 1));
        assert_eq!((sample[0].world_x, sample[0].world_y), (5, 0));
        let tm = [Mask::TileBytes(vec![0, 1, 20, 21])];
        assert!(compare_block(&bid, &a, &b, &Masks::new(&tm), &o).is_none());
        b[TILES_BYTES + 3] = 5;
        assert!(matches!(
            compare_block(&bid, &a, &b, &Masks::new(&tm), &o),
            Some(RecordDiff::Block { differing_tiles: 0, .. })
        ));
        // whole-record path: raw blocks of the right size are compared tile by tile
        let r = compare_record(&id("world_db", "other", "1_1"), &a, &block(7, 1, 1), &gm, &o).unwrap();
        assert!(matches!(r, RecordDiff::Bytes { .. }), "outside world_db/blocks: plain bytes");
        let real = id("world_db", "blocks", "198_18");
        assert!(matches!(compare_record(&real, &a, &block(7, 1, 1), &gm, &o), Some(RecordDiff::Block { .. })));
        let same_under_mask = Masks::new(&tm);
        assert!(compare_record(&real, &a, &block(1, 1, 1), &same_under_mask, &o).is_none());
    }

    fn two_saves() -> (super::super::TempDir, PathBuf, PathBuf) {
        let tmp = super::super::TempDir::new("savediff").unwrap();
        let mk = |name: &str, tweak: u8, extra: bool| {
            let dir = tmp.path().join(name).join("save1");
            std::fs::create_dir_all(&dir).unwrap();
            let world = xml(dict(vec![
                ("hostPort", PlistValue::int(tweak as i64)),
                ("saveDate", PlistValue::date(tweak as f64)),
            ]));
            std::fs::write(dir.join("worldv2"), &world).unwrap();
            std::fs::write(dir.join("adminlist.txt"), format!("hdr\nbob{tweak}\n")).unwrap();
            std::fs::write(dir.join("lock.mdb"), b"skip").unwrap();
            let blk = gzip_deflate(&block(tweak, 1, tweak as u32));
            let mut blocks: Vec<(&[u8], Vec<u8>)> = vec![(b"198_18", blk)];
            if extra {
                blocks.push((b"1_1", gzip_deflate(&block(0, 1, 0))));
            }
            let rows: Vec<(&[u8], &[u8])> = blocks.iter().map(|(k, v)| (*k, v.as_slice())).collect();
            put_env(&dir.join("world_db"), &[("blocks", &rows), ("main", &[(b"worldv2", &world)])]);
            put_env(&dir.join("server_db"), &[("misc", &[(b"k", b"v")])]);
            dir
        };
        let a = mk("a", 1, false);
        let b = mk("b", 2, true);
        (tmp, a, b)
    }

    #[test]
    fn whole_saves_load_diff_and_render() {
        let (tmp, a, b) = two_saves();
        let recs = load_records(&a).unwrap();
        assert!(
            recs.contains_key(&id("world_db", "blocks", "198_18")) && recs.contains_key(&id("server_db", "misc", "k"))
        );
        assert!(recs.contains_key(&id("file", "", "worldv2")) && !recs.contains_key(&id("file", "", "lock.mdb")));
        // a saves dir with one child resolves to it; anything else is rejected
        let rep = diff_saves(a.parent().unwrap(), b.parent().unwrap(), &SaveDiffOptions::parity_default()).unwrap();
        assert!(!rep.is_clean());
        let ids: Vec<String> = rep.diffs.iter().map(|d| d.id().to_string()).collect();
        assert!(
            ids.contains(&"world_db/blocks/1_1".to_string()) && ids.contains(&"file//adminlist.txt".to_string()),
            "{ids:?}"
        );
        assert!(rep.per_db.contains_key("file") && rep.per_db.contains_key("world_db/blocks"));
        let masked = SaveDiffOptions {
            masks: vec![Mask::Record("world_db/blocks/*".into()), Mask::AllDates],
            ..SaveDiffOptions::default()
        };
        assert!(diff_saves(&a, &b, &masked).unwrap().masked >= 2);
        assert!(diff_saves(&a, &a, &SaveDiffOptions::default()).unwrap().is_clean());
        let empty = tmp.path().join("empty");
        std::fs::create_dir_all(empty.join("x")).unwrap();
        std::fs::create_dir_all(empty.join("y")).unwrap();
        assert!(matches!(resolve_save(&empty), Err(HarnessError::Usage(_))));
        assert!(resolve_save(&tmp.path().join("missing")).is_err());
        assert!(open_env_readonly(&tmp.path().join("missing")).is_err());
        // an env directory that is not an LMDB env makes the load fail
        let broken = tmp.path().join("broken");
        std::fs::create_dir_all(broken.join("world_db")).unwrap();
        std::fs::write(broken.join("worldv2"), b"x").unwrap();
        assert!(matches!(load_records(&broken), Err(HarnessError::Lmdb(_))));
    }

    #[test]
    fn record_sets_and_odd_lmdb_content() {
        let o = SaveDiffOptions::default();
        let mut a = BTreeMap::new();
        let mut b = BTreeMap::new();
        a.insert(id("e", "d", "only-a"), vec![1]);
        b.insert(id("e", "d", "only-b"), vec![1]);
        a.insert(id("e", "d", "same"), vec![2]);
        b.insert(id("e", "d", "same"), vec![2]);
        let rep = diff_record_sets(&a, &b, &o);
        assert_eq!((rep.compared_records, rep.identical, rep.diffs.len()), (3, 1, 2));
        assert!(matches!(rep.diffs[0], RecordDiff::OnlyInA(_)) && matches!(rep.diffs[1], RecordDiff::OnlyInB(_)));
        // a plain key in the unnamed database is not a sub-database: the load fails
        let tmp = super::super::TempDir::new("odd-lmdb").unwrap();
        let save = tmp.path().join("save");
        std::fs::create_dir_all(&save).unwrap();
        std::fs::write(save.join("worldv2"), b"w").unwrap();
        std::fs::create_dir_all(save.join("world_db")).unwrap();
        // SAFETY: private temp directory.
        let env =
            unsafe { heed::EnvOpenOptions::new().max_dbs(4).map_size(1 << 24).open(save.join("world_db")) }.unwrap();
        let mut w = env.write_txn().unwrap();
        let main = env.create_database::<Bytes, Bytes>(&mut w, None).unwrap();
        main.put(&mut w, b"plain", b"value").unwrap();
        w.commit().unwrap();
        env.prepare_for_closing().wait();
        assert!(matches!(load_records(&save), Err(HarnessError::Lmdb(_))));
        // summary with an unreadable worldv2 only lists the file
        std::fs::remove_dir_all(save.join("world_db")).unwrap();
        std::fs::write(save.join("worldv2"), b"<?xml junk").unwrap();
        assert!(summary(&save).unwrap().contains("file worldv2: 10 bytes\n"));
    }

    #[test]
    fn inspection_commands() {
        let (tmp, a, _b) = two_saves();
        let (ok, bad) = regzip_check(&a).unwrap();
        assert_eq!((ok, bad.len()), (1, 0));
        // a block that recompresses differently, and one that is not gzip at all
        let raw = block(3, 1, 1);
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        std::io::Write::write_all(&mut enc, &raw).unwrap();
        let best = enc.finish().unwrap();
        let other = tmp.path().join("other/save1");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("worldv2"), b"w").unwrap();
        put_env(
            &other.join("world_db"),
            &[("blocks", &[(b"1_1", &best), (b"2_2", b"not gzip"), (b"3_3", &gzip_deflate(b"short"))])],
        );
        let (ok, bad) = regzip_check(&other).unwrap();
        assert_eq!(ok + bad.len(), 3);
        assert!(bad.iter().any(|i| i.key_string() == "2_2"));
        // grid
        let grid = render_block_grid(&a, 198, 18, "typeIndex").unwrap();
        assert_eq!(grid.lines().count(), 33);
        assert!(grid.starts_with("block 198_18: version=1 updateIndex=1 field=typeIndex"));
        assert_eq!(render_block_grid(&a, 5, 5, "typeIndex").unwrap(), "no such block\n");
        assert!(matches!(render_block_grid(&a, 198, 18, "nope"), Err(HarnessError::Usage(_))));
        assert!(matches!(render_block_grid(&other, 2, 2, "light"), Err(HarnessError::Parse(_))));
        assert!(matches!(render_block_grid(&other, 3, 3, "light"), Err(HarnessError::Parse(_))));
        // summary and dump
        let s = summary(&a).unwrap();
        assert!(
            s.contains("file worldv2:") && s.contains("/hostPort = integer:1") && s.contains("file adminlist.txt:")
        );
        assert!(
            s.contains("env world_db: data.mdb=") && s.contains("db \"blocks\": 1 entries") && s.contains("198_18:")
        );
        let out = tmp.path().join("dump");
        dump(&a, &out).unwrap();
        assert!(out.join("world_db/blocks/198_18.bin").exists() && out.join("world_db/main/worldv2.plist").exists());
        assert!(out.join("server_db/misc/k").exists());
        let other_dump = tmp.path().join("dump2");
        dump(&other, &other_dump).unwrap();
        assert!(other_dump.join("world_db/blocks/2_2").exists());
    }
}
