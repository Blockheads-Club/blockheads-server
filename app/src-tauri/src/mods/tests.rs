use super::*;
use bhmod_manifest::package::DirSource;
use std::fs;

const TOML: &str = "schema = 1\nid = \"ID\"\nversion = \"VER\"\nname = { en = \"NAME\" }\nauthors = [\"ann\"]\nlicense = \"MIT\"\napi = \"API\"\n[sides]\nserver = true\nclient = CLIENT\n[entry]\nserver = { runtime = \"lua\", path = \"server/main.luau\" }\n";

pub(crate) struct Spec<'a> {
    pub id: &'a str,
    pub version: &'a str,
    pub api: &'a str,
    pub client: bool,
    pub extra: &'a str,
}

pub(crate) fn spec(id: &str) -> Spec<'_> {
    Spec { id, version: "1.0.0", api: "^0.1", client: false, extra: "" }
}

/// An unpacked mod in `root/dir`.
pub(crate) fn write_dir(root: &Path, dir: &str, s: &Spec) -> PathBuf {
    let d = root.join(dir);
    fs::create_dir_all(d.join("server")).unwrap();
    let toml = TOML
        .replace("ID", s.id)
        .replace("VER", s.version)
        .replace("NAME", &s.id.to_uppercase())
        .replace("API", s.api)
        .replace("CLIENT", if s.client { "true" } else { "false" });
    let client = if s.client { "client = { runtime = \"lua\", path = \"client/main.luau\" }\n" } else { "" };
    fs::write(d.join("mod.toml"), format!("{toml}{client}{}", s.extra)).unwrap();
    fs::write(d.join("server/main.luau"), "return {}").unwrap();
    if s.client {
        fs::create_dir_all(d.join("client")).unwrap();
        fs::write(d.join("client/main.luau"), "return {}").unwrap();
    }
    d
}

/// The `.bhmod` bytes of a mod.
pub(crate) fn zip(s: &Spec) -> Vec<u8> {
    let t = tempfile::tempdir().unwrap();
    let d = write_dir(t.path(), "m", s);
    let mut src = DirSource::open(&d).unwrap();
    bhmod_manifest::pack::write_zip(&mut src, Cursor::new(Vec::new())).unwrap().into_inner()
}

pub(crate) fn sha(b: &[u8]) -> String {
    bhmod_manifest::pack::sha256_hex(b)
}

pub(crate) fn version_json(v: &str, url: &str, bytes: &[u8], extra: &str) -> String {
    format!(
        r#"{{"version":"{v}","url":"{url}","sha256":"{}","size":{},"sides":["server"],"api":"^0.1"{},"published":"2026-10-01"{extra}}}"#,
        sha(bytes),
        bytes.len(),
        if extra.contains("permissions") { "" } else { r#","permissions":{"players":["chat"]}"# }
    )
}

pub(crate) fn index_json(mods: &[String]) -> String {
    format!(r#"{{"schema":1,"generated":"2026-10-21T00:00:00Z","api":["0.1.0"],"mods":[{}]}}"#, mods.join(","))
}

pub(crate) fn mod_json(id: &str, versions: &[String]) -> String {
    format!(
        r#"{{"id":"{id}","name":"{} name","owners":["o"],"repo":"https://x","tags":["chat"],"versions":[{}]}}"#,
        id,
        versions.join(",")
    )
}

#[test]
fn config_round_trip_and_invalid_file() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().join("mods");
    assert_eq!(read_config(&dir).unwrap(), ModsConfig::default());
    write_dir(&dir, "a", &spec("aa"));
    set_enabled(&dir, "aa", false).unwrap();
    assert!(!read_config(&dir).unwrap().get("aa").enabled);
    set_enabled(&dir, "aa", true).unwrap();
    // Back to the defaults: the entry is dropped.
    assert!(read_config(&dir).unwrap().mods.is_empty());
    assert_eq!(set_enabled(&dir, "zz", true).unwrap_err().code, "missing");
    fs::write(dir.join(FILE_NAME), "[aa]\nbogus = 1\n").unwrap();
    assert_eq!(read_config(&dir).unwrap_err().code, "config");
    assert_eq!(set_enabled(&dir, "aa", false).unwrap_err().code, "config");
    assert_eq!(fs::read_to_string(dir.join(FILE_NAME)).unwrap(), "[aa]\nbogus = 1\n", "never overwritten");
    // Unreadable: a directory where the file should be.
    fs::remove_file(dir.join(FILE_NAME)).unwrap();
    fs::create_dir(dir.join(FILE_NAME)).unwrap();
    assert_eq!(read_config(&dir).unwrap_err().code, "io");
}

#[test]
fn grants_and_remove() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path();
    write_dir(dir, "a", &Spec { extra: "[permissions]\nplayers = [\"chat\", \"kick\"]\n", ..spec("aa") });
    set_grant(dir, "aa", "players:kick", true).unwrap();
    set_grant(dir, "aa", "players:chat", true).unwrap();
    set_grant(dir, "aa", "players:chat", true).unwrap();
    assert_eq!(read_config(dir).unwrap().get("aa").grant, vec!["players:chat", "players:kick"]);
    assert_eq!(set_grant(dir, "aa", "native", true).unwrap_err().code, "permission");
    set_grant(dir, "aa", "players:kick", false).unwrap();
    assert_eq!(read_config(dir).unwrap().get("aa").grant, vec!["players:chat"]);
    fs::write(dir.join("zz-1.0.0.bhmod"), zip(&spec("zz"))).unwrap();
    fs::write(dir.join("broken.bhmod"), b"nope").unwrap();
    for bad in ["", ".hidden", "../x", "a/b", FILE_NAME, "absent"] {
        assert_eq!(remove(dir, bad).unwrap_err().code, "missing", "{bad}");
    }
    remove(dir, "a").unwrap();
    remove(dir, "zz-1.0.0.bhmod").unwrap();
    remove(dir, "broken.bhmod").unwrap();
    assert!(read_config(dir).unwrap().mods.is_empty());
    assert!(fs::read_dir(dir).unwrap().flatten().all(|e| e.file_name() == FILE_NAME));
}

#[test]
fn log_lines_and_live_state() {
    let p = "2026-10-22 10:00:00.000 blockheads_server[1:2] ";
    assert_eq!(parse_line(&format!("{p}[mods] hello: enabled")), Some(("info", Some("hello"), "enabled")));
    assert_eq!(parse_line(&format!("{p}[mods WARN] hello: x")), Some(("warn", Some("hello"), "x")));
    assert_eq!(
        parse_line(&format!("{p}[mods ERROR] loaded 2 mod(s): a, b")),
        Some(("error", None, "loaded 2 mod(s): a, b"))
    );
    assert_eq!(parse_line(&format!("{p}[mods] Not An Id: x")), Some(("info", None, "Not An Id: x")));
    assert_eq!(parse_line(&format!("{p}[mods]x")), None);
    assert_eq!(parse_line(&format!("{p}MIRA: [mods] hello: enabled")), None, "chat cannot imitate the host");
    assert_eq!(parse_line(&format!("{p}[modsX] a: b")), None);
    assert_eq!(parse_line(&format!("{p}World load complete.")), None);
    assert!(!is_mod_id("a") && !is_mod_id(&"a".repeat(41)) && is_mod_id("a_b-1"));
    assert_eq!(slow_ms("tick took 7.5 ms (budget 5 ms)"), Some(7.5));
    assert_eq!(slow_ms("tick took long"), None);

    let mut l = Live::default();
    let push = |l: &mut Live, s: &str| l.push(&format!("{p}{s}"), 7);
    push(&mut l, "World load complete.");
    push(&mut l, "[mods] loaded 1 mod(s): hello 1.0.0");
    push(&mut l, "[mods] hello: enabled");
    assert_eq!(l.states["hello"], LiveState::Enabled);
    push(&mut l, "[mods WARN] hello: tick took 7.5 ms (budget 5 ms)");
    push(&mut l, "[mods WARN] hello: on-disable failed: x");
    push(&mut l, "[mods ERROR] hello: tick: timeout");
    push(&mut l, "[mods] hello: disabled: 3 faults within 60 s");
    assert_eq!(l.states["hello"], LiveState::Disabled("3 faults within 60 s".into()));
    let f = &l.faults["hello"];
    assert_eq!((f[0].kind.as_str(), f[0].ms), ("slow", Some(7.5)));
    assert_eq!(f[1].kind, "error");
    push(&mut l, "[mods] other: disabled: on-enable failed: boom");
    assert_eq!(l.states["other"], LiveState::Failed("on-enable failed: boom".into()));
    push(&mut l, "[mods ERROR] cannot start bad: syntax error");
    assert_eq!(l.states["bad"], LiveState::Failed("syntax error".into()));
    assert_eq!(l.faults["bad"][0].kind, "start");
    push(&mut l, "[mods ERROR] not loading /m/x: x: needs API");
    assert_eq!(l.host.len(), 3);
    assert_eq!(l.lines["hello"].len(), 5);
    for _ in 0..70 {
        push(&mut l, "[mods ERROR] hello: tick: trap");
    }
    assert_eq!(l.lines["hello"].len(), KEEP_LINES);
    assert_eq!(l.faults["hello"].len(), KEEP_FAULTS);
}

#[test]
fn snapshots_list_what_a_restart_applies() {
    let a = |v: &str, en: bool, g: &[&str], fp: &str| {
        (v.to_string(), en, g.iter().map(|s| s.to_string()).collect(), fp.to_string())
    };
    let mut loaded = Snapshot::default();
    loaded.mods.insert("same".into(), a("1", true, &[], "f"));
    loaded.mods.insert("upd".into(), a("1", true, &[], "f"));
    loaded.mods.insert("on".into(), a("1", false, &[], "f"));
    loaded.mods.insert("off".into(), a("1", true, &[], "f"));
    loaded.mods.insert("perm".into(), a("1", true, &[], "f"));
    loaded.mods.insert("permoff".into(), a("1", false, &[], "f"));
    loaded.mods.insert("gone".into(), a("1", true, &[], "f"));
    let mut now = loaded.clone();
    now.mods.remove("gone");
    now.mods.insert("upd".into(), a("2", true, &[], "g"));
    now.mods.insert("on".into(), a("1", true, &[], "f"));
    now.mods.insert("off".into(), a("1", false, &[], "f"));
    now.mods.insert("perm".into(), a("1", true, &["x"], "f"));
    now.mods.insert("permoff".into(), a("1", false, &["x"], "f"));
    now.mods.insert("new".into(), a("1", true, &[], "n"));
    let kinds: Vec<(String, String)> = changes(&loaded, &now).into_iter().map(|c| (c.kind, c.id)).collect();
    let want = [
        ("installed", "new"),
        ("disabled", "off"),
        ("enabled", "on"),
        ("permissions", "perm"),
        ("updated", "upd"),
        ("removed", "gone"),
    ];
    assert_eq!(kinds, want.map(|(k, i)| (k.to_string(), i.to_string())).to_vec());

    let t = tempfile::tempdir().unwrap();
    write_dir(t.path(), "d", &spec("dd"));
    fs::write(t.path().join("zz.bhmod"), zip(&spec("zz"))).unwrap();
    let s = snapshot(t.path());
    assert_eq!(s.mods.len(), 2);
    assert_eq!(s.mods["dd"].3.len(), 64, "directory fingerprint is the manifest hash");
    assert_eq!(s.mods["zz"].3, sha(&zip(&spec("zz"))));
}

fn by_id<'a>(o: &'a Overview, id: &str) -> &'a ModSummary {
    o.mods.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("{id} not listed: {o:?}"))
}

#[test]
fn overview_statuses() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().join("mods");
    // A missing folder: no mods, no error.
    let o = overview(&dir, &Context::default());
    assert!(o.mods.is_empty() && o.error.is_none() && o.size == 0);

    write_dir(&dir, "off", &spec("off"));
    write_dir(
        &dir,
        "perm",
        &Spec { extra: "[permissions]\nnet-http = [\"discord.com\"]\nnative = true\n", ..spec("perm") },
    );
    write_dir(&dir, "dep", &Spec { extra: "[dependencies]\neconomy = \"^2\"\nclientthing = { version = \"1\", side = \"client\" }\n[soft-dependencies]\nnice = \"1\"\n", ..spec("dep") });
    // Two packages with one id: the second is refused by the host.
    write_dir(&dir, "twin-a", &spec("twin"));
    write_dir(&dir, "twin-b", &spec("twin"));
    write_dir(&dir, "ok", &Spec { client: true, extra: "[dependencies]\nbase = \">=1\"\n", ..spec("ok") });
    write_dir(&dir, "base", &spec("base"));
    fs::write(dir.join("broken.bhmod"), b"no").unwrap();
    // A client-only mod (no server entry).
    let c = dir.join("cl");
    fs::create_dir_all(c.join("client")).unwrap();
    fs::write(
        c.join("mod.toml"),
        "schema = 1\nid = \"cl\"\nversion = \"1.0.0\"\nname = { en = \"Cl\" }\nauthors = [\"a\"]\nlicense = \"MIT\"\napi = \"^0.1\"\n[sides]\nserver = false\nclient = true\n[entry]\nclient = { runtime = \"lua\", path = \"client/main.luau\" }\n",
    )
    .unwrap();
    fs::write(c.join("client/main.luau"), "return {}").unwrap();
    set_enabled(&dir, "off", false).unwrap();

    let o = overview(&dir, &Context::default());
    assert!(o.size > 0);
    assert_eq!(o.problems.len(), 1, "{:?}", o.problems);
    assert_eq!(o.problems[0].file, "broken.bhmod");
    assert_eq!(by_id(&o, "off").status, "off");
    let p = by_id(&o, "perm");
    assert_eq!((p.status.as_str(), p.detail.as_deref()), ("permissions", Some("net-http:discord.com, native")));
    assert!(p.native);
    let d = by_id(&o, "dep");
    assert_eq!((d.status.as_str(), d.detail.as_deref()), ("dependency", Some("economy ^2")));
    assert_eq!(d.dependencies.len(), 2, "client-side dependencies are not needed here");
    assert!(d.dependencies[1].soft);
    let twins: Vec<&ModSummary> = o.mods.iter().filter(|m| m.id == "twin").collect();
    assert_eq!((twins[0].status.as_str(), twins[1].status.as_str()), ("ready", "failed"));
    assert!(twins[1].detail.as_deref().unwrap().contains("same id"), "{:?}", twins[1].detail);
    let k = by_id(&o, "ok");
    assert_eq!(k.status, "ready");
    assert!(k.dependencies[0].ok && k.sides.client_required);
    assert_eq!(k.runtime.as_deref(), Some("lua"));
    assert_eq!(by_id(&o, "cl").status, "ready");
    assert_eq!(o.client_mods, vec!["cl", "ok"]);
    assert!(o.changes.is_empty());

    // Running: live state from the log, and what changed since the start.
    let loaded = snapshot(&dir);
    let mut live = Live::default();
    let p = "x[1] ";
    live.push(&format!("{p}[mods] ok: enabled"), 1);
    live.push(&format!("{p}[mods WARN] ok: tick took 9 ms (budget 5 ms)"), 2);
    live.push(&format!("{p}[mods WARN] ok: tick took 6 ms (budget 5 ms)"), 3);
    live.push(&format!("{p}[mods] base: disabled: 3 faults within 60 s"), 4);
    live.push(&format!("{p}[mods] loaded 2 mod(s)"), 5);
    let ctx = Context { running: true, live: Some(&live), loaded: Some(&loaded), ..Default::default() };
    let o = overview(&dir, &ctx);
    let k = by_id(&o, "ok");
    assert_eq!((k.status.as_str(), k.slowest_ms, k.lines.len()), ("running", Some(9.0), 3));
    assert_eq!(by_id(&o, "base").status, "budget");
    assert_eq!(by_id(&o, "cl").status, "running");
    assert_eq!(o.host_lines.len(), 1);
    assert_eq!(k.storage_bytes, None, "storage is not read while running");
    live.push(&format!("{p}[mods] base: disabled: host stopping"), 6);
    live.states.insert("ok".into(), LiveState::Failed("trap".into()));
    let o = overview(&dir, &Context { running: true, live: Some(&live), loaded: Some(&loaded), ..Default::default() });
    assert_eq!(by_id(&o, "base").status, "off");
    assert_eq!(by_id(&o, "ok").status, "failed");
    live.states.clear();
    let o = overview(&dir, &Context { running: true, live: Some(&live), loaded: Some(&loaded), ..Default::default() });
    assert_eq!(by_id(&o, "ok").status, "starting");
    // A change while running waits for a restart.
    write_dir(&dir, "late", &spec("late"));
    set_grant(&dir, "perm", "native", true).unwrap();
    let o = overview(&dir, &Context { running: true, live: Some(&live), loaded: Some(&loaded), ..Default::default() });
    assert_eq!(by_id(&o, "late").status, "restart");
    assert_eq!(o.changes.len(), 2, "{:?}", o.changes);
    let o = overview(&dir, &Context { running: true, live: Some(&live), loaded: None, ..Default::default() });
    assert_eq!(by_id(&o, "late").status, "starting");

    // An invalid mods.toml is reported; the mods are listed with the defaults.
    fs::write(dir.join(FILE_NAME), "nope =").unwrap();
    let o = overview(&dir, &Context::default());
    assert_eq!(o.error.as_ref().unwrap().code, "config");
    assert_eq!(by_id(&o, "off").status, "ready");
    // A folder that cannot be read.
    let file = t.path().join("file");
    fs::write(&file, "x").unwrap();
    let o = overview(&file, &Context::default());
    assert_eq!(o.error.unwrap().code, "io");
}

#[test]
fn reviewed_updates_and_storage() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().join("mods");
    fs::create_dir_all(&dir).unwrap();
    let z = zip(&spec("chat"));
    fs::write(dir.join("chat-1.0.0.bhmod"), &z).unwrap();
    write_dir(&dir, "dirmod", &spec("dirmod"));
    let newer = zip(&Spec { version: "1.2.0", ..spec("chat") });
    let idx = parse_index(
        index_json(&[
            mod_json(
                "chat",
                &[
                    version_json("1.0.0", "https://x/a.bhmod", &z, ""),
                    version_json("1.2.0", "https://x/b.bhmod", &newer, ""),
                ],
            ),
            mod_json("dirmod", &[version_json("0.5.0", "https://x/c.bhmod", &z, "")]),
        ])
        .as_bytes(),
    )
    .unwrap();
    // Storage: one key for "chat".
    let storage = t.path().join("storage");
    {
        use blockheads_server::storage::lmdb::{Database, DatabaseEnvironment};
        let env = DatabaseEnvironment::new(&storage.to_string_lossy(), 256, 1024).unwrap();
        assert!(Database::new(&env, "chat").unwrap().put("k", b"value"));
    }
    let ctx = Context { index: Some(&idx), storage: Some(storage.clone()), ..Default::default() };
    let o = overview(&dir, &ctx);
    let c = by_id(&o, "chat");
    assert!(c.reviewed && c.packed);
    assert_eq!(c.update.as_deref(), Some("1.2.0"));
    assert_eq!(c.storage_bytes, Some(6));
    let d = by_id(&o, "dirmod");
    assert!(!d.reviewed && !d.packed && d.update.is_none());
    assert_eq!(d.storage_bytes, Some(0));
    let r = StorageReader::open(&storage);
    assert!(r.clear("chat"));
    assert_eq!(r.size("chat"), Some(0));
    drop(r);
    // A world without mod storage.
    let none = StorageReader::open(&t.path().join("nothing"));
    assert_eq!(none.size("chat"), Some(0));
    assert!(none.clear("chat"));
}

#[test]
fn index_parse_fetch_and_cache() {
    assert_eq!(parse_index(b"{").unwrap_err().code, "index");
    assert_eq!(parse_index(br#"{"schema":2,"generated":"x","mods":[]}"#).unwrap_err().code, "index");
    let t = tempfile::tempdir().unwrap();
    let f = t.path().join("i.json");
    fs::write(&f, index_json(&[])).unwrap();
    assert_eq!(fetch(&format!("file://{}", f.display()), 1000).unwrap().len(), index_json(&[]).len());
    assert_eq!(fetch(f.to_str().unwrap(), 5).unwrap_err().code, "tooLarge");
    assert_eq!(fetch("/nonexistent/x", 5).unwrap_err().code, "download");
    assert_eq!(fetch("http://x/y", 5).unwrap_err().code, "insecure");
    assert_eq!(fetch("https://x/y", 5).unwrap_err().code, "download");
    let dir = t.path().join("dir");
    fs::create_dir(&dir).unwrap();
    assert_eq!(fetch(dir.to_str().unwrap(), 5).unwrap_err().code, "download");

    let cache = t.path().join("cache.json");
    assert!(cached_index(&cache).is_none());
    assert_eq!(load_index("https://offline", &cache).unwrap_err().code, "offline");
    let (idx, offline, err) = load_index(f.to_str().unwrap(), &cache).unwrap();
    assert!(!offline && err.is_none() && idx.mods.is_empty());
    assert!(cached_index(&cache).is_some());
    let (_, offline, err) = load_index("https://offline", &cache).unwrap();
    assert!(offline);
    assert_eq!(err.unwrap().code, "download");
}

#[test]
fn browse_lists_usable_versions() {
    let z = zip(&spec("a"));
    let idx = parse_index(
        index_json(&[
            mod_json(
                "a",
                &[
                    version_json(
                        "1.0.0",
                        "https://x/1",
                        &z,
                        r#","published":"2026-09-01""#.replace(r#","published":"2026-09-01""#, "").as_str(),
                    ),
                    version_json("2.0.0", "https://x/2", &z, r#","client-required":true,"depends":{"b":"^1"}"#),
                    version_json("3.0.0", "https://x/3", &z, r#","yanked":true"#),
                ],
            ),
            mod_json(
                "old",
                &[version_json("1.0.0", "https://x/o", &z, "").replace(r#""api":"^0.1""#, r#""api":"^9""#)],
            ),
        ])
        .as_bytes(),
    )
    .unwrap();
    let mut installed = HashMap::new();
    installed.insert("a".to_string(), "1.0.0".to_string());
    let b = browse(&idx, &installed, true, Some(ModError::new("download")));
    assert_eq!((b.mods.len(), b.hidden, b.offline), (1, 1, true));
    let m = &b.mods[0];
    assert_eq!((m.version.as_str(), m.client_required, m.installed.as_deref()), ("2.0.0", true, Some("1.0.0")));
    assert_eq!(m.permissions, vec!["players:chat"]);
    assert_eq!(m.first_published, "2026-10-01");
    assert_eq!(m.depends["b"], "^1");
}

pub(crate) fn fixture_index(t: &Path) -> (IndexFile, Vec<u8>) {
    let shop = zip(&Spec { extra: "[dependencies]\neconomy = \"^2\"\n", ..spec("shop") });
    let eco = zip(&Spec { version: "2.3.1", extra: "[dependencies]\nbase = \"^1\"\n", ..spec("economy") });
    let base = zip(&spec("base"));
    let w = |n: &str, b: &[u8]| {
        let p = t.join(n);
        fs::write(&p, b).unwrap();
        p.to_string_lossy().into_owned()
    };
    let json = index_json(&[
        mod_json(
            "shop",
            &[version_json("1.0.0", &w("shop.bhmod", &shop), &shop, r#","depends":{"economy":"^2","ghost":"1"}"#)],
        ),
        mod_json(
            "economy",
            &[
                version_json("1.0.0", "https://x/e1", &eco, ""),
                version_json(
                    "2.3.1",
                    &w("eco.bhmod", &eco),
                    &eco,
                    r#","depends":{"base":"^1"},"permissions":{"native":true}"#,
                ),
            ],
        ),
        mod_json("base", &[version_json("1.0.0", &w("base.bhmod", &base), &base, "")]),
    ]);
    (parse_index(json.as_bytes()).unwrap(), shop)
}

#[test]
fn plans_resolve_dependencies() {
    let t = tempfile::tempdir().unwrap();
    let (idx, _) = fixture_index(t.path());
    let none = HashMap::new();
    let p = plan_index(&idx, &none, "shop", None).unwrap();
    let ids: Vec<(&str, Option<&str>)> = p.items.iter().map(|i| (i.id.as_str(), i.needed_by.as_deref())).collect();
    assert_eq!(ids, vec![("shop", None), ("economy", Some("shop")), ("base", Some("economy"))]);
    assert!(p.items[1].native);
    assert_eq!(p.missing, vec![Missing { id: "ghost".into(), req: "1".into(), needed_by: "shop".into() }]);
    assert!(p.total_bytes > 0);
    let mut have = HashMap::new();
    have.insert("base".to_string(), "1.4.0".to_string());
    have.insert("economy".to_string(), "1.0.0".to_string());
    let p = plan_index(&idx, &have, "economy", Some("2.3.1")).unwrap();
    assert_eq!(p.items[0].replaces.as_deref(), Some("1.0.0"));
    assert_eq!(p.satisfied, vec![Satisfied { id: "base".into(), req: "^1".into(), version: "1.4.0".into() }]);
    assert_eq!(plan_index(&idx, &none, "nope", None).unwrap_err().code, "notListed");
    assert_eq!(plan_index(&idx, &none, "shop", Some("9.9.9")).unwrap_err().code, "notListed");
    // A dependency listed twice (shop -> economy, economy also listed by a later dependency) is planned once.
    let mut deps = BTreeMap::new();
    deps.insert("economy".to_string(), "^2".to_string());
    deps.insert("base".to_string(), "bogus req".to_string());
    let mut plan = Plan { items: vec![], satisfied: vec![], missing: vec![], total_bytes: 0 };
    add_deps(Some(&idx), &none, ("root", &deps), &mut plan);
    assert_eq!(plan.items.len(), 2);
}

#[test]
fn inspect_and_candidate_plans() {
    let t = tempfile::tempdir().unwrap();
    let (idx, shop) = fixture_index(t.path());
    assert_eq!(inspect(b"junk".to_vec(), "x.bhmod").unwrap_err().code, "invalid");
    assert_eq!(inspect(vec![0; MAX_PACKAGE as usize + 1], "big").unwrap_err().code, "tooLarge");
    let bad = zip(&Spec { api: "not a req", ..spec("bad") });
    let e = inspect(bad, "bad.bhmod").unwrap_err();
    assert_eq!((e.code.as_str(), e.file.as_deref()), ("invalid", Some("bad.bhmod")));
    let c = inspect(shop.clone(), "shop.bhmod").unwrap();
    let p = plan_candidate(&c, Some(&idx), &HashMap::new());
    assert!(p.items[0].reviewed);
    assert_eq!(p.items.len(), 3);
    let other = inspect(
        zip(&Spec {
            client: true,
            extra: "[dependencies]\nxx = { version = \"1\", side = \"client\" }\n",
            ..spec("other")
        }),
        "o",
    )
    .unwrap();
    let p = plan_candidate(&other, None, &HashMap::new());
    assert!(!p.items[0].reviewed);
    assert_eq!(p.items[0].sides, vec!["server", "client"]);
    assert!(p.missing.is_empty(), "client-side dependencies are not installed on the server");
    // Neither a server part nor a client side.
    let t2 = tempfile::tempdir().unwrap();
    let d = t2.path().join("n");
    fs::create_dir_all(&d).unwrap();
    fs::write(
        d.join("mod.toml"),
        "schema = 1\nid = \"nothing\"\nversion = \"1.0.0\"\nname = { de = \"N\" }\nauthors = [\"a\"]\nlicense = \"MIT\"\napi = \"^0.1\"\n[sides]\nserver = true\nclient = false\n",
    )
    .unwrap();
    let mut src = DirSource::open(&d).unwrap();
    let bytes = bhmod_manifest::pack::write_zip(&mut src, Cursor::new(Vec::new())).unwrap().into_inner();
    match inspect(bytes, "n") {
        Err(e) => assert_eq!(e.code, "invalid"),
        Ok(c) => panic!("accepted {:?}", c.manifest.id),
    }
}

#[test]
fn downloads_are_checked() {
    let t = tempfile::tempdir().unwrap();
    let (idx, _) = fixture_index(t.path());
    let v = |id: &str| idx.mods.iter().find(|m| m.id == id).unwrap().latest().unwrap().clone();
    assert_eq!(download(&v("base"), "base").unwrap().manifest.id, "base");
    let mut wrong_size = v("base");
    wrong_size.size = Some(3);
    assert_eq!(download(&wrong_size, "base").unwrap_err().code, "tooLarge");
    let mut short = v("base");
    short.size = Some(short.size.unwrap() + 10);
    let e = download(&short, "base").unwrap_err();
    assert_eq!((e.code.as_str(), e.file.as_deref()), ("size", Some("base-1.0.0.bhmod")));
    let mut bad_sum = v("base");
    bad_sum.sha256 = "0".repeat(64);
    let e = download(&bad_sum, "base").unwrap_err();
    assert_eq!(e.code, "checksum");
    assert_eq!(e.expected.as_deref(), Some("0".repeat(64).as_str()));
    let mut mirrored = v("base");
    mirrored.mirror = Some(mirrored.url.clone());
    mirrored.url = "https://gone/x.bhmod".into();
    assert!(download(&mirrored, "base").is_ok());
    mirrored.mirror = Some("https://gone/y.bhmod".into());
    assert_eq!(download(&mirrored, "base").unwrap_err().code, "download");
    let e = download(&v("base"), "economy").unwrap_err();
    assert_eq!(e.code, "mismatch");
    let mut nosize = v("base");
    nosize.size = None;
    assert!(download(&nosize, "base").is_ok());
}

#[test]
fn packages_replace_old_versions_and_record_grants() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path().join("mods");
    write_dir(&dir, "old-econ", &Spec { version: "1.0.0", ..spec("economy") });
    fs::write(dir.join("economy-0.9.0.bhmod"), zip(&Spec { version: "0.9.0", ..spec("economy") })).unwrap();
    let c = inspect(
        zip(&Spec { version: "2.0.0", extra: "[permissions]\nplayers = [\"chat\"]\n", ..spec("economy") }),
        "e",
    )
    .unwrap();
    let mut grants = BTreeMap::new();
    grants.insert(
        "economy".to_string(),
        vec!["players:chat".to_string(), "native".to_string(), "players:chat".to_string()],
    );
    write_packages(&dir, std::slice::from_ref(&c), &grants, false).unwrap();
    let names: BTreeSet<String> =
        fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, ["economy-2.0.0.bhmod", FILE_NAME].iter().map(|s| s.to_string()).collect());
    let cfg = read_config(&dir).unwrap().get("economy");
    assert_eq!((cfg.enabled, cfg.grant), (false, vec!["players:chat".to_string()]));
    // Same version again: overwritten in place.
    write_packages(&dir, std::slice::from_ref(&c), &BTreeMap::new(), true).unwrap();
    assert!(read_config(&dir).unwrap().mods.is_empty());
    assert_eq!(installed_versions(&dir)["economy"], "2.0.0");
    fs::write(dir.join(FILE_NAME), "broken =").unwrap();
    assert_eq!(write_packages(&dir, &[c], &BTreeMap::new(), true).unwrap_err().code, "config");
}

#[test]
fn errors_serialize_compactly() {
    let e = ModError::with("download", "x");
    assert_eq!(serde_json::to_string(&e).unwrap(), r#"{"code":"download","detail":"x"}"#);
    assert_eq!(find_package(Path::new("/nonexistent"), "a").unwrap_err().code, "missing");
    assert_eq!(child(Path::new("/nonexistent"), "a").unwrap_err().code, "missing");
    assert_eq!(file_name(Path::new("/")), "");
    assert_eq!(dir_size(Path::new("/nonexistent")), 0);
}

#[test]
fn the_development_fixture_is_a_valid_index() {
    let idx = parse_index(include_bytes!("../../fixtures/mods-index.json")).unwrap();
    let b = browse(&idx, &HashMap::new(), false, None);
    assert_eq!(b.mods.len(), 5);
    assert!(b.mods.iter().any(|m| m.native));
    let p = plan_index(&idx, &HashMap::new(), "auction-house", None).unwrap();
    assert_eq!(p.items[1].id, "economy-core");
    assert_eq!(p.items[1].version, "2.3.1");
}
