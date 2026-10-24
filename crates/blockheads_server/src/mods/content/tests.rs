use super::*;
use bhmod_core::config::FILE_NAME;

fn mod_dir(root: &Path, id: &str, cosmetics: &str) {
    mod_dir_with(root, id, &format!("cosmetics = [{cosmetics}]"));
}

fn mod_dir_with(root: &Path, id: &str, content: &str) {
    let d = root.join(id);
    std::fs::create_dir_all(d.join("client")).unwrap();
    std::fs::write(d.join("client/main.luau"), "-- nothing\n").unwrap();
    std::fs::write(
        d.join("mod.toml"),
        format!(
            "schema = 1\nid = \"{id}\"\nversion = \"1.0.0\"\nname = {{ en = \"X\" }}\nauthors = [\"me\"]\nlicense = \"MIT\"\n\
             api = \"^0.1\"\n[sides]\nclient = true\n[entry]\nclient = {{ runtime = \"lua\", path = \"client/main.luau\" }}\n\
             [content]\n{content}\n"
        ),
    )
    .unwrap();
    std::fs::create_dir_all(d.join("assets")).unwrap();
    std::fs::write(d.join("assets/a.png"), tiny_png()).unwrap();
}

/// A 1x1 PNG made here (original, not game art).
fn tiny_png() -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png_encode::Encoder::new(&mut out);
    enc.write();
    out
}

mod png_encode {
    pub struct Encoder<'a>(&'a mut Vec<u8>);
    impl<'a> Encoder<'a> {
        pub fn new(o: &'a mut Vec<u8>) -> Self {
            Encoder(o)
        }
        pub fn write(&mut self) {
            // Signature + IHDR(1x1, 8 bit RGBA) + IDAT(stored zlib of one filter byte + 4 bytes) + IEND.
            self.0.extend([0x89, b'P', b'N', b'G', 13, 10, 26, 10]);
            self.chunk(b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0]);
            let raw = [0u8, 200, 30, 30, 255];
            let mut z = vec![0x78, 0x01, 0x01, raw.len() as u8, 0, !(raw.len() as u8), 0xff];
            z.extend(raw);
            let (mut a, mut b) = (1u32, 0u32);
            for x in raw {
                a = (a + u32::from(x)) % 65521;
                b = (b + a) % 65521;
            }
            z.extend(((b << 16) | a).to_be_bytes());
            self.chunk(b"IDAT", &z);
            self.chunk(b"IEND", &[]);
        }
        fn chunk(&mut self, kind: &[u8; 4], data: &[u8]) {
            self.0.extend((data.len() as u32).to_be_bytes());
            let mut body = kind.to_vec();
            body.extend(data);
            self.0.extend(&body);
            let mut crc = 0xffff_ffffu32;
            for byte in &body {
                crc ^= u32::from(*byte);
                for _ in 0..8 {
                    crc = if crc & 1 == 1 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
                }
            }
            self.0.extend((!crc).to_be_bytes());
        }
    }
}

const TWO: &str = "{ key = \"hats:crown\", slot = \"hat\", texture = \"assets/a.png\" }, \
                   { key = \"hats:ember\", slot = \"hair\", texture = \"assets/a.png\" }";

fn run(root: &Path, offered: &[&str], sidecar: &Path) -> (Content, Vec<String>) {
    let mut log = Vec::new();
    let offered: Vec<String> = offered.iter().map(|s| s.to_string()).collect();
    (build(root, &offered, sidecar, &mut |m| log.push(m)), log)
}

#[test]
fn allocates_persists_and_keeps_ids() {
    let tmp = tempfile::tempdir().unwrap();
    let mods = tmp.path().join("mods");
    mod_dir(&mods, "hats", TWO);
    let side = tmp.path().join("world/mods/registry.toml");
    let (c, log) = run(&mods, &["hats"], &side);
    assert!(log.is_empty(), "{log:?}");
    assert_eq!(c.cosmetics, BTreeMap::from([(1, "hat".to_string()), (2, "hair".to_string())]));
    assert!(c.has_cosmetics());
    assert_eq!(c.id(Kind::Cosmetics, "hats:ember"), Some(2));
    assert_eq!(c.id(Kind::Items, "hats:ember"), None);
    assert_eq!(c.wire["cosmetics"]["hats:crown"], 1);
    let text = std::fs::read_to_string(&side).unwrap();
    assert!(text.contains("\"hats:crown\" = 1"));
    // A second run changes nothing in the file; a new cosmetic gets the next id; a removed one keeps its id.
    let before = std::fs::metadata(&side).unwrap().modified().unwrap();
    let (again, _) = run(&mods, &["hats"], &side);
    assert_eq!(again, c);
    assert_eq!(std::fs::metadata(&side).unwrap().modified().unwrap(), before);
    mod_dir(&mods, "hats", "{ key = \"hats:new\", slot = \"hat\", texture = \"assets/a.png\" }");
    let (c2, _) = run(&mods, &["hats"], &side);
    assert_eq!(c2.cosmetics, BTreeMap::from([(3, "hat".to_string())]));
    mod_dir(&mods, "hats", TWO);
    assert_eq!(run(&mods, &["hats"], &side).0, c);
}

#[test]
fn nothing_declared_touches_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let mods = tmp.path().join("mods");
    mod_dir(&mods, "hats", TWO);
    let side = tmp.path().join("world/mods/registry.toml");
    // Not offered (or no mods at all): no file, empty content.
    assert_eq!(run(&mods, &[], &side).0, Content::default());
    assert_eq!(run(tmp.path().join("none").as_path(), &["hats"], &side).0, Content::default());
    assert!(!side.exists());
    assert!(!Content::default().has_cosmetics());
    let _ = FILE_NAME;
}

#[test]
fn a_broken_sidecar_turns_content_off_and_stays_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let mods = tmp.path().join("mods");
    mod_dir(&mods, "hats", TWO);
    let side = tmp.path().join("world/mods/registry.toml");
    std::fs::create_dir_all(side.parent().unwrap()).unwrap();
    std::fs::write(&side, "this is = not valid [").unwrap();
    let (c, log) = run(&mods, &["hats"], &side);
    assert_eq!(c, Content::default());
    assert!(log[0].contains("content is off") && log[0].contains("untouched"), "{log:?}");
    assert_eq!(std::fs::read_to_string(&side).unwrap(), "this is = not valid [");
    // The sidecar cannot be written (a directory is in the way of the temporary file).
    let side2 = tmp.path().join("w2/registry.toml");
    std::fs::create_dir_all(tmp.path().join("w2/registry.toml.tmp")).unwrap();
    let (c, log) = run(&mods, &["hats"], &side2);
    assert_eq!(c, Content::default());
    assert!(log[0].contains("content is off"));
    // No free ids left for a kind.
    let side3 = tmp.path().join("w3/registry.toml");
    std::fs::create_dir_all(side3.parent().unwrap()).unwrap();
    let full: String = (1..=u32::from(u16::MAX)).map(|i| format!("\"gone:k{i}\" = {i}\n")).collect();
    std::fs::write(&side3, format!("schema = 1\n[cosmetics]\n{full}")).unwrap();
    let (c, log) = run(&mods, &["hats"], &side3);
    assert_eq!(c, Content::default());
    assert!(log[0].contains("no free cosmetics id"), "{log:?}");
}

const DECOR: &str = "items = [{ key = \"decor:lamp\", texture = \"assets/a.png\" }, \
                     { key = \"decor:plaque\", texture = \"assets/a.png\" }]\n\
                     blocks = [{ key = \"decor:brick\", texture = \"assets/a.png\", base = \"red-bricks\", drops = \"decor:plaque\" },\n\
                     { key = \"decor:plain\", texture = \"assets/a.png\", base = \"cobblestone\" },\n\
                     { key = \"decor:nobase\", texture = \"assets/a.png\" }]";

#[test]
fn items_and_variant_blocks_are_registered() {
    let tmp = tempfile::tempdir().unwrap();
    let mods = tmp.path().join("mods");
    mod_dir_with(&mods, "decor", DECOR);
    let side = tmp.path().join("world/mods/registry.toml");
    let (c, log) = run(&mods, &["decor"], &side);
    assert!(log.is_empty(), "{log:?}");
    assert!(!c.has_cosmetics());
    assert_eq!(c.items, BTreeSet::from([1107, 1108]));
    // Blocks without a base are ids but no variants; drops resolve to the item id.
    assert_eq!(c.id(Kind::Blocks, "decor:nobase"), Some(2));
    assert_eq!(
        c.variants,
        BTreeMap::from([(1, VariantInfo { base: 11, drops: Some(1108) }), (3, VariantInfo { base: 10, drops: None }),])
    );
    assert_eq!(c.id(Kind::Items, "decor:lamp"), Some(1107));
    // The tables the game rules see: variants only when they are switched on.
    assert_eq!(c.tables(false).items, BTreeSet::from([1107, 1108]));
    assert!(c.tables(false).variants.is_empty());
    assert_eq!(c.tables(true).variants[&1], VariantInfo { base: 11, drops: Some(1108) });
    // Ids that do not fit the game's types are left out of the tables.
    let wide = Content {
        items: BTreeSet::from([1107, u64::from(u32::MAX) + 1]),
        variants: BTreeMap::from([(70_000, VariantInfo { base: 10, drops: None })]),
        ..Content::default()
    };
    assert_eq!(wide.tables(true), ContentTables { items: BTreeSet::from([1107]), variants: BTreeMap::new() });
    assert_eq!(Content::default().tables(true), ContentTables::default());
}
