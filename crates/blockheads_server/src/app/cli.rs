//! `main` and the command line: option dispatch, validation, delegate construction and the run loop.
//!

use crate::app::delegate::{CommandLineDelegate, InitParams};
use crate::app::getopt::{stderr_sink, Getopt};
use crate::app::help::{help_text, missing_arg_text};
use crate::app::run_loop::RunLoop;
use crate::foundation::exception::{catch_ns, install_quiet_panic_hook, uncaught_exception_exit};
use crate::foundation::{clock, nslog, nsstring};
use crate::nslog;

/// `main`'s locals with the defaults of.
#[derive(Clone, Debug, PartialEq)]
pub struct CliArgs {
    pub new_name: Option<String>,
    pub load_name: Option<String>,
    pub port: Option<String>,
    pub max_players: i32,
    pub rename_from: Option<String>,
    pub rename_to: Option<String>,
    pub world_id: Option<String>,
    pub delete_world_name: Option<String>,
    pub list_worlds: bool,
    pub force_delete: bool,
    pub save_delay: i32,
    pub width_multiplier_string: Option<String>,
    pub credit: i32,
    pub cloud_salt: Option<String>,
    pub owner_name: Option<String>,
    pub privacy: Option<String>,
    pub custom_rules_json: Option<String>,
    pub convert_to_custom_rules: bool,
    pub expert_mode: bool,
    pub no_exit: bool,
}

impl Default for CliArgs {
    /// Defaults exactly as.
    fn default() -> Self {
        CliArgs {
            new_name: None,
            load_name: None,
            port: None,
            max_players: 0,
            rename_from: None,
            rename_to: None,
            world_id: None,
            delete_world_name: None,
            list_worlds: false,
            force_delete: false,
            save_delay: 10,
            width_multiplier_string: None,
            credit: -1,
            cloud_salt: None,
            owner_name: None,
            privacy: None,
            custom_rules_json: None,
            convert_to_custom_rules: false,
            expert_mode: false,
            no_exit: false,
        }
    }
}

/// Result of command line parsing. `ExitZero` = `-h`, `?` or a validation failure (message already logged).
#[derive(Debug)]
pub enum Parsed {
    Run(CliArgs),
    ExitZero,
}

/// `[NSString stringWithUTF8String:]`: every byte that does not begin a well-formed UTF-8 sequence becomes one
/// U+FFFD and decoding resumes at the next byte (NOT `String::from_utf8_lossy`).
pub fn ns_string_from_utf8_lossy_gnustep(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let (len, lo, hi): (usize, u8, u8) = match b {
            0x00..=0x7F => {
                out.push(b as char);
                i += 1;
                continue;
            }
            0xC2..=0xDF => (2, 0x80, 0xBF),
            0xE0 => (3, 0xA0, 0xBF),
            0xE1..=0xEC | 0xEE | 0xEF => (3, 0x80, 0xBF),
            0xED => (3, 0x80, 0x9F),
            0xF0 => (4, 0x90, 0xBF),
            0xF1..=0xF3 => (4, 0x80, 0xBF),
            0xF4 => (4, 0x80, 0x8F),
            _ => {
                out.push('\u{FFFD}');
                i += 1;
                continue;
            }
        };
        let ok = i + len <= bytes.len()
            && (lo..=hi).contains(&bytes[i + 1])
            && bytes[i + 2..i + len].iter().all(|c| (0x80..=0xBF).contains(c));
        if ok {
            // The ranges above guarantee a valid scalar sequence.
            if let Ok(s) = std::str::from_utf8(&bytes[i..i + len]) {
                out.push_str(s);
            }
            i += len;
        } else {
            out.push('\u{FFFD}');
            i += 1;
        }
    }
    out
}

/// Steps 2 and 3 of: getopt loop and validation; logs through `nslog!`.
pub fn parse_command_line(argv: Vec<Vec<u8>>) -> Parsed {
    let mut a = CliArgs::default();
    let mut g = Getopt::new(argv);
    loop {
        let o = g.next(&mut |m| stderr_sink(m));
        if o == -1 {
            break;
        }
        let s = || g.optarg().map(ns_string_from_utf8_lossy_gnustep);
        match o as u8 {
            b'n' => a.new_name = s(),
            b'o' => a.load_name = s(),
            b'p' => a.port = s(),
            b'm' => a.max_players = nsstring::int_value(&s().unwrap_or_default()),
            b't' => a.rename_to = s(),
            b'r' => a.rename_from = s(),
            b'd' => a.delete_world_name = s(),
            b'f' => a.force_delete = true,
            b'l' => {
                nslog!("Listing worlds:");
                a.list_worlds = true;
            }
            b'e' => a.expert_mode = true,
            b'N' => a.no_exit = true,
            b'V' => a.convert_to_custom_rules = true,
            b's' => a.save_delay = nsstring::int_value(&s().unwrap_or_default()),
            b'w' => a.width_multiplier_string = s(),
            b'i' => a.world_id = s(),
            b'c' => a.credit = nsstring::int_value(&s().unwrap_or_default()),
            b'k' => a.cloud_salt = s(),
            b'u' => a.owner_name = s(),
            b'P' => a.privacy = s(),
            b'C' => a.custom_rules_json = s(),
            b'h' => {
                nslog!("{}", help_text());
                return Parsed::ExitZero;
            }
            // `?` (the only other value `Getopt` returns): the option error, with the per-option message.
            _ => {
                if let Some(t) = missing_arg_text(g.optopt()) {
                    nslog!("{}", t);
                }
                return Parsed::ExitZero;
            }
        }
    }
    if a.rename_from.is_some() && a.rename_to.is_none() {
        nslog!("--rename-to must be used with --rename-from, or be used while loading a world.");
        return Parsed::ExitZero;
    }
    if a.width_multiplier_string.is_some() && a.new_name.is_none() {
        nslog!("--world_width must be used with --new.");
        return Parsed::ExitZero;
    }
    if a.custom_rules_json.is_some() && a.new_name.is_none() {
        nslog!("--custom-rules must be used with --new.");
        return Parsed::ExitZero;
    }
    if a.new_name.is_some() && a.width_multiplier_string.is_some() && width_multiplier(&a).is_none() {
        nslog!("--world_width must be either 1, 4, or 16. example: --new 'NEW WORLD' --world_width 4");
        return Parsed::ExitZero;
    }
    if let Some(p) = &a.privacy {
        let ok = ["public", "searchable", "private"].iter().any(|k| nsstring::case_insensitive_compare(p, k) == 0);
        if !ok {
            nslog!("--privacy must be followed by public,searchable or private.");
            return Parsed::ExitZero;
        }
    }
    Parsed::Run(a)
}

/// Width string to multiplier (116 = 1/16, 104 = 1/4); `None` when not recognised.
fn width_multiplier(a: &CliArgs) -> Option<i32> {
    match a.width_multiplier_string.as_deref()? {
        "1/16" | "116" => Some(116),
        "1/4" | "104" => Some(104),
        "4" | "4x" => Some(4),
        "16" | "16x" => Some(16),
        "1" | "1x" => Some(1),
        _ => None,
    }
}

/// `main` steps 1 to 6; never returns.
pub fn main_entry() -> ! {
    crate::rng::trace::init_from_env();
    clock::init_from_env();
    install_quiet_panic_hook();
    let argv: Vec<Vec<u8>> = std::env::args_os().map(|a| crate::foundation::sys::os_str_bytes(&a)).collect();
    let argv0 = argv.first().map(|a| String::from_utf8_lossy(a).into_owned()).unwrap_or_default();
    nslog::set_process_name(&argv0);

    let a = match parse_command_line(argv) {
        Parsed::Run(a) => a,
        Parsed::ExitZero => std::process::exit(0),
    };
    // Validation guarantees the width string is recognised whenever it is used (only with --new).
    let width = if a.new_name.is_some() && a.width_multiplier_string.is_some() {
        width_multiplier(&a).unwrap_or(-1)
    } else {
        -1
    };
    let p = InitParams {
        new_name: a.new_name.as_deref().map(nsstring::uppercase_string),
        load_name: a.load_name,
        world_id: a.world_id,
        port: a.port,
        max_players: a.max_players,
        rename_from_name: a.rename_from,
        rename_to_name: a.rename_to.as_deref().map(nsstring::uppercase_string),
        delete_world_name: a.delete_world_name,
        list_worlds: a.list_worlds,
        force_delete: a.force_delete,
        save_delay: a.save_delay,
        width_multiplier: width,
        credit: a.credit,
        cloud_salt: a.cloud_salt,
        owner_name: a.owner_name,
        privacy: a.privacy,
        custom_rules_json: a.custom_rules_json,
        convert_to_custom_rules: a.convert_to_custom_rules,
        expert_mode: a.expert_mode,
        no_exit: a.no_exit,
    };
    // `init` and the run loop have no TryCatchHandler here: an escaping NSException is fatal (exit 1, no prefix).
    let r = catch_ns(move || {
        let delegate = CommandLineDelegate::init_with_new_world_name(p);
        if let Some(d) = &delegate {
            let rl = RunLoop::current();
            while !d.done() && rl.run_mode_default_before_distant_future() {}
        }
        delegate.is_some()
    });
    if let Err(ex) = r {
        uncaught_exception_exit(&ex, &argv0);
    }
    std::process::exit(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decoder vectors of (argv decoding).
    #[test]
    fn argv_decoder() {
        let f = '\u{FFFD}';
        let d = ns_string_from_utf8_lossy_gnustep;
        assert_eq!(d(b"\xe2\x82"), format!("{f}{f}"));
        assert_eq!(d(b"\xc3("), format!("{f}("));
        assert_eq!(d(b"\xf0\x9f\x98"), format!("{f}{f}{f}"));
        assert_eq!(d(b"\xed\xa0\x80"), format!("{f}{f}{f}"));
        assert_eq!(d(b"\xc0\x80"), format!("{f}{f}"));
        assert_eq!(d(b"\xf8\x88\x80\x80\x80"), f.to_string().repeat(5));
        assert_eq!(d(b"a\xffb"), format!("a{f}b"));
        assert_eq!(d("d\u{fc}nya".as_bytes()), "d\u{fc}nya");
        assert_eq!(d("\u{1F600}".as_bytes()), "\u{1F600}");
    }

    /// Defaults of main's locals.
    #[test]
    fn defaults() {
        let a = CliArgs::default();
        assert_eq!((a.save_delay, a.credit, a.max_players), (10, -1, 0));
    }

    fn parse(args: &[&str]) -> Parsed {
        let mut argv = vec![b"bh".to_vec()];
        argv.extend(args.iter().map(|a| a.as_bytes().to_vec()));
        parse_command_line(argv)
    }

    fn run(args: &[&str]) -> CliArgs {
        let Parsed::Run(a) = parse(args) else { panic!("expected a run for {args:?}") };
        a
    }

    fn exits(args: &[&str]) -> bool {
        matches!(parse(args), Parsed::ExitZero)
    }

    /// Every option fills its own field; non-numeric values parse like `intValue`.
    #[test]
    fn every_option_fills_its_field() {
        let f = '\u{FFFD}';
        let d = ns_string_from_utf8_lossy_gnustep;
        assert_eq!(d(b"\xe0\xa0\x80\xf1\x80\x80\x80"), "\u{800}\u{40000}");
        assert_eq!(d(b"\xe0\x80\x80"), format!("{f}{f}{f}"));
        assert_eq!(d(b"\xf4\x8f\xbf\xbf"), "\u{10FFFF}");
        let a = run(&[
            "-n", "world", "-o", "old", "-p", "15152", "-m", "12", "-t", "to", "-r", "from", "-d", "gone", "-f", "-l",
            "-e", "-N", "-V", "-s", "5", "-w", "4", "-i", "wid", "-c", "99", "-k", "salt", "-u", "Boss", "-P",
            "Public", "-C", "{}",
        ]);
        assert_eq!(a.new_name.as_deref(), Some("world"));
        assert_eq!(a.load_name.as_deref(), Some("old"));
        assert_eq!(a.port.as_deref(), Some("15152"));
        assert_eq!(a.max_players, 12);
        assert_eq!((a.rename_to.as_deref(), a.rename_from.as_deref()), (Some("to"), Some("from")));
        assert_eq!(a.delete_world_name.as_deref(), Some("gone"));
        assert!(a.force_delete && a.list_worlds && a.expert_mode && a.no_exit && a.convert_to_custom_rules);
        assert_eq!((a.save_delay, a.credit), (5, 99));
        assert_eq!(a.width_multiplier_string.as_deref(), Some("4"));
        assert_eq!(a.world_id.as_deref(), Some("wid"));
        assert_eq!((a.cloud_salt.as_deref(), a.owner_name.as_deref()), (Some("salt"), Some("Boss")));
        assert_eq!((a.privacy.as_deref(), a.custom_rules_json.as_deref()), (Some("Public"), Some("{}")));
        assert_eq!(run(&[]), CliArgs::default());
        assert_eq!(run(&["--new", "w", "--world_width", "1/16"]).width_multiplier_string.as_deref(), Some("1/16"));
    }

    #[test]
    fn help_and_option_errors_exit_cleanly() {
        assert!(exits(&["-h"]));
        assert!(exits(&["--nosuch"]));
        assert!(exits(&["-n"])); // missing argument: the per-option message
        assert!(exits(&["-d"])); // `d` has no message
    }

    #[test]
    fn validation_failures_exit_cleanly() {
        assert!(exits(&["-r", "from"]));
        assert!(exits(&["-w", "4"]));
        assert!(exits(&["-C", "{}"]));
        assert!(exits(&["-n", "w", "-w", "3"]));
        assert!(exits(&["-P", "friends"]));
        for p in ["PUBLIC", "Searchable", "private"] {
            assert!(!exits(&["-P", p]));
        }
        for w in ["1/16", "116", "1/4", "104", "4", "4x", "16", "16x", "1", "1x"] {
            assert!(!exits(&["-n", "w", "-w", w]), "{w}");
        }
    }

    #[test]
    fn width_multiplier_maps_names() {
        let mut a = CliArgs::default();
        assert_eq!(width_multiplier(&a), None);
        for (s, m) in [("1/16", 116), ("104", 104), ("4x", 4), ("16", 16), ("1", 1)] {
            a.width_multiplier_string = Some(s.into());
            assert_eq!(width_multiplier(&a), Some(m));
        }
        a.width_multiplier_string = Some("7".into());
        assert_eq!(width_multiplier(&a), None);
    }
}
