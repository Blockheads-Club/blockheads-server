//! `--help` text and the `printErrorForMissingArgForOption` table.
//!

/// `HELP_FMT`: the exact 1823 bytes of the NSString literal, with one `%@` for the version.
pub const HELP_FMT: &str = "\n\nUsage: blockheads_server [options]\n\nVersion:%@\n\nWhen run without options, the most recent world will be loaded.\nAvailable options:\n\t--list (-l) list all available worlds.\n\t--new WORLD_NAME (-n) create a new world named WORLD_NAME.\n\t--world_id WORLD_ID (-i) when used with --new, assigns the given id to the newly created world. Otherwise ignored.\n\t--load WORLD_ID [options] (-o) load the world with the id WORLD_ID.\n\t--rename-from WORLD_NAME_OR_ID (-r) --rename_to NEW_NAME (-t) rename the world with the id or name WORLD_NAME_OR_ID to NEW_NAME.\n\t--delete WORLD_NAME_OR_ID (-d) delete the world with the id or name WORLD_NAME.\n\t--force -f when used with --delete, doesn't prompt for confirmation.\n\t--port PORT (-p) the port to run on when specified with --new or --load. Default is 15151.\n\t--max_players MAX (-m) the maximum number of players that will be allowed to connect at once. Default is 16, max is 32.\n\t--save_delay SECONDS (-s) will only save the world to disk every n seconds (default is 1). This is useful to limit disk activity, but more progress will be lost in the event of a crash or unclean exit.\n\t--world_width WORLD_WIDTH (-w) (optional) used when creating a new world to sepecify how big the world should be. WORLD_WIDTH must be one of '1/16', '1/4', '1' (default), '4' or '16'. 4 and 16 are 4x and 16x the normal size.\n \t--custom-rules RULES (-C) (optional) used when creating a new world to sepecify that the world should be created with custom rules enabled. RULES is a json string containing world generation rules.\n \t--convert-to-custum-rules (-V) (optional) used when loading a world to sepecify that the world should be converted from a vanilla world to a custom rules world.\n \t--expert-mode (-e) (optional) used when creating a world to select expert mode.\n\t--help (-h) display this help text\n\n";

/// `[NSString stringWithFormat:HELP_FMT, @"1.7.1"]`.
pub fn help_text() -> String {
    HELP_FMT.replacen("%@", "1.7.1", 1)
}

/// `printErrorForMissingArgForOption(optopt)`: the text of the one `NSLog` for this option, `None` for
/// `d` and every option without a case.
pub fn missing_arg_text(option: i32) -> Option<&'static str> {
    if !(0..256).contains(&option) {
        return None;
    }
    match option as u8 {
        b'n' => Some("--new must be followed by the name of the world you would like to create."),
        b'o' => Some("--load must be followed by the name of the world you would like to load."),
        b'p' => Some("--port must be followed by the port you would like the server to run on. Default is 15151."),
        b'm' => Some("--max_players must be followed by the maximum number of players you wish to be able to connect at the same time. Default is 16, max is 32."),
        b'r' => Some("--rename-from must be followed by the new name for the world. It may also only be used with --rename-to."),
        b't' => Some("--rename-to must be followed by the new name for the world. It may also only be used with --rename-from."),
        b's' => Some("--save_delay must be followed by the number of seconds to delay between each world save. Default is 1."),
        b'w' => Some("--world_width must be followed by the size multiplier of the world you would like to create. Options are '1/16', 1/4', '1', '4' or '16'."),
        b'i' => Some("--world_id must be followed by the id of the world you would like to load or create."),
        b'u' => Some("--owner_name must be followed by the owner name."),
        b'k' => Some("--cloud_salt must be followed by the salt."),
        b'c' => Some("--credit must be followed by the credit the world has available at launch."),
        b'P' => Some("--privacy must be followed by public,searchable or private."),
        b'C' => Some("--custom-rules must be followed by a json string containing world generation options. Should be ommitted for vanilla rules."),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// HELP_FMT is 1823 bytes with exactly one `%@`.
    #[test]
    fn help_fmt_shape() {
        assert_eq!(HELP_FMT.len(), 1823);
        assert_eq!(HELP_FMT.matches("%@").count(), 1);
        assert!(help_text().starts_with("\n\nUsage: blockheads_server [options]\n\nVersion:1.7.1\n"));
        assert!(help_text().ends_with("\n\n"));
    }

    /// Table: `d` and unknown options print nothing.
    #[test]
    fn table() {
        assert!(missing_arg_text(b'd' as i32).is_none());
        assert!(missing_arg_text(0).is_none());
        assert!(missing_arg_text(b'n' as i32).unwrap().starts_with("--new must"));
        assert!(missing_arg_text(b'C' as i32).is_some());
    }
}
