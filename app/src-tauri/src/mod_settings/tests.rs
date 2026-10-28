use super::*;
use crate::mods::tests::{spec, write_dir, zip, Spec};
use crate::mods::{edit_config, read_config};
use serde_json::json;
use std::fs;

const SETTINGS: &str = r#"
[settings.enabled_modes]
type = "bool"
default = true
label = { en = "Modes on", tr = "Modlar açık" }
help = { en = "Turn talking on" }
group = "Modes"

[settings.radius]
type = "int"
default = 32
min = 4
max = 128
step = 4
label = "Radius"
restart = true

[settings.falloff]
type = "float"
min = 0.0
max = 1.0
step = 0.05

[settings.curve]
type = "enum"
values = ["linear", "log"]

[settings.motd]
type = "string"
max-length = 5

[settings.admins]
type = "roles"
default = ["admin"]

[settings.talk]
type = "matrix"
rows = ["proximity", "global"]
cols = ["owner", "everyone"]
cell = "bool"
default = { proximity = { everyone = true } }

[settings.who]
type = "matrix"
rows = ["global"]
cols = ["talk"]
cell = "roles"

[settings.channels]
type = "list"
max_items = 2
[settings.channels.item.name]
type = "string"
max_length = 8
[settings.channels.item.size]
type = "int"
default = 1

[settings.keys]
type = "string"
scope = "client"
"#;

fn with_settings(root: &Path, id: &str) -> PathBuf {
    write_dir(root, id, &Spec { extra: SETTINGS, ..spec(id) })
}

#[test]
fn schema_in_declaration_order_with_contract_json() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path();
    let origin = with_settings(dir, "voice");
    assert_eq!(backend().server_fields(&origin), 9);
    let v = backend().get(dir, "voice").unwrap();
    let keys: Vec<&str> = v.fields.iter().map(|f| f.key.as_str()).collect();
    assert_eq!(
        keys,
        ["enabled_modes", "radius", "falloff", "curve", "motd", "admins", "talk", "who", "channels", "keys"]
    );
    assert!(v.values.is_empty());
    let j = serde_json::to_value(&v).unwrap();
    assert_eq!(
        j["fields"][0],
        json!({"key": "enabled_modes", "type": "bool", "default": true, "label": {"en": "Modes on", "tr": "Modlar açık"},
               "help": {"en": "Turn talking on"}, "group": "Modes", "scope": "server", "restart": false})
    );
    assert_eq!(j["fields"][1]["label"], json!({"en": "Radius"}));
    assert_eq!(j["fields"][1]["restart"], json!(true));
    assert_eq!(j["fields"][2]["default"], json!(0.0));
    assert_eq!(j["fields"][3]["default"], json!("linear"));
    assert_eq!(j["fields"][4]["max_length"], json!(5));
    assert_eq!(j["fields"][4]["label"], json!({"en": "motd"}));
    assert_eq!(j["fields"][6]["cell"], json!("bool"));
    assert_eq!(
        j["fields"][8]["item"],
        json!({"name": {"type": "string", "default": "", "label": {"en": "name"}, "scope": "server", "restart": false, "max_length": 8},
               "size": {"type": "int", "default": 1, "label": {"en": "size"}, "scope": "server", "restart": false}})
    );
    assert_eq!(j["fields"][8]["max_items"], json!(2));
    assert_eq!(j["fields"][9]["scope"], json!("client"));

    // Packed mods are read the same way; mods without settings have none.
    let packed = zip(&Spec { extra: SETTINGS, ..spec("packed") });
    fs::write(dir.join("packed-1.0.0.bhmod"), packed).unwrap();
    assert_eq!(backend().get(dir, "packed").unwrap().fields.len(), 10);
    let plain = write_dir(dir, "plain", &spec("plain"));
    assert_eq!(backend().server_fields(&plain), 0);
    assert!(backend().get(dir, "plain").unwrap().fields.is_empty());
    assert_eq!(backend().server_fields(&dir.join("nothing")), 0);
    fs::write(dir.join("junk.bhmod"), b"junk").unwrap();
    assert_eq!(backend().server_fields(&dir.join("junk.bhmod")), 0);
    assert_eq!(backend().get(dir, "nope").unwrap_err(), ModError::with("missing", "nope"));
    assert_eq!(backend().get(&dir.join("absent"), "x").unwrap_err().code, "missing");
}

#[test]
fn set_validates_stores_and_keeps_other_entries() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path();
    with_settings(dir, "voice");
    let b = backend();
    // The pinned discover() cannot list a mod with settings yet, so on/off goes through the file directly.
    let on = |v: bool| edit_config(dir, |c| c.mods.entry("voice".into()).or_default().enabled = v).unwrap();
    on(false);
    b.set(dir, "voice", "radius", json!(64)).unwrap();
    b.set(dir, "voice", "falloff", json!(1)).unwrap();
    b.set(dir, "voice", "talk", json!({"global": {"owner": true}})).unwrap();
    b.set(dir, "voice", "who", json!({"global": {"talk": ["admin", "vip"]}})).unwrap();
    b.set(dir, "voice", "channels", json!([{"name": "red", "size": 3}])).unwrap();
    let v = b.get(dir, "voice").unwrap();
    assert_eq!(v.values["radius"], json!(64));
    assert_eq!(v.values["falloff"], json!(1.0), "floats stay floats");
    assert_eq!(v.values["channels"], json!([{"name": "red", "size": 3}]));
    let text = fs::read_to_string(dir.join(FILE_NAME)).unwrap();
    assert!(text.contains("[voice]\nenabled = false\n"), "{text}");
    assert!(text.contains("[voice.settings]\nradius = 64\n"), "{text}");
    // On/off and grants still work next to the settings, and keep them.
    on(true);
    assert!(read_config(dir).unwrap().get("voice").enabled);
    assert_eq!(b.get(dir, "voice").unwrap().values["radius"], json!(64));
    // Back to the default: the key goes, then the table, then the entry.
    for k in ["radius", "falloff", "talk", "who"] {
        let d = b.get(dir, "voice").unwrap().fields.into_iter().find(|f| f.key == k).unwrap().default;
        b.set(dir, "voice", k, d).unwrap();
    }
    b.set(dir, "voice", "channels", json!([])).unwrap();
    assert!(!fs::read_to_string(dir.join(FILE_NAME)).unwrap().contains("voice"));
    // Values of unknown keys are not handed out.
    fs::write(dir.join(FILE_NAME), "[voice.settings]\nold = 1\nmotd = \"hi\"\n").unwrap();
    let v = b.get(dir, "voice").unwrap();
    assert_eq!(v.values.len(), 1);
    assert_eq!(v.values["motd"], json!("hi"));
    // Every TOML kind comes back as JSON.
    let raw: toml::Table = "a = 1979-05-27\nb = [1, 2.5]\n".parse().unwrap();
    let j: Vec<Value> = raw.into_iter().map(|(_, v)| to_json(v)).collect();
    assert_eq!(j, [json!("1979-05-27"), json!([1, 2.5])]);
}

#[test]
fn set_refuses_bad_values_keys_and_files() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path();
    with_settings(dir, "voice");
    let b = backend();
    let err = |k: &str, v: Value| b.set(dir, "voice", k, v).unwrap_err();
    let rule = |e: ModError| (e.detail.unwrap_or_default(), e.expected, e.got);
    assert_eq!(err("keys", json!("x")), ModError::with("settingKey", "keys"), "client scope");
    assert_eq!(err("nope", json!(1)), ModError::with("settingKey", "nope"));
    assert_eq!(err("enabled_modes", json!(1)).code, "setting");
    assert_eq!(rule(err("enabled_modes", json!(1))), ("type".into(), None, None));
    assert_eq!(rule(err("radius", json!(2.5))), ("type".into(), None, None));
    assert_eq!(rule(err("radius", json!(0))), ("min".into(), Some("4".into()), None));
    assert_eq!(rule(err("radius", json!(200))), ("max".into(), Some("128".into()), None));
    assert_eq!(rule(err("radius", json!(10))), ("step".into(), Some("4".into()), None));
    assert_eq!(rule(err("falloff", json!("x"))), ("type".into(), None, None));
    assert_eq!(rule(err("falloff", json!(1.5))), ("max".into(), Some("1".into()), None));
    assert_eq!(rule(err("curve", json!(1))), ("type".into(), None, None));
    assert_eq!(rule(err("curve", json!("cube"))), ("choice".into(), Some("linear, log".into()), None));
    assert_eq!(rule(err("motd", json!(1))), ("type".into(), None, None));
    assert_eq!(rule(err("motd", json!("toolong"))), ("length".into(), Some("5".into()), None));
    assert_eq!(rule(err("admins", json!("admin"))), ("type".into(), None, None));
    assert_eq!(rule(err("admins", json!([" "]))), ("role".into(), None, None));
    assert_eq!(rule(err("admins", json!([1]))), ("role".into(), None, None));
    assert_eq!(rule(err("talk", json!([]))), ("type".into(), None, None));
    assert_eq!(rule(err("talk", json!({"x": {}}))), ("unknown".into(), None, Some("x".into())));
    assert_eq!(rule(err("talk", json!({"global": 1}))), ("type".into(), None, Some("global".into())));
    assert_eq!(rule(err("talk", json!({"global": {"x": true}}))), ("unknown".into(), None, Some("global.x".into())));
    assert_eq!(rule(err("talk", json!({"global": {"owner": 1}}))), ("type".into(), None, Some("global.owner".into())));
    assert_eq!(rule(err("who", json!({"global": {"talk": [""]}}))), ("role".into(), None, Some("global.talk".into())));
    assert_eq!(rule(err("channels", json!({}))), ("type".into(), None, None));
    assert_eq!(rule(err("channels", json!([{}, {}, {}]))), ("items".into(), Some("2".into()), None));
    assert_eq!(rule(err("channels", json!([1]))), ("type".into(), None, Some("channels[0]".into())));
    assert_eq!(rule(err("channels", json!([{"x": 1}]))), ("unknown".into(), None, Some("channels[0].x".into())));
    assert_eq!(
        rule(err("channels", json!([{"name": "a"}, {"name": "much too long"}]))),
        ("length".into(), Some("8".into()), Some("channels[1].name".into()))
    );
    // A broken mods.toml is never overwritten; a missing mod cannot be set.
    fs::write(dir.join(FILE_NAME), "[voice]\nsettings = 3\n").unwrap();
    assert_eq!(b.set(dir, "voice", "radius", json!(8)).unwrap_err().code, "config");
    assert_eq!(b.get(dir, "voice").unwrap_err().code, "config");
    assert_eq!(read_config(dir).unwrap_err().code, "config");
    fs::write(dir.join(FILE_NAME), "not toml =").unwrap();
    assert_eq!(read_config(dir).unwrap_err().code, "config");
    fs::write(dir.join(FILE_NAME), "[voice]\nbogus = 1\n").unwrap();
    assert_eq!(read_config(dir).unwrap_err().code, "config");
    assert_eq!(b.set(dir, "gone", "radius", json!(8)).unwrap_err().code, "missing");
    assert_eq!(validate(&dummy(), &json!(true), "outer").unwrap(), json!(true));
    assert_eq!(num_text(0.5), "0.5");
    assert_eq!(num_text(1e20), "100000000000000000000");
}

fn dummy() -> Field {
    Field {
        key: String::new(),
        kind: Kind::Bool,
        default: json!(false),
        label: BTreeMap::new(),
        help: None,
        group: None,
        scope: Scope::Server,
        restart: false,
        min: None,
        max: None,
        step: None,
        values: None,
        max_length: None,
        rows: None,
        cols: None,
        cell: None,
        item: None,
        max_items: None,
    }
}

#[test]
fn nested_list_paths() {
    let inner = Field { key: "n".into(), kind: Kind::Int, max: Some(1.0), ..dummy() };
    let f = Field { key: "l".into(), kind: Kind::List, item: Some(vec![inner]), ..dummy() };
    let e = validate(&f, &json!([{"n": 2}]), "outer[0].l").unwrap_err();
    assert_eq!(e.got.as_deref(), Some("outer[0].l[0].n"));
    // An int without min steps from 0; a step of 0 is ignored.
    let s = Field { kind: Kind::Int, step: Some(5.0), ..dummy() };
    assert!(validate(&s, &json!(10), "").is_ok());
    let z = Field { kind: Kind::Int, step: Some(0.0), ..dummy() };
    assert!(validate(&z, &json!(3), "").is_ok());
}

#[test]
fn schema_errors() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path();
    let bad = [
        ("[settings]\na = 1", "a: must be a table"),
        ("[settings.a]\ntype = \"x\"", "a: unknown type"),
        ("[settings.a]\ntype = \"bool\"\nscope = \"x\"", "a: scope must be server or client"),
        ("[settings.a]\ntype = \"matrix\"\ncell = \"x\"", "a: cell must be bool or roles"),
        ("[settings.a]\ntype = \"list\"\nitem = 1", "a: item must be a table"),
        ("[settings.a]\ntype = \"list\"\n[settings.a.item.b]\ntype = \"x\"", "a.item.b: unknown type"),
        ("[settings.a]\ntype = \"bool\"\nlabel = 1", "a: label must be text"),
        ("[settings.a]\ntype = \"bool\"\nhelp = { en = 1 }", "a: help must be text"),
        ("[settings.a]\ntype = \"int\"\nmin = \"x\"", "a: min must be a number"),
        ("[settings.a]\ntype = \"enum\"\nvalues = 1", "a: values must be texts"),
        ("[settings.a]\ntype = \"enum\"\nvalues = [1]", "a: values must be texts"),
        ("[settings.a]\ntype = \"string\"\nmax_length = -1", "a: max_length must be a count"),
        ("[settings.a]\ntype = \"enum\"\nvalues = []", "a: enum needs values"),
        ("[settings.a]\ntype = \"matrix\"\nrows = []", "a: matrix needs rows, cols and cell"),
        ("[settings.a]\ntype = \"list\"", "a: list needs item"),
        ("[settings.a]\ntype = \"int\"\nmin = 1\ndefault = 0", "a: bad default (min)"),
    ];
    for (i, (extra, msg)) in bad.iter().enumerate() {
        let id = format!("m{i}");
        let origin = write_dir(dir, &id, &Spec { extra: &format!("{extra}\n"), ..spec(&id) });
        assert_eq!(backend().get(dir, &id).unwrap_err(), ModError::with("settingsSchema", *msg), "{extra}");
        assert_eq!(backend().server_fields(&origin), 0);
    }
    // Defaults of fields that declare none respect min/max.
    let origin = write_dir(
        dir,
        "defaults",
        &Spec { extra: "[settings.i]\ntype = \"int\"\nmin = 3\n[settings.j]\ntype = \"float\"\nmax = -1.0\nmin = -5\n[settings.r]\ntype = \"roles\"\n[settings.m]\ntype = \"matrix\"\nrows = []\ncols = []\ncell = \"roles\"\n", ..spec("defaults") },
    );
    assert_eq!(backend().server_fields(&origin), 4);
    let v = backend().get(dir, "defaults").unwrap();
    let d: Vec<Value> = v.fields.iter().map(|f| f.default.clone()).collect();
    assert_eq!(d, [json!(3), json!(-1.0), json!([]), json!({})]);
    assert_eq!(schema(&"settings = 1".parse().unwrap()).unwrap_err(), schema_err("settings", "must be a table"));
    // A mod.toml that is not TOML.
    fs::create_dir_all(dir.join("broken")).unwrap();
    fs::write(dir.join("broken/mod.toml"), "id = ").unwrap();
    assert_eq!(manifest_table(&dir.join("broken")).unwrap_err().code, "settingsSchema");
}
