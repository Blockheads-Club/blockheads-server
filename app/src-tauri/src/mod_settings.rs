//! Mod settings (voice plan V2): the fields a mod declares in its `mod.toml` (`[settings.<key>]`, read by
//! bhmod-manifest) and the admin's values in `mods.toml` (`[<id>.settings]`, read and written by bhmod-core), handed
//! to the Settings tab of a mod's page as `{ "fields": [...], "values": {...} }` (see [`View`]).
//!
//! The JSON is mod-sdk's canonical form (`bhmod_manifest::settings::schema_json`): fields in declaration order,
//! `item` of a list is an array of field objects, `step` is a hint only. A running server picks a change up by
//! itself (it watches `mods.toml`); settings marked `restart` apply at the next start.

use std::path::Path;

use bhmod_manifest::settings::{schema_json, validate_value, Scope, SettingsError};
use bhmod_manifest::Manifest;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::mods::{find_package, read_config, MRes, ModError};

/// What the Settings tab gets: every field the mod declares and the values it runs with (stored valid values,
/// else the defaults).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct View {
    pub fields: Vec<Value>,
    pub values: Map<String, Value>,
}

/// Server-scope fields of a mod (the ones its Settings tab shows).
pub fn server_fields(m: &Manifest) -> usize {
    m.settings.fields.iter().filter(|f| f.scope == Scope::Server).count()
}

/// The schema and values of mod `id` installed in `dir`. Stored values that no longer fit fall back to the default
/// like on the server.
pub fn get(dir: &Path, id: &str) -> MRes<View> {
    let m = find_package(dir, id)?.manifest;
    let stored = read_config(dir)?.get(id).settings_json();
    let (values, _ignored) = m.settings.resolve(&stored);
    let mut j = schema_json(&m, &values);
    let fields = match j["fields"].take() {
        Value::Array(a) => a,
        _ => Vec::new(),
    };
    let values = match j["values"].take() {
        Value::Object(o) => o,
        _ => Map::new(),
    };
    Ok(View { fields, values })
}

/// A rejected value in the shape the UI reads: `code` (`settingKey` / `setting`), `detail` the rule, `expected` what
/// the rule wants and `got` where inside the value it broke (`SettingsError`'s `key`: `radius`, `talk.global`,
/// `channels[2].name`; the given value itself is not shown).
pub fn setting_error(e: &SettingsError) -> ModError {
    let j = e.to_json();
    let text = |k: &str| j.get(k).and_then(Value::as_str).map(String::from);
    ModError {
        code: e.code().into(),
        // An unknown key has no rule: the key is the detail (the UI names it).
        detail: text("detail").or_else(|| text("key")),
        expected: text("expected"),
        got: text("key"),
        ..ModError::default()
    }
}

/// Checks `value` against the server setting `key` and stores it in `mods.toml` (a value equal to the default is
/// removed). The rest of the file stays as it is.
pub fn set(dir: &Path, id: &str, key: &str, value: Value) -> MRes<()> {
    let m = find_package(dir, id)?.manifest;
    let field = m
        .settings
        .get(key)
        .filter(|f| f.scope == Scope::Server)
        .ok_or_else(|| setting_error(&SettingsError::UnknownKey(key.into())))?;
    let v = validate_value(field, &value).map_err(|e| setting_error(&e))?;
    read_config(dir)?;
    let stored = (v != field.default).then_some(&v);
    bhmod_core::config::write_setting(dir, id, key, stored).map_err(|e| ModError::with("io", e))
}

#[cfg(test)]
mod tests;
