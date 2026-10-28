//! Mod settings (voice plan V2): the fields a mod declares in its `mod.toml` (`[settings.<key>]`) and the admin's
//! values in `mods.toml` (`[<id>.settings]`), handed to the Settings tab of a mod's page as
//! `{ "fields": [...], "values": {...} }` (see [`View`]).
//!
//! The commands only talk to the [`Settings`] trait. [`Interim`] implements it by reading the TOML itself, because the
//! pinned mod-sdk does not know about settings yet.
//!
//! TODO(mod-sdk feat/settings): once mod-sdk feat/settings is merged and pinned, implement [`Settings`] with
//! bhmod-manifest's settings schema (parsed with the manifest, so `discover` lists mods that have settings) and
//! bhmod-core's storage and validation (`mods.toml` `[<id>.settings]`), delete [`Interim`], [`validate`] and the raw
//! manifest reading below, and point [`backend`] at the new implementation. The JSON shape of [`View`] stays.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use bhmod_core::config::FILE_NAME;
use bhmod_manifest::package::{Source, ZipSource, MAX_MANIFEST};
use serde::{Serialize, Serializer};
use serde_json::{Map, Value};

use crate::mods::{edit_settings, read_settings, MRes, ModError};

/// Field types (`type` in the schema).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Bool,
    Int,
    Float,
    Enum,
    String,
    Roles,
    Matrix,
    List,
}

/// Who edits a field: the server admin (host app, `/mod set`) or each player on their device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Server,
    Client,
}

/// What a matrix cell holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Cell {
    Bool,
    Roles,
}

/// One setting. Serialized as the UI contract: snake_case keys, absent options left out; list item fields are a map
/// from sub-key to a field without its `key`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Field {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub key: String,
    #[serde(rename = "type")]
    pub kind: Kind,
    pub default: Value,
    pub label: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub scope: Scope,
    pub restart: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cols: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell: Option<Cell>,
    #[serde(skip_serializing_if = "Option::is_none", serialize_with = "item_map")]
    pub item: Option<Vec<Field>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_items: Option<usize>,
}

/// `item` as `{sub-key: field without key}`, in declaration order.
fn item_map<S: Serializer>(item: &Option<Vec<Field>>, s: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let fields = item.as_deref().unwrap_or_default();
    let mut m = s.serialize_map(Some(fields.len()))?;
    for f in fields {
        m.serialize_entry(&f.key, &Field { key: String::new(), ..f.clone() })?;
    }
    m.end()
}

/// What the Settings tab gets: every field the mod declares and the values set in `mods.toml` (unset fields use
/// their default).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct View {
    pub fields: Vec<Field>,
    pub values: Map<String, Value>,
}

/// The settings layer the commands use. See the module docs for what replaces [`Interim`].
pub trait Settings: Sync {
    /// Server-scope fields of the package at `origin` (0 when it declares none or cannot be read).
    fn server_fields(&self, origin: &Path) -> usize;
    /// The schema and values of mod `id` installed in `dir`.
    fn get(&self, dir: &Path, id: &str) -> MRes<View>;
    /// Checks `value` against the field `key` and stores it (a value equal to the default is removed).
    fn set(&self, dir: &Path, id: &str, key: &str, value: Value) -> MRes<()>;
}

/// The implementation in use.
pub fn backend() -> &'static dyn Settings {
    &Interim
}

// ---------------------------------------------------------------------------------------------------------------
// TODO(mod-sdk feat/settings): everything below is temporary (see the module docs).

/// Reads `[settings.*]` from the raw `mod.toml` and validates values by the UI contract.
pub struct Interim;

impl Settings for Interim {
    fn server_fields(&self, origin: &Path) -> usize {
        manifest_table(origin)
            .and_then(|t| schema(&t))
            .map_or(0, |f| f.iter().filter(|f| f.scope == Scope::Server).count())
    }

    fn get(&self, dir: &Path, id: &str) -> MRes<View> {
        let fields = schema(&find(dir, id)?.1)?;
        let mut stored = stored(dir, id)?;
        let values = fields.iter().filter_map(|f| stored.remove(&f.key).map(|v| (f.key.clone(), v))).collect();
        Ok(View { fields, values })
    }

    fn set(&self, dir: &Path, id: &str, key: &str, value: Value) -> MRes<()> {
        let fields = schema(&find(dir, id)?.1)?;
        let f = fields
            .iter()
            .find(|f| f.key == key && f.scope == Scope::Server)
            .ok_or_else(|| ModError::with("settingKey", key))?;
        let value = validate(f, &value, "")?;
        let is_default = value == f.default;
        let toml_value = toml::Value::try_from(&value).map_err(|e| ModError::with("io", e.to_string()))?;
        edit_settings(dir, id, |t| {
            if is_default {
                t.remove(key);
            } else {
                t.insert(key.into(), toml_value);
            }
        })
    }
}

/// The `mod.toml` of a package (directory or `.bhmod` zip) as TOML.
fn manifest_table(origin: &Path) -> MRes<toml::Table> {
    let bytes = if origin.is_dir() {
        let f = std::fs::File::open(origin.join("mod.toml")).map_err(|e| ModError::with("io", e.to_string()))?;
        let mut buf = Vec::new();
        f.take(MAX_MANIFEST).read_to_end(&mut buf).map_err(|e| ModError::with("io", e.to_string()))?;
        buf
    } else {
        let f = std::fs::File::open(origin).map_err(|e| ModError::with("io", e.to_string()))?;
        let len = f.metadata().map_or(0, |m| m.len());
        let mut zip = ZipSource::open(f, len).map_err(|e| ModError::with("io", e.to_string()))?;
        zip.read("mod.toml", MAX_MANIFEST).map_err(|e| ModError::with("io", e.to_string()))?
    };
    String::from_utf8_lossy(&bytes).parse::<toml::Table>().map_err(|e| ModError::with("settingsSchema", e.to_string()))
}

/// The installed package of `id` by its raw manifest (the pinned `discover` rejects manifests with `[settings]`).
fn find(dir: &Path, id: &str) -> MRes<(PathBuf, toml::Table)> {
    let entries = std::fs::read_dir(dir).map_err(|_| ModError::with("missing", id))?;
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| !n.to_string_lossy().starts_with('.') && n != FILE_NAME))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .find_map(|p| {
            manifest_table(&p).ok().filter(|t| t.get("id").and_then(|v| v.as_str()) == Some(id)).map(|t| (p, t))
        })
        .ok_or_else(|| ModError::with("missing", id))
}

/// The admin's stored values of mod `id` (JSON).
fn stored(dir: &Path, id: &str) -> MRes<Map<String, Value>> {
    let table = read_settings(dir)?.remove(id).unwrap_or_default();
    Ok(table.into_iter().map(|(k, v)| (k, to_json(v))).collect())
}

fn to_json(v: toml::Value) -> Value {
    match v {
        toml::Value::String(s) => Value::String(s),
        toml::Value::Integer(i) => Value::from(i),
        toml::Value::Float(f) => Value::from(f),
        toml::Value::Boolean(b) => Value::Bool(b),
        toml::Value::Datetime(d) => Value::String(d.to_string()),
        toml::Value::Array(a) => Value::Array(a.into_iter().map(to_json).collect()),
        toml::Value::Table(t) => Value::Object(t.into_iter().map(|(k, v)| (k, to_json(v))).collect()),
    }
}

fn schema_err(key: &str, what: &str) -> ModError {
    ModError::with("settingsSchema", format!("{key}: {what}"))
}

/// All fields of `[settings]`, in declaration order.
fn schema(manifest: &toml::Table) -> MRes<Vec<Field>> {
    match manifest.get("settings") {
        None => Ok(Vec::new()),
        Some(toml::Value::Table(t)) => fields(t, ""),
        Some(_) => Err(schema_err("settings", "must be a table")),
    }
}

fn fields(t: &toml::Table, prefix: &str) -> MRes<Vec<Field>> {
    t.iter()
        .map(|(k, v)| {
            let path = format!("{prefix}{k}");
            v.as_table().ok_or_else(|| schema_err(&path, "must be a table")).and_then(|t| field(k, &path, t))
        })
        .collect()
}

fn texts(v: Option<&toml::Value>, path: &str, what: &str) -> MRes<Option<BTreeMap<String, String>>> {
    match v {
        None => Ok(None),
        Some(toml::Value::String(s)) => Ok(Some(BTreeMap::from([("en".to_string(), s.clone())]))),
        Some(toml::Value::Table(t)) => t
            .iter()
            .map(|(l, s)| s.as_str().map(|s| (l.clone(), s.to_string())).ok_or_else(|| schema_err(path, what)))
            .collect::<MRes<_>>()
            .map(Some),
        Some(_) => Err(schema_err(path, what)),
    }
}

fn strings(v: Option<&toml::Value>, path: &str, what: &str) -> MRes<Option<Vec<String>>> {
    match v {
        None => Ok(None),
        Some(toml::Value::Array(a)) => a
            .iter()
            .map(|s| s.as_str().map(String::from).ok_or_else(|| schema_err(path, what)))
            .collect::<MRes<_>>()
            .map(Some),
        Some(_) => Err(schema_err(path, what)),
    }
}

fn number(v: Option<&toml::Value>, path: &str, what: &str) -> MRes<Option<f64>> {
    match v {
        None => Ok(None),
        Some(toml::Value::Integer(i)) => Ok(Some(*i as f64)),
        Some(toml::Value::Float(f)) => Ok(Some(*f)),
        Some(_) => Err(schema_err(path, what)),
    }
}

fn count(v: Option<&toml::Value>, path: &str, what: &str) -> MRes<Option<usize>> {
    match v {
        None => Ok(None),
        Some(toml::Value::Integer(i)) if *i >= 0 => Ok(Some(*i as usize)),
        Some(_) => Err(schema_err(path, what)),
    }
}

/// Reads one field; `-` and `_` spellings of the two-word options are both accepted.
fn field(key: &str, path: &str, t: &toml::Table) -> MRes<Field> {
    let get = |k: &str| t.get(k).or_else(|| t.get(&k.replace('_', "-")));
    let kind = match get("type").and_then(|v| v.as_str()) {
        Some("bool") => Kind::Bool,
        Some("int") => Kind::Int,
        Some("float") => Kind::Float,
        Some("enum") => Kind::Enum,
        Some("string") => Kind::String,
        Some("roles") => Kind::Roles,
        Some("matrix") => Kind::Matrix,
        Some("list") => Kind::List,
        _ => return Err(schema_err(path, "unknown type")),
    };
    let scope = match get("scope").and_then(|v| v.as_str()) {
        None | Some("server") => Scope::Server,
        Some("client") => Scope::Client,
        Some(_) => return Err(schema_err(path, "scope must be server or client")),
    };
    let cell = match get("cell").and_then(|v| v.as_str()) {
        None => None,
        Some("bool") => Some(Cell::Bool),
        Some("roles") => Some(Cell::Roles),
        Some(_) => return Err(schema_err(path, "cell must be bool or roles")),
    };
    let item = match get("item") {
        None => None,
        Some(toml::Value::Table(it)) => Some(fields(it, &format!("{path}.item."))?),
        Some(_) => return Err(schema_err(path, "item must be a table")),
    };
    let mut f = Field {
        key: key.into(),
        kind,
        default: Value::Null,
        label: texts(get("label"), path, "label must be text")?
            .unwrap_or_else(|| BTreeMap::from([("en".into(), key.into())])),
        help: texts(get("help"), path, "help must be text")?,
        group: get("group").and_then(|v| v.as_str()).map(String::from),
        scope,
        restart: get("restart").and_then(|v| v.as_bool()).unwrap_or(false),
        min: number(get("min"), path, "min must be a number")?,
        max: number(get("max"), path, "max must be a number")?,
        step: number(get("step"), path, "step must be a number")?,
        values: strings(get("values"), path, "values must be texts")?,
        max_length: count(get("max_length"), path, "max_length must be a count")?,
        rows: strings(get("rows"), path, "rows must be texts")?,
        cols: strings(get("cols"), path, "cols must be texts")?,
        cell,
        item,
        max_items: count(get("max_items"), path, "max_items must be a count")?,
    };
    let missing = match kind {
        Kind::Enum if f.values.as_ref().is_none_or(|v| v.is_empty()) => Some("enum needs values"),
        Kind::Matrix if f.rows.is_none() || f.cols.is_none() || f.cell.is_none() => {
            Some("matrix needs rows, cols and cell")
        }
        Kind::List if f.item.is_none() => Some("list needs item"),
        _ => None,
    };
    if let Some(m) = missing {
        return Err(schema_err(path, m));
    }
    f.default = match get("default") {
        Some(v) => to_json(v.clone()),
        None => empty(&f),
    };
    f.default = validate(&f, &f.default, "")
        .map_err(|e| schema_err(path, &format!("bad default ({})", e.detail.unwrap_or_default())))?;
    Ok(f)
}

/// The default of a field that declares none.
fn empty(f: &Field) -> Value {
    match f.kind {
        Kind::Bool => Value::Bool(false),
        Kind::Int => Value::from(f.min.unwrap_or(0.0).max(0.0).min(f.max.unwrap_or(f64::MAX)) as i64),
        Kind::Float => Value::from(f.min.unwrap_or(0.0).max(0.0).min(f.max.unwrap_or(f64::MAX))),
        Kind::Enum => Value::String(f.values.as_ref().map(|v| v[0].clone()).unwrap_or_default()),
        Kind::String => Value::String(String::new()),
        Kind::Roles | Kind::List => Value::Array(Vec::new()),
        Kind::Matrix => Value::Object(Map::new()),
    }
}

/// `32.0` -> `"32"`, `0.5` -> `"0.5"`.
fn num_text(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

/// A value that breaks its field: `detail` is the rule (`type`, `min`, `max`, `step`, `choice`, `length`, `items`,
/// `unknown`, `role`), `expected` the limit, `got` where inside the value (`row.col`, `list[2].name`).
fn invalid(rule: &str, expected: Option<String>, at: &str) -> ModError {
    ModError { expected, got: (!at.is_empty()).then(|| at.to_string()), ..ModError::with("setting", rule) }
}

fn join(at: &str, part: &str) -> String {
    if at.is_empty() {
        part.to_string()
    } else {
        format!("{at}.{part}")
    }
}

fn roles(v: &Value, at: &str) -> MRes<Value> {
    let a = v.as_array().ok_or_else(|| invalid("type", None, at))?;
    if a.iter().any(|r| r.as_str().is_none_or(|s| s.trim().is_empty() || s.len() > 40)) {
        return Err(invalid("role", None, at));
    }
    Ok(v.clone())
}

/// Checks a value against its field; returns it normalised (floats stored as floats).
pub fn validate(f: &Field, v: &Value, at: &str) -> MRes<Value> {
    let bounds = |n: f64| -> MRes<()> {
        if let Some(min) = f.min.filter(|m| n < *m) {
            return Err(invalid("min", Some(num_text(min)), at));
        }
        if let Some(max) = f.max.filter(|m| n > *m) {
            return Err(invalid("max", Some(num_text(max)), at));
        }
        Ok(())
    };
    match f.kind {
        Kind::Bool => v.as_bool().map(Value::Bool).ok_or_else(|| invalid("type", None, at)),
        Kind::Int => {
            let n = v.as_i64().ok_or_else(|| invalid("type", None, at))?;
            bounds(n as f64)?;
            if let Some(step) = f.step.filter(|s| *s > 0.0) {
                let off = (n as f64 - f.min.unwrap_or(0.0)) / step;
                if (off - off.round()).abs() > 1e-9 {
                    return Err(invalid("step", Some(num_text(step)), at));
                }
            }
            Ok(Value::from(n))
        }
        Kind::Float => {
            let n = v.as_f64().ok_or_else(|| invalid("type", None, at))?;
            bounds(n)?;
            Ok(Value::from(n))
        }
        Kind::Enum => {
            let s = v.as_str().ok_or_else(|| invalid("type", None, at))?;
            let values = f.values.as_deref().unwrap_or_default();
            if !values.iter().any(|x| x == s) {
                return Err(invalid("choice", Some(values.join(", ")), at));
            }
            Ok(v.clone())
        }
        Kind::String => {
            let s = v.as_str().ok_or_else(|| invalid("type", None, at))?;
            if let Some(max) = f.max_length.filter(|m| s.chars().count() > *m) {
                return Err(invalid("length", Some(max.to_string()), at));
            }
            Ok(v.clone())
        }
        Kind::Roles => roles(v, at),
        Kind::Matrix => {
            let rows = v.as_object().ok_or_else(|| invalid("type", None, at))?;
            let (rnames, cnames) = (f.rows.as_deref().unwrap_or_default(), f.cols.as_deref().unwrap_or_default());
            let mut out = Map::new();
            for (r, cols) in rows {
                let rat = join(at, r);
                if !rnames.contains(r) {
                    return Err(invalid("unknown", None, &rat));
                }
                let cols = cols.as_object().ok_or_else(|| invalid("type", None, &rat))?;
                let mut row = Map::new();
                for (c, cell) in cols {
                    let cat = join(&rat, c);
                    if !cnames.contains(c) {
                        return Err(invalid("unknown", None, &cat));
                    }
                    let cell = match f.cell {
                        Some(Cell::Roles) => roles(cell, &cat)?,
                        _ => cell.as_bool().map(Value::Bool).ok_or_else(|| invalid("type", None, &cat))?,
                    };
                    row.insert(c.clone(), cell);
                }
                out.insert(r.clone(), Value::Object(row));
            }
            Ok(Value::Object(out))
        }
        Kind::List => {
            let items = v.as_array().ok_or_else(|| invalid("type", None, at))?;
            if let Some(max) = f.max_items.filter(|m| items.len() > *m) {
                return Err(invalid("items", Some(max.to_string()), at));
            }
            let sub = f.item.as_deref().unwrap_or_default();
            let mut out = Vec::new();
            for (i, it) in items.iter().enumerate() {
                let iat = format!("{}[{i}]", if at.is_empty() { &f.key } else { at });
                let obj = it.as_object().ok_or_else(|| invalid("type", None, &iat))?;
                let mut o = Map::new();
                for (k, x) in obj {
                    let kat = join(&iat, k);
                    let sf = sub.iter().find(|s| &s.key == k).ok_or_else(|| invalid("unknown", None, &kat))?;
                    o.insert(k.clone(), validate(sf, x, &kat)?);
                }
                out.push(Value::Object(o));
            }
            Ok(Value::Array(out))
        }
    }
}

#[cfg(test)]
mod tests;
