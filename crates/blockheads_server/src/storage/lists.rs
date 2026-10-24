//! Text access lists (black/white/admin/mod + cloud/curse lists): parsing and rewrite rules.

use crate::foundation::nsstring;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Admin list header (182 bytes).
pub const ADMIN_HEADER: &str = "Usernames in this file will be able to issue all server commands via chat, and pick up and remove all objects. One username per line, non case sensitive. This first line is ignored.\n";
/// Mod list header (147 bytes).
pub const MOD_HEADER: &str = "Usernames in this file will be able to issue most server commands via chat. One username per line, non case sensitive. This first line is ignored.\n";
/// Blacklist header (153 bytes).
pub const BLACK_HEADER: &str = "Usernames or IP addresses in this file will not be able to connect. One username or IP address per line, non case sensitive. This first line is ignored.\n";
/// Whitelist header (154 bytes).
pub const WHITE_HEADER: &str = "Only usernames or IP addresses in this file will be able to connect. One username or IP address per line, non case sensitive. This first line is ignored.\n";

/// ListType 0.
pub const LIST_TYPE_BLACKLIST: i32 = 0;
/// ListType 1.
pub const LIST_TYPE_WHITELIST: i32 = 1;
/// ListType 2.
pub const LIST_TYPE_ADMINLIST: i32 = 2;
/// ListType 3.
pub const LIST_TYPE_MODLIST: i32 = 3;

/// File base name for a list type; unknown -> "blacklist".
pub fn file_type_name(list_type: i32) -> &'static str {
    match list_type {
        0 => "blacklist",
        1 => "whitelist",
        2 => "adminlist",
        3 => "modlist",
        _ => "blacklist",
    }
}

/// Header for a list type; unknown (and 2) -> admin header.
pub fn header_for(list_type: i32) -> &'static str {
    match list_type {
        0 => BLACK_HEADER,
        1 => WHITE_HEADER,
        3 => MOD_HEADER,
        _ => ADMIN_HEADER,
    }
}

/// `<docs>/saves/<id>/<file_type_name>.txt`.
pub fn list_file_path(docs: &Path, save_id: &str, list_type: i32) -> PathBuf {
    docs.join("saves").join(save_id).join(format!("{}.txt", file_type_name(list_type)))
}

/// In-memory lists; `None` == ObjC nil.
#[derive(Default, Clone, Debug)]
pub struct AccessLists {
    pub black: Option<Vec<String>>,
    pub white: Option<Vec<String>>,
    pub admin: Option<Vec<String>>,
    pub cloud_admin: Option<Vec<String>>,
    pub cloud_invisible_admin: Option<Vec<String>>,
    pub curse: Option<Vec<String>>,
    pub moderator: Option<Vec<String>>,
}

/// Parsing rule per file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListRule {
    Black,
    White,
    Plain,
    Cloud,
    Curse,
}

/// Read a file as UTF-8; any failure -> None.
pub fn read_list_file(path: &Path) -> Option<String> {
    if !path.exists() {
        return None;
    }
    std::fs::read(path).ok().and_then(|b| String::from_utf8(b).ok())
}

/// Parse one file body.
pub fn parse_list(contents: Option<&str>, rule: ListRule) -> Option<Vec<String>> {
    let c = contents?;
    if c.is_empty() {
        return None;
    }
    let mut arr: Vec<String> = c.split('\n').map(str::to_string).collect();
    let len0 = |a: &Vec<String>| a.first().map(|s| nsstring::length(s));
    match rule {
        ListRule::Black => {
            if len0(&arr).is_some_and(|l| l > 64) {
                arr.remove(0);
            }
            if len0(&arr) == Some(0) {
                arr.remove(0);
            }
            if arr.is_empty() {
                None
            } else {
                Some(arr)
            }
        }
        ListRule::White => {
            if len0(&arr).is_some_and(|l| l > 64) {
                arr.remove(0);
            }
            if arr.iter().any(|s| !s.is_empty()) {
                Some(arr)
            } else {
                None
            }
        }
        ListRule::Plain => {
            if len0(&arr).is_some_and(|l| l > 64) {
                arr.remove(0);
            }
            if arr.is_empty() {
                None
            } else {
                Some(arr)
            }
        }
        ListRule::Cloud => {
            if len0(&arr).is_some_and(|l| !(l <= 64 && l != 0)) {
                arr.remove(0);
            }
            if arr.is_empty() {
                None
            } else {
                Some(arr)
            }
        }
        ListRule::Curse => Some(arr),
    }
}

/// File-reading half of `-[BHServer reloadLists]`.
pub fn load_all(docs: &Path, save_id: &str) -> AccessLists {
    let save = docs.join("saves").join(save_id);
    let rd = |p: PathBuf, r: ListRule| parse_list(read_list_file(&p).as_deref(), r);
    let black = rd(save.join("blacklist.txt"), ListRule::Black);
    let white = rd(save.join("whitelist.txt"), ListRule::White);
    let admin = rd(save.join("adminlist.txt"), ListRule::Plain);
    let cloud_admin = rd(docs.join("cloudWideOwnedAdminlist.txt"), ListRule::Cloud);
    let cloud_invisible_admin = rd(docs.join("cloudWideInvisibleAdminlist.txt"), ListRule::Cloud);
    let curse = rd(docs.join("curseList.txt"), ListRule::Curse);
    let moderator = rd(save.join("modlist.txt"), ListRule::Plain);
    AccessLists { black, white, admin, cloud_admin, cloud_invisible_admin, curse, moderator }
}

/// Atomic UTF-8 write (temp file in same dir + rename).
pub fn write_text_atomic(path: &Path, text: &str) -> bool {
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{}.{}.tmp", name, std::process::id()));
    let r = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r.is_ok()
}

/// File part of `clearList:`.
pub fn write_header_only(docs: &Path, save_id: &str, list_type: i32) -> bool {
    write_text_atomic(&list_file_path(docs, save_id, list_type), header_for(list_type))
}

/// New world: always overwrite blacklist/whitelist with headers.
pub fn create_new_world_lists(docs: &Path, world_id: &str) {
    let _ = std::fs::create_dir_all(docs.join("saves").join(world_id));
    write_text_atomic(&list_file_path(docs, world_id, LIST_TYPE_BLACKLIST), BLACK_HEADER);
    write_text_atomic(&list_file_path(docs, world_id, LIST_TYPE_WHITELIST), WHITE_HEADER);
}

/// Create adminlist/modlist with header only if missing.
pub fn ensure_admin_mod_files(docs: &Path, save_id: &str) {
    for t in [LIST_TYPE_ADMINLIST, LIST_TYPE_MODLIST] {
        let p = list_file_path(docs, save_id, t);
        if !p.exists() {
            write_text_atomic(&p, header_for(t));
        }
    }
}

/// Result of [`build_modified_list`].
#[derive(Clone, Debug)]
pub struct ModifyResult {
    pub new_text: String,
    pub found: bool,
    pub changed: bool,
    pub message: String,
}

/// Steps 3-6 of `modifyListForPlayerOrIP:`.
pub fn build_modified_list(
    list_type: i32,
    existing: Option<&[String]>,
    name_or_ip: &str,
    is_added: bool,
    udid: Option<&str>,
) -> ModifyResult {
    let mut text = String::from(header_for(list_type));
    let mut found = false;
    for entry in existing.unwrap_or(&[]) {
        let name = entry.split(" \\").next().unwrap_or("");
        if nsstring::safe_case_insensitive_compare(Some(name), Some(name_or_ip)) == 0 {
            found = true;
        } else if !name.is_empty() {
            text.push('\n');
            text.push_str(entry);
        }
    }
    if is_added {
        text.push('\n');
        text.push_str(name_or_ip);
        if let Some(u) = udid {
            text.push_str(" \\");
            text.push_str(u);
        }
    }
    let f = file_type_name(list_type);
    let (changed, message) = match (found, is_added) {
        (false, false) => (false, format!("{} was not on the {}", name_or_ip, f)),
        (false, true) => (true, format!("{} has been added to the {}", name_or_ip, f)),
        (true, false) => (true, format!("{} has been removed from the {}", name_or_ip, f)),
        (true, true) => (false, format!("{} was already on the {}", name_or_ip, f)),
    };
    ModifyResult { new_text: text, found, changed, message }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Option<Vec<String>> {
        Some(v.iter().map(|x| x.to_string()).collect())
    }

    #[test]
    fn header_lengths() {
        assert_eq!(ADMIN_HEADER.len(), 182);
        assert_eq!(MOD_HEADER.len(), 147);
        assert_eq!(BLACK_HEADER.len(), 153);
        assert_eq!(WHITE_HEADER.len(), 154);
        assert_eq!(file_type_name(9), "blacklist");
        assert_eq!(header_for(9), ADMIN_HEADER);
        assert_eq!((header_for(0), header_for(1), header_for(3)), (BLACK_HEADER, WHITE_HEADER, MOD_HEADER));
    }

    #[test]
    fn parse_table() {
        let h = ADMIN_HEADER;
        assert_eq!(parse_list(Some(h), ListRule::Black), None);
        assert_eq!(parse_list(Some(h), ListRule::White), None);
        assert_eq!(parse_list(Some(h), ListRule::Plain), s(&[""]));
        assert_eq!(parse_list(Some(&format!("{h}bob\n")), ListRule::Plain), s(&["bob", ""]));
        let t = format!("{h}\nbob");
        assert_eq!(parse_list(Some(&t), ListRule::Black), s(&["bob"]));
        assert_eq!(parse_list(Some(&t), ListRule::Plain), s(&["", "bob"]));
        assert_eq!(parse_list(Some("short line\nx"), ListRule::Plain), s(&["short line", "x"]));
        let astral = "\u{1F600}".repeat(33);
        assert_eq!(parse_list(Some(&format!("{astral}\nx")), ListRule::Plain), s(&["x"]));
        assert_eq!(parse_list(Some("\r"), ListRule::Plain), s(&["\r"]));
        assert_eq!(parse_list(Some(""), ListRule::Plain), None);
        assert_eq!(parse_list(None, ListRule::Curse), None);
        assert_eq!(parse_list(Some("\nx"), ListRule::Cloud), s(&["x"]));
        assert_eq!(parse_list(Some("a\nb"), ListRule::Cloud), s(&["a", "b"]));
        assert_eq!(parse_list(Some("a\n"), ListRule::Curse), s(&["a", ""]));
        let bare = h.trim_end_matches('\n');
        assert_eq!(parse_list(Some(bare), ListRule::Plain), None);
        assert_eq!(parse_list(Some(bare), ListRule::Cloud), None);
        assert_eq!(parse_list(Some(&format!("{bare}\nx")), ListRule::White), s(&["x"]));
    }

    #[test]
    fn atomic_write_in_cwd_and_failure() {
        let name = format!(".bh-lists-test-{}.txt", std::process::id());
        assert!(write_text_atomic(Path::new(&name), "hello"));
        assert_eq!(std::fs::read_to_string(&name).unwrap(), "hello");
        std::fs::remove_file(&name).unwrap();
        assert!(!write_text_atomic(Path::new("/nonexistent-bh-dir/list.txt"), "x"));
        let dir = std::env::temp_dir().join(format!("bh-lists-dir-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("target")).unwrap();
        // renaming a file over a directory fails after the temp file was written, and the temp file is removed
        assert!(!write_text_atomic(&dir.join("target"), "x"));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn modify() {
        let r = build_modified_list(0, None, "bob", true, None);
        assert_eq!(r.new_text, format!("{}\nbob", BLACK_HEADER));
        assert_eq!(r.message, "bob has been added to the blacklist");
        let ex = vec!["bob \\u1".to_string(), "".to_string(), "al".to_string()];
        let r = build_modified_list(0, Some(&ex), "BOB", false, None);
        assert!(r.found && r.changed);
        assert_eq!(r.new_text, format!("{}\nal", BLACK_HEADER));
        assert_eq!(r.message, "BOB has been removed from the blacklist");
        let r = build_modified_list(2, Some(&ex), "bob", true, None);
        assert_eq!(r.message, "bob was already on the adminlist");
        assert!(!r.changed);
        let r = build_modified_list(0, None, "x", false, None);
        assert_eq!(r.message, "x was not on the blacklist");
        let r = build_modified_list(0, None, "bob", true, Some("u"));
        assert!(r.new_text.ends_with("\nbob \\u"));
    }

    #[test]
    fn files() {
        let d = std::env::temp_dir().join(format!("bh_lists_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        create_new_world_lists(&d, "w");
        let p = list_file_path(&d, "w", 0);
        assert_eq!(std::fs::read_to_string(&p).unwrap(), BLACK_HEADER);
        std::fs::write(&p, "junk").unwrap();
        create_new_world_lists(&d, "w");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), BLACK_HEADER);
        ensure_admin_mod_files(&d, "w");
        let a = list_file_path(&d, "w", 2);
        std::fs::write(&a, "mine").unwrap();
        ensure_admin_mod_files(&d, "w");
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "mine");
        assert!(write_header_only(&d, "w", 2));
        assert_eq!(std::fs::read_to_string(&a).unwrap(), ADMIN_HEADER);
        let l = load_all(&d, "w");
        assert_eq!(l.admin, s(&[""]));
        assert_eq!(l.black, None);
        let _ = std::fs::remove_dir_all(&d);
    }
}
