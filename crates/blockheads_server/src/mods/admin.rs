//! The `/mods` admin command (not in the original server): list the mods, hot-reload them from disk and show their
//! time accounting. Only admins and the console may use it; it is handled by the mod host before the built-in
//! commands and before the mods' own, so a mod cannot shadow it.
//!
//! ```text
//! /mods                      list the mods and their state
//! /mods reload [id]          reload the changed mods (or one mod, changed or not) without restarting the world
//! /mods timings [id]         calls, total/avg/max time per handler kind, budget hits and strikes
//! /mods timings reset [id]   clear the accounting
//! /mods settings <mod>       a mod's settings (bhmod-core `ModHost::settings_command`)
//! /mods set <mod> <k> <v>    change one
//! ```

use bhmod_core::host::{ModHost, ModState, ReloadOutcome};
use bhmod_core::timings::format_lines;

/// The usage line.
pub const USAGE: &str =
    "Usage: /mods [list] | reload [id] | timings [id] | timings reset [id] | settings <mod> | set <mod> <key> <value>";

/// What a `/mods` command printed and whether it changed the set of running mods (the offered client packages are
/// then built again).
#[derive(Debug, PartialEq, Eq)]
pub struct Reply {
    /// The lines for the issuer.
    pub lines: Vec<String>,
    /// True when some mod was reloaded, started or stopped.
    pub changed: bool,
}

impl Reply {
    fn text(lines: Vec<String>) -> Reply {
        Reply { lines, changed: false }
    }

    /// The answer while a mod call is running (the host cannot be entered).
    pub fn busy() -> Reply {
        Reply::text(vec!["The mods are busy, try again.".to_string()])
    }
}

/// Runs `/mods <args>` on the host.
pub fn run(host: &mut ModHost, args: &[String]) -> Reply {
    let arg = |i: usize| args.get(i).map(String::as_str);
    match (arg(0), arg(1), arg(2)) {
        (None | Some("list"), None, None) => Reply::text(list(host)),
        (Some("reload"), id, None) => reload(host, id),
        (Some("timings"), None, None) => Reply::text(format_lines(&host.timings(None))),
        (Some("timings"), Some("reset"), id) => Reply::text(vec![reset(host, id)]),
        (Some("timings"), Some(id), None) => Reply::text(timings_of(host, id)),
        _ => Reply::text(vec![USAGE.to_string()]),
    }
}

fn list(host: &ModHost) -> Vec<String> {
    let mods = host.mods();
    if mods.is_empty() {
        return vec!["No mods are loaded.".to_string()];
    }
    let mut out = vec![format!("{} mod(s):", mods.len())];
    out.extend(mods.iter().map(|m| {
        let state = match &m.state {
            ModState::Loaded => "loaded".to_string(),
            ModState::Enabled => "enabled".to_string(),
            ModState::Disabled(why) => format!("disabled: {why}"),
        };
        format!("  {} {} [{state}]", m.id, m.version)
    }));
    out
}

fn reload(host: &mut ModHost, id: Option<&str>) -> Reply {
    let results = host.reload(id);
    let changed = results.iter().any(|r| {
        matches!(r.outcome, ReloadOutcome::Reloaded(..) | ReloadOutcome::Added(_) | ReloadOutcome::Removed(_))
    });
    let unchanged = results.iter().filter(|r| r.outcome == ReloadOutcome::Unchanged).count();
    let mut lines: Vec<String> =
        results.iter().filter(|r| r.outcome != ReloadOutcome::Unchanged).map(ToString::to_string).collect();
    if lines.is_empty() {
        lines.push(format!("No mod changed on disk ({unchanged} checked)."));
    }
    Reply { lines, changed }
}

fn timings_of(host: &ModHost, id: &str) -> Vec<String> {
    let t = host.timings(Some(id));
    if t.is_empty() {
        return vec![format!("No mod {id} is loaded.")];
    }
    format_lines(&t)
}

fn reset(host: &mut ModHost, id: Option<&str>) -> String {
    match (host.reset_timings(id), id) {
        (true, Some(id)) => format!("Timings of {id} cleared."),
        (true, None) => "Timings cleared.".to_string(),
        (false, id) => format!("No mod {} is loaded.", id.unwrap_or_default()),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::Path;
    use std::rc::Rc;

    use bhmod_core::types::Side;
    use bhmod_core::HostOptions;

    use crate::mods::server_host::Bridge;

    use super::*;

    /// Writes an unpacked Luau server mod.
    pub(crate) fn write_lua_mod(mods: &Path, id: &str, version: &str, main: &str) {
        let d = mods.join(id);
        std::fs::create_dir_all(d.join("server")).unwrap();
        let toml = format!(
            "schema = 1\nid = \"{id}\"\nversion = \"{version}\"\nname = {{ en = \"N\" }}\nauthors = [\"a\"]\nlicense = \"MIT\"\n\
             api = \"^0.1\"\n[sides]\nserver = true\nclient = false\n[entry]\nserver = {{ runtime = \"lua\", path = \"server/main.luau\" }}\n"
        );
        std::fs::write(d.join("mod.toml"), toml).unwrap();
        std::fs::write(d.join("server/main.luau"), main).unwrap();
    }

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_string).collect()
    }

    fn lines(host: &mut ModHost, s: &str) -> Vec<String> {
        run(host, &args(s)).lines
    }

    const MAIN: &str =
        "return { on_enable = function() bh.commands.register({ name = 'hi' }, function() return 'v' end) end }";

    #[test]
    fn list_reload_and_timings() {
        let tmp = tempfile::tempdir().unwrap();
        let mut host = ModHost::load(
            HostOptions::new(tmp.path(), Side::Server),
            Rc::new(Bridge { srv: std::rc::Weak::new(), net: Rc::default(), voice: Rc::default() }),
        );
        assert_eq!(lines(&mut host, ""), ["No mods are loaded."]);
        assert_eq!(lines(&mut host, "timings"), ["No mods are loaded."]);
        write_lua_mod(tmp.path(), "alpha", "1.0.0", MAIN);
        write_lua_mod(tmp.path(), "broken", "1.0.0", "this is not luau");
        // A reload starts what appeared on disk (the world is not loaded: nothing runs yet).
        let r = run(&mut host, &args("reload"));
        assert!(r.changed && r.lines.iter().any(|l| l == "alpha: loaded (1.0.0)"), "{r:?}");
        assert_eq!(lines(&mut host, "list")[1], "  alpha 1.0.0 [loaded]");
        host.enable("W", "w");
        let l = lines(&mut host, "list");
        assert_eq!(l[..2], ["2 mod(s):", "  alpha 1.0.0 [enabled]"]);
        assert!(l[2].starts_with("  broken 1.0.0 [disabled: "), "{l:?}");
        // Nothing changed on disk (the broken mod stays what it is).
        let r = run(&mut host, &args("reload"));
        assert!(!r.changed, "{r:?}");
        assert_eq!(r.lines, ["No mod changed on disk (2 checked)."]);
        write_lua_mod(tmp.path(), "alpha", "1.1.0", MAIN);
        let r = run(&mut host, &args("reload alpha"));
        assert!(r.changed);
        assert_eq!(r.lines, ["alpha: reloaded (1.0.0 -> 1.1.0)"]);
        assert_eq!(lines(&mut host, "reload nope"), ["nope: not reloaded: no package for nope in the mods directory"]);
        assert!(!run(&mut host, &args("reload nope")).changed);
        // Timings.
        let all = lines(&mut host, "timings");
        assert!(all[0].starts_with("Mod timings over the last 60 s"), "{all:?}");
        assert!(all.iter().any(|l| l.starts_with("alpha [enabled]: ")), "{all:?}");
        assert!(lines(&mut host, "timings alpha")[1].starts_with("alpha [enabled]: "));
        assert_eq!(lines(&mut host, "timings ghost"), ["No mod ghost is loaded."]);
        assert_eq!(lines(&mut host, "timings reset"), ["Timings cleared."]);
        assert_eq!(lines(&mut host, "timings reset alpha"), ["Timings of alpha cleared."]);
        assert_eq!(lines(&mut host, "timings reset ghost"), ["No mod ghost is loaded."]);
        // Anything else is the usage.
        for bad in ["what", "reload a b", "timings a b", "list x"] {
            assert_eq!(lines(&mut host, bad), [USAGE], "{bad}");
        }
        assert_eq!(Reply::busy().lines, ["The mods are busy, try again."]);
    }
}
