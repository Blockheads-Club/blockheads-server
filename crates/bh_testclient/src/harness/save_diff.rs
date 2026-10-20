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
    let _guard = OPEN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let mut out = BTreeMap::new();
    for e in ENVS {
        let p = save.join(e);
        if !p.is_dir() {
            continue;
        }
        let env = open_env_readonly(&p)?;
        {
            let rtxn = env.read_txn().map_err(lm)?;
            if let Some(root) = env.open_database::<Bytes, Bytes>(&rtxn, None).map_err(lm)? {
                let mut names: Vec<Vec<u8>> = Vec::new();
                for kv in root.iter(&rtxn).map_err(lm)? {
                    let (k, _) = kv.map_err(lm)?;
                    names.push(k.to_vec());
                }
                for name in names {
                    let nm = String::from_utf8_lossy(&name).to_string();
                    let db = match env.open_database::<Bytes, Bytes>(&rtxn, Some(&nm)).map_err(lm)? {
                        Some(d) => d,
                        None => continue,
                    };
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
            (None, Some(y)) => {
                diffs.push(PlistDiff { path: disp, kind: PlistDiffKind::OnlyInB, a: String::new(), b: y.show() })
            }
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
            (None, None) => {}
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
            (Some(_), None) => rep.diffs.push(RecordDiff::OnlyInA(id.clone())),
            (None, Some(_)) => rep.diffs.push(RecordDiff::OnlyInB(id.clone())),
            (Some(x), Some(y)) => match compare_record(id, x, y, &m, o) {
                None => rep.identical += 1,
                Some(d) => rep.diffs.push(d),
            },
            (None, None) => {}
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

    fn sample(rel: &str) -> Option<PathBuf> {
        let p = super::super::refenv_dir().join("samples").join(rel);
        if p.exists() {
            Some(p)
        } else {
            eprintln!("SKIP: sample {} missing", rel);
            None
        }
    }
    fn only_child(p: &Path) -> PathBuf {
        resolve_save(p).unwrap_or_else(|_| {
            let mut v: Vec<PathBuf> =
                std::fs::read_dir(p).unwrap().map(|e| e.unwrap().path()).filter(|x| x.is_dir()).collect();
            v.sort();
            v.remove(0)
        })
    }
    fn save_of(rel: &str) -> Option<PathBuf> {
        sample(rel).map(|p| only_child(&p))
    }

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
        match compare_record(&id, &a, &b, &masks, &o) {
            Some(RecordDiff::Plist { diffs, .. }) => assert_eq!(diffs[0].kind, PlistDiffKind::TypeMismatch),
            other => panic!("{:?}", other),
        }
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
        match compare_record(&bid, &ga, &gb, &masks, &o) {
            Some(RecordDiff::Block { differing_tiles: 1, sample, .. }) => {
                let t = &sample[0];
                assert_eq!(
                    (t.local_x, t.local_y, t.world_x, t.world_y, t.field, t.a, t.b),
                    (14, 2, 6350, 578, "typeIndex", 25, 6)
                );
            }
            other => panic!("{:?}", other),
        }
        // level-9 gzip -> GzipRawOnly, ignored when not strict
        let mut f = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        std::io::Write::write_all(&mut f, &ba).unwrap();
        let g9 = f.finish().unwrap();
        if g9 != ga {
            assert!(matches!(compare_record(&bid, &ga, &g9, &masks, &o), Some(RecordDiff::GzipRawOnly { .. })));
            let lax = SaveDiffOptions { strict_gzip: false, ..Default::default() };
            assert!(compare_record(&bid, &ga, &g9, &masks, &lax).is_none());
        }
        // tile mask
        let tmask = [Mask::TileBytes(vec![0])];
        let tm = Masks::new(&tmask);
        assert!(
            compare_record(&bid, &ga, &gb, &tm, &o).is_none()
                || matches!(compare_record(&bid, &ga, &gb, &tm, &o), Some(RecordDiff::GzipRawOnly { .. }))
        );
        // text
        let tid = RecordId { env: "file".into(), db: String::new(), key: b"adminlist.txt".to_vec() };
        match compare_record(&tid, b"hdr\n", b"HDR\n", &masks, &o) {
            Some(RecordDiff::Text { first_diff_line: 1, .. }) => {}
            other => panic!("{:?}", other),
        }
        // empty sets are never "clean"
        let rep = diff_record_sets(&BTreeMap::new(), &BTreeMap::new(), &o);
        assert!(!rep.is_clean());
    }

    /// S1.
    #[test]
    fn s1_rep1_rep2() {
        let (Some(a), Some(b)) = (save_of("rep1"), save_of("rep2")) else { return };
        let r = diff_saves(&a, &b, &SaveDiffOptions::default()).unwrap();
        assert!(r.compared_records > 0);
        assert_eq!(r.diffs.len(), 1, "{}", r.render(50));
        match &r.diffs[0] {
            RecordDiff::Plist { id, diffs } => {
                assert_eq!(id.to_string(), "world_db/main/worldv2");
                for d in diffs {
                    assert!(["/creationDate", "/saveDate", "/hostPort"].contains(&d.path.as_str()), "{}", d.path);
                }
            }
            o => panic!("{:?}", o),
        }
        assert_eq!(r.identical, 416);
        let c = diff_saves(&a, &b, &SaveDiffOptions::parity_default()).unwrap();
        assert!(c.is_clean(), "{}", c.render(50));
    }

    /// S2/S3.
    #[test]
    fn s2_s3_seeds() {
        let (Some(a), Some(b)) = (save_of("seedA"), save_of("seedB")) else { return };
        let r = diff_saves(&a, &b, &SaveDiffOptions::default()).unwrap();
        let ids: Vec<String> = r.diffs.iter().map(|d| d.id().to_string()).collect();
        assert_eq!(ids, vec!["file//worldv2".to_string(), "world_db/main/worldv2".to_string()], "{}", r.render(50));
        assert_eq!(r.identical, 326);
        assert!(diff_saves(&a, &b, &SaveDiffOptions::parity_default()).unwrap().is_clean());
        if let Some(c) = save_of("seedC") {
            let r = diff_saves(&a, &c, &SaveDiffOptions::default()).unwrap();
            for d in &r.diffs {
                let id = d.id().to_string();
                assert!(id == "file//worldv2" || id == "world_db/main/worldv2", "{}", id);
            }
            assert!(r.identical >= 320, "{}", r.render(20));
        }
    }

    /// S4 + S5 + S7.
    #[test]
    fn s4_s5_s7() {
        let (Some(a), Some(b)) = (save_of("w1"), save_of("expert")) else { return };
        let r = diff_saves(&a, &b, &SaveDiffOptions::parity_default()).unwrap();
        assert!(!r.is_clean());
        assert!(r.diffs.iter().any(|d| matches!(d, RecordDiff::Block { differing_tiles, .. } if *differing_tiles > 0)));
        let (ok, bad) = regzip_check(&a).unwrap();
        assert_eq!((ok, bad.len()), (398, 0));
        // S7: read-only open leaves lock.mdb alone
        let lock = a.join("world_db/lock.mdb");
        let before = (std::fs::metadata(&lock).unwrap().modified().unwrap(), std::fs::read(&lock).unwrap());
        let _ = load_records(&a).unwrap();
        let after = (std::fs::metadata(&lock).unwrap().modified().unwrap(), std::fs::read(&lock).unwrap());
        assert!(before == after);
        let s = render_block_grid(&a, 198, 18, "typeIndex").unwrap();
        assert!(s.lines().count() == 33, "{}", s);
        assert!(summary(&a).unwrap().contains("db \"blocks\""));
    }

    /// S6. Mutation tests on a copy of w1.
    #[test]
    fn s6_mutations() {
        use heed::types::Bytes;
        let Some(w1) = save_of("w1") else { return };
        let tmp = super::super::TempDir::new("s6").unwrap();
        let copy = tmp.path().join("save");
        fn cp(a: &Path, b: &Path) {
            std::fs::create_dir_all(b).unwrap();
            for e in std::fs::read_dir(a).unwrap() {
                let e = e.unwrap();
                let t = b.join(e.file_name());
                if e.file_type().unwrap().is_dir() {
                    cp(&e.path(), &t);
                } else {
                    std::fs::copy(e.path(), t).unwrap();
                }
            }
        }
        cp(&w1, &copy);
        let o = SaveDiffOptions::default();
        let put = |db: &str, key: &str, f: &dyn Fn(Vec<u8>) -> Option<Vec<u8>>| {
            // SAFETY: private temp copy, no other process uses it.
            let env = unsafe { heed::EnvOpenOptions::new().max_dbs(16).map_size(1 << 30).open(copy.join("world_db")) }
                .unwrap();
            let mut w = env.write_txn().unwrap();
            let d = env.create_database::<Bytes, Bytes>(&mut w, Some(db)).unwrap();
            let old = d.get(&w, key.as_bytes()).unwrap().unwrap().to_vec();
            match f(old) {
                Some(n) => d.put(&mut w, key.as_bytes(), &n).unwrap(),
                None => {
                    d.delete(&mut w, key.as_bytes()).unwrap();
                }
            }
            w.commit().unwrap();
        };
        // block tile byte
        put("blocks", "198_18", &|v| {
            let mut raw = gzip_inflate(&v).unwrap();
            raw[(2 * 32 + 14) * 64] = raw[(2 * 32 + 14) * 64].wrapping_add(1);
            Some(gzip_deflate(&raw))
        });
        let r = diff_saves(&w1, &copy, &o).unwrap();
        assert_eq!(r.diffs.len(), 1);
        let txt = r.render(20);
        assert!(txt.contains("blocks 198_18 tile(14,2) world(6350,578) typeIndex"), "{}", txt);
        // delete record
        put("blocks", "198_18", &|_| None);
        let r = diff_saves(&w1, &copy, &o).unwrap();
        assert!(r.diffs.iter().any(|d| matches!(d, RecordDiff::OnlyInA(_))));
    }
}
