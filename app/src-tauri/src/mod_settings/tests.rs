use super::*;
use crate::mods::tests::{spec, write_dir, zip, Spec};
use crate::mods::{edit_config, set_grant};
use bhmod_core::config::FILE_NAME;
use serde_json::json;
use std::fs;
use std::path::PathBuf;

const SETTINGS: &str = r#"
[settings.enabled_modes]
type = "bool"
default = true
label = { en = "Modes on", tr = "Modlar açık" }
help = { en = "Turn talking on" }
group = "modes"

[settings.radius]
type = "int"
default = 32
min = 4
max = 128
step = 4
label = "Radius"
restart = true

[settings.motd]
type = "string"
default = ""
max_length = 5
label = "Message"

[settings.talk]
type = "matrix"
rows = ["proximity", "global"]
cols = ["owner", "everyone"]
default = { proximity = { owner = true, everyone = true }, global = { owner = true, everyone = false } }
label = "Who may talk"

[settings.channels]
type = "list"
default = []
max_items = 2
label = "Channels"
item = { name = { type = "string", default = "", max_length = 8, label = "Name" }, size = { type = "int", default = 1, label = "Size" } }

[settings.keys]
type = "string"
default = "V"
scope = "client"
label = "Key"
"#;

fn with_settings(root: &Path, id: &str) -> PathBuf {
    write_dir(root, id, &Spec { api: "^0.3.3", extra: SETTINGS, ..spec(id) })
}

#[test]
fn schema_in_declaration_order_with_the_canonical_json() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path();
    with_settings(dir, "voice");
    let m = find_package(dir, "voice").unwrap().manifest;
    assert_eq!(server_fields(&m), 5);
    let v = get(dir, "voice").unwrap();
    let keys: Vec<&str> = v.fields.iter().map(|f| f["key"].as_str().unwrap()).collect();
    assert_eq!(keys, ["enabled_modes", "radius", "motd", "talk", "channels", "keys"]);
    assert_eq!(v.values["radius"], json!(32), "every key has a value");
    assert_eq!(
        v.fields[0],
        json!({"key": "enabled_modes", "type": "bool", "default": true, "label": {"en": "Modes on", "tr": "Modlar açık"},
               "help": {"en": "Turn talking on"}, "group": "modes", "scope": "server", "restart": false})
    );
    assert_eq!(v.fields[1]["step"], json!(4));
    assert_eq!(v.fields[4]["item"][0]["key"], json!("name"), "list items are an array of fields");
    assert_eq!(v.fields[5]["scope"], json!("client"));
    // Packed mods are read the same way; mods without settings have none; a missing mod is an error.
    let packed = zip(&Spec { api: "^0.3.3", extra: SETTINGS, ..spec("packed") });
    fs::write(dir.join("packed-1.0.0.bhmod"), packed).unwrap();
    assert_eq!(get(dir, "packed").unwrap().fields.len(), 6);
    write_dir(dir, "plain", &spec("plain"));
    assert!(get(dir, "plain").unwrap().fields.is_empty());
    assert_eq!(server_fields(&find_package(dir, "plain").unwrap().manifest), 0);
    assert_eq!(get(dir, "nope").unwrap_err(), ModError::with("missing", "nope"));
}

#[test]
fn set_validates_stores_and_keeps_the_rest_of_the_file() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path();
    with_settings(dir, "voice");
    fs::write(dir.join(FILE_NAME), "# mine\n[voice]\nenabled = false\n").unwrap();
    set(dir, "voice", "radius", json!(64)).unwrap();
    set(dir, "voice", "channels", json!([{"name": "red"}])).unwrap();
    let v = get(dir, "voice").unwrap();
    assert_eq!(v.values["radius"], json!(64));
    assert_eq!(v.values["channels"], json!([{"name": "red", "size": 1}]), "record fields get their default");
    let text = fs::read_to_string(dir.join(FILE_NAME)).unwrap();
    assert!(text.starts_with("# mine\n[voice]\nenabled = false\n"), "{text}");
    assert!(text.contains("radius = 64"), "{text}");
    // On/off, grants and the client-required override keep the settings.
    edit_config(dir, |c| c.mods.entry("voice".into()).or_default().enabled = true).unwrap();
    set_grant(dir, "voice", "native", false).unwrap();
    edit_config(dir, |c| c.mods.entry("voice".into()).or_default().client_required = Some(false)).unwrap();
    assert_eq!(get(dir, "voice").unwrap().values["radius"], json!(64));
    // Back to the default: the key goes, then the table.
    set(dir, "voice", "radius", json!(32)).unwrap();
    set(dir, "voice", "channels", json!([])).unwrap();
    let text = fs::read_to_string(dir.join(FILE_NAME)).unwrap();
    assert!(!text.contains("settings"), "{text}");
    // A stored value that does not fit any more falls back to the default; unknown keys are not handed out.
    fs::write(dir.join(FILE_NAME), "[voice.settings]\nold = 1\nradius = 1000\nmotd = \"hi\"\n").unwrap();
    let v = get(dir, "voice").unwrap();
    assert_eq!((v.values.len(), &v.values["radius"], &v.values["motd"]), (6, &json!(32), &json!("hi")));
}

#[test]
fn set_refuses_bad_values_keys_and_files() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path();
    with_settings(dir, "voice");
    let err = |k: &str, v: Value| set(dir, "voice", k, v).unwrap_err();
    let unknown = |k: &str| ModError {
        code: "settingKey".into(),
        detail: Some(k.into()),
        got: Some(k.into()),
        ..ModError::default()
    };
    assert_eq!(err("keys", json!("x")), unknown("keys"), "client scope");
    assert_eq!(err("nope", json!(1)), unknown("nope"));
    let e = err("radius", json!(0));
    assert_eq!(
        (e.code.as_str(), e.detail.as_deref(), e.expected.as_deref(), e.got.as_deref()),
        ("setting", Some("min"), Some("4"), Some("radius"))
    );
    assert_eq!(err("radius", json!(2.5)).detail.as_deref(), Some("integer"));
    assert!(set(dir, "voice", "radius", json!(10)).is_ok(), "step is a hint");
    assert_eq!(err("motd", json!("toolong")).detail.as_deref(), Some("length"));
    let e = err("talk", json!({"global": {"owner": true, "everyone": true}}));
    assert_eq!((e.detail.as_deref(), e.got.as_deref()), (Some("items"), Some("talk")), "every row is needed");
    let e = err("channels", json!([{"name": "much too long"}]));
    assert_eq!(e.got.as_deref(), Some("channels[0].name"));
    // A broken mods.toml is never overwritten; a missing mod cannot be set.
    fs::write(dir.join(FILE_NAME), "[voice]\nsettings = 3\n").unwrap();
    assert_eq!(set(dir, "voice", "radius", json!(8)).unwrap_err().code, "config");
    assert_eq!(get(dir, "voice").unwrap_err().code, "config");
    assert_eq!(fs::read_to_string(dir.join(FILE_NAME)).unwrap(), "[voice]\nsettings = 3\n");
    assert_eq!(set(dir, "gone", "radius", json!(8)).unwrap_err().code, "missing");
    // A file that cannot be written.
    fs::remove_file(dir.join(FILE_NAME)).unwrap();
    let ro = tempfile::tempdir().unwrap();
    with_settings(ro.path(), "voice");
    fs::create_dir(ro.path().join(FILE_NAME)).unwrap();
    assert_eq!(set(ro.path(), "voice", "radius", json!(8)).unwrap_err().code, "io");
}
