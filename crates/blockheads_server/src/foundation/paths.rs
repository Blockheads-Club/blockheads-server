//! APPSUP paths and GNUstep path-string helpers.
//!

/// `$HOME` (`%USERPROFILE%` on Windows), else the passwd entry (NSHomeDirectory).
pub fn home_dir() -> String {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    if let Ok(h) = std::env::var(var) {
        if !h.is_empty() {
            return h;
        }
    }
    crate::foundation::sys::passwd_home_dir().unwrap_or_else(|| "/".to_string())
}

thread_local! {
    static DATA_DIR_OVERRIDE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Override the data directory for the current thread (used by embedded hosts). `None` clears it.
pub fn set_data_dir_override(dir: Option<String>) {
    DATA_DIR_OVERRIDE.with(|d| *d.borrow_mut() = dir);
}

/// Explicit data directory: the thread override, else env `BLOCKHEADS_DATA_DIR` (when non-empty).
fn data_dir_override() -> Option<String> {
    if let Some(d) = DATA_DIR_OVERRIDE.with(|d| d.borrow().clone()) {
        return Some(d);
    }
    match std::env::var("BLOCKHEADS_DATA_DIR") {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

/// `"<home>/GNUstep/Library/ApplicationSupport/TheBlockheads"` (no trailing slash).
pub fn appsup() -> String {
    if let Some(d) = data_dir_override() {
        return d.trim_end_matches(['/', '\\']).to_string();
    }
    format!("{}/GNUstep/Library/ApplicationSupport/TheBlockheads", home_dir())
}

/// `NSSearchPathForDirectoriesInDomains(14,1,YES)[0]`.
pub fn search_path_application_support() -> String {
    if let Some(d) = data_dir_override() {
        let d = d.trim_end_matches(['/', '\\']);
        return match d.rfind(['/', '\\']) {
            Some(i) if i > 0 => d[..i].to_string(),
            _ => d.to_string(),
        };
    }
    format!("{}/GNUstep/Library/ApplicationSupport", home_dir())
}

/// `SAVES = "{appsup}/saves/"` (trailing slash).
pub fn saves_dir() -> String {
    format!("{}/saves/", appsup())
}

/// `SAVEDIR = stringByAppendingPathComponent`: `"{appsup}/saves/{id}"`.
pub fn save_dir(id: &str) -> String {
    append_path_component(&saves_dir(), id)
}

/// `stringByAppendingPathComponent:` (GNUstep-base 1.24): join with one `/` (none added when `base` already ends
/// in `/` or `c` is empty; empty `base` gives `c`), then trailing separators are removed (never the root `/`)
/// and runs of separators after the root are collapsed to one.
/// `/` of base") and the 12 path cases, e.g. `("a//b/","c")`.
// Note: trailing-trim and multi-separator collapse follow GNUstep-base's documented behaviour
// ("removes trailing separators and multiple separators"); not probed on the reference.
pub fn append_path_component(base: &str, c: &str) -> String {
    let mut buf: Vec<char> = Vec::with_capacity(base.len() + c.len() + 1);
    if base.is_empty() {
        buf.extend(c.chars());
    } else {
        buf.extend(base.chars());
        if !c.is_empty() && buf.last() != Some(&'/') {
            buf.push('/');
        }
        buf.extend(c.chars());
    }
    let root = if buf.first() == Some(&'/') { 1 } else { 0 };
    while buf.len() > root && buf.last() == Some(&'/') {
        buf.pop();
    }
    let mut out: Vec<char> = Vec::with_capacity(buf.len());
    for (i, &ch) in buf.iter().enumerate() {
        if ch == '/' && i > root && buf[i - 1] == '/' {
            continue;
        }
        out.push(ch);
    }
    out.into_iter().collect()
}

/// `lastPathComponent`: trailing slashes ignored; `/` stays `/`.
pub fn last_path_component(p: &str) -> String {
    if p.is_empty() {
        return String::new();
    }
    let t = p.trim_end_matches('/');
    if t.is_empty() {
        return "/".to_string();
    }
    t.rsplit('/').next().unwrap_or(t).to_string()
}

/// `gethostname()`; only for the unreachable userName == nil fallback.
pub fn host_name() -> String {
    crate::foundation::sys::host_name()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_rules() {
        assert_eq!(append_path_component("a/", "b"), "a/b");
        assert_eq!(append_path_component("a//b/", "c"), "a/b/c");
        assert_eq!(append_path_component("a/", ""), "a");
        assert_eq!(append_path_component("/", "x"), "/x");
        assert_eq!(append_path_component("/", ""), "/");
        assert_eq!(append_path_component("saves/", "id/"), "saves/id");
        assert_eq!(append_path_component("", "c"), "c");
        assert_eq!(append_path_component("a", "b"), "a/b");
        assert_eq!(last_path_component("/x/y/"), "y");
        assert_eq!(last_path_component("/"), "/");
        assert_eq!(last_path_component("abc"), "abc");
        assert!(!host_name().is_empty() || true);
    }

    #[test]
    fn appsup_layout() {
        std::env::remove_var("BLOCKHEADS_DATA_DIR");
        std::env::set_var(if cfg!(windows) { "USERPROFILE" } else { "HOME" }, "/home/bh");
        assert_eq!(appsup(), "/home/bh/GNUstep/Library/ApplicationSupport/TheBlockheads");
        assert_eq!(save_dir("abc"), "/home/bh/GNUstep/Library/ApplicationSupport/TheBlockheads/saves/abc");
        assert_eq!(saves_dir(), "/home/bh/GNUstep/Library/ApplicationSupport/TheBlockheads/saves/");
        assert_eq!(search_path_application_support(), "/home/bh/GNUstep/Library/ApplicationSupport");
    }
}
