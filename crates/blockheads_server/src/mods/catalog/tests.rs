use std::fs;

use bhmod_manifest::pack::{sha256_hex, write_zip};
use bhmod_proto::hex;

use super::*;

/// Writes an unpacked mod. `server`/`client`: which halves it has.
fn write_mod(root: &Path, id: &str, server: bool, client: bool, extra: &str) -> PathBuf {
    let d = root.join(id);
    let mut toml = format!(
        "schema = 1\nid = \"{id}\"\nversion = \"1.0.0\"\nname = {{ en = \"{id}\" }}\nauthors = [\"a\"]\nlicense = \"MIT\"\napi = \"^0.1\"\n{extra}[sides]\nserver = {server}\nclient = {client}\n[entry]\n"
    );
    for (on, side) in [(server, "server"), (client, "client")] {
        if on {
            fs::create_dir_all(d.join(side)).unwrap();
            fs::write(d.join(format!("{side}/main.luau")), "return {}").unwrap();
            toml.push_str(&format!("{side} = {{ runtime = \"lua\", path = \"{side}/main.luau\" }}\n"));
        }
    }
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("mod.toml"), toml).unwrap();
    d
}

fn zip_of(dir: &Path) -> Vec<u8> {
    let mut src = DirSource::open(dir).unwrap();
    write_zip(&mut src, Cursor::new(Vec::new())).unwrap().into_inner()
}

#[test]
fn builds_the_offer() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let mods = tmp.path();
    // A client+server mod as a zip.
    let both = write_mod(src.path(), "both", true, true, "[permissions]\nworld = \"write\"\nplayers = [\"chat\", \"teleport\", \"kick\", \"ban\", \"inventory\", \"health\"]\nchannels = true\n");
    let zip = zip_of(&both);
    fs::write(mods.join("both-1.0.0.bhmod"), &zip).unwrap();
    // A client-only optional mod, unpacked.
    let opt = write_mod(mods, "cosmetic", false, true, "");
    let toml = fs::read_to_string(opt.join("mod.toml"))
        .unwrap()
        .replace("client = true", "client = true\nclient-required = false");
    fs::write(opt.join("mod.toml"), toml).unwrap();
    // Server-only, loaded and not loaded; a client+server mod whose server half did not load; a native client mod
    // (listed, never served); a disabled one.
    write_mod(mods, "srv", true, false, "[permissions]\nworld = \"read\"\n");
    write_mod(mods, "srv-off", true, false, "");
    write_mod(mods, "half", true, true, "");
    write_mod(mods, "nat", false, true, "[permissions]\nnative = true\n");
    write_mod(mods, "zzz-disabled", false, true, "");
    fs::write(mods.join("mods.toml"), "[zzz-disabled]\nenabled = false\n").unwrap();

    let loaded = vec!["both".to_string(), "srv".to_string()];
    let mut logs = Vec::new();
    let c = build(mods, &loaded, &mut |m| logs.push(m));
    let ids: Vec<&str> = c.entries.iter().map(|e| e.info.id.as_str()).collect();
    assert_eq!(ids, ["both", "cosmetic", "nat", "srv"]);
    assert_eq!(logs.len(), 2, "{logs:?}");
    assert!(logs[0].contains("half") && logs[0].contains("server half"));
    assert!(logs[1].contains("nat") && logs[1].contains("without bytes") && logs[1].contains("index"));

    let b = &c.entries[0];
    assert_eq!(b.info.sides, ["server", "client"]);
    assert!(b.info.client_required && !b.info.index_listed && b.info.source_url.is_none());
    assert_eq!(hex(&b.info.sha256), sha256_hex(&zip));
    assert_eq!(b.info.size, zip.len() as u64);
    assert_eq!(b.info.permissions.world, "write");
    assert_eq!(b.info.permissions.players, ["chat", "teleport", "kick", "ban", "inventory", "health"]);
    assert!(b.info.permissions.channels);
    assert_eq!(b.read(0, 4).unwrap(), zip[..4]);
    assert!(b.read(zip.len() as u64, 1).is_err());
    assert!(matches!(b.bytes, Some(Bytes::File(_))));

    let o = &c.entries[1];
    assert_eq!(o.info.sides, ["client"]);
    assert!(!o.info.client_required);
    let packed = zip_of(&opt);
    assert_eq!(hex(&o.info.sha256), sha256_hex(&packed));
    assert_eq!(o.read(0, packed.len()).unwrap(), packed);
    assert!(o.read(1, packed.len()).is_err());

    // Native: a client mod with its hash in the list and the hash in `modsHash`, but no bytes to serve.
    let n = &c.entries[2];
    assert_eq!(n.info.sides, ["client"]);
    assert!(n.info.permissions.native && n.info.client_required && n.is_client());
    assert_eq!(hex(&n.info.sha256), sha256_hex(&zip_of(&mods.join("nat"))));
    assert!(n.bytes.is_none() && n.read(0, 1).is_err());

    let s = &c.entries[3];
    assert!(!s.is_client());
    assert_eq!(s.info.sides, ["server"]);
    assert!(s.bytes.is_none() && !s.info.client_required);
    assert_eq!(s.info.permissions.world, "read");
    assert_eq!(s.info.permissions.world, "read");

    // `both` and `nat` are client-required; the bytes of `nat` are never served.
    assert!(c.client_required);
    assert_eq!(c.mods_hash, mods_hash([("both", "1.0.0", &b.info.sha256), ("nat", "1.0.0", &n.info.sha256)]));
    assert!(c.served(&b.info.sha256).is_some() && c.served(&s.info.sha256).is_none());
    assert!(c.served(&n.info.sha256).is_none());
    assert!(!c.is_empty());
}

#[test]
fn limits_duplicates_and_unreadable_packages() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs: Vec<PathBuf> = ["aa", "bb", "cc"].iter().map(|id| write_mod(tmp.path(), id, false, true, "")).collect();
    let (pkgs, _) = discover(tmp.path());
    // Deflate output differs by a byte or so between them.
    let size = dirs.iter().map(|d| zip_of(d).len() as u64).max().unwrap();
    let min = dirs.iter().map(|d| zip_of(d).len() as u64).min().unwrap();
    let mut logs = Vec::new();
    // Duplicate ids: the first wins. Per-package and total limits.
    let mut with_dup = pkgs.clone();
    with_dup.insert(1, pkgs[0].clone());
    let c = from_packages(with_dup, &AdminConfig::default(), &[], (size, size * 2), &mut |m| logs.push(m));
    assert_eq!(c.entries.len(), 2, "{logs:?}");
    assert_eq!(logs.len(), 1);
    assert!(logs[0].contains("cc") && logs[0].contains("too large"));
    let c = from_packages(pkgs.clone(), &AdminConfig::default(), &[], (min - 1, u64::MAX), &mut |_| {});
    assert!(c.is_empty() && !c.client_required);
    assert_eq!(hex(&c.mods_hash), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    // A package that went away (directory or zip).
    let mut gone = pkgs[0].clone();
    gone.origin = tmp.path().join("missing");
    let mut gone_zip = pkgs[1].clone();
    gone_zip.origin = tmp.path().join("missing.bhmod");
    gone_zip.sha256 = Some("ab".repeat(32));
    let mut logs = Vec::new();
    let c =
        from_packages(vec![gone, gone_zip], &AdminConfig::default(), &[], (u64::MAX, u64::MAX), &mut |m| logs.push(m));
    assert!(c.is_empty());
    assert_eq!(logs.len(), 2, "{logs:?}");
}

#[test]
fn hashes_parse() {
    let h = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    assert_eq!(hex(&parse_hash(h).unwrap()), h);
    assert_eq!(parse_hash("ab"), None);
    assert_eq!(parse_hash(&"zz".repeat(32)), None);
    assert_eq!(parse_hash(&"é".repeat(32)), None);
}
