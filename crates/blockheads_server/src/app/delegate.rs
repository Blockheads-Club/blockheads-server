//! `CommandLineDelegate`: list / create / rename / delete / load of worlds, the console input handler and the
//! `GameController` delegate callbacks. Also the C helper `getLine`.
//!

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::{Rc, Weak};

use crate::app::game_controller::{GameController, GameControllerArgs, GameControllerDelegate};
use crate::app::run_loop::RunLoop;
use crate::app::stdin::StdinHandle;
use crate::foundation::exception::{catch_ns, invalid_argument};
use crate::foundation::gsdict::GsDict;
use crate::foundation::{cfmt, clock, md5, nsstring, paths};
use crate::nslog;
use crate::plist::helpers::{data_from_property_list, property_list_from_data};
use crate::plist::json::json_object_with_data;
use crate::plist::value::{ClassTag, Origin, PlistDict, PlistValue};
use crate::plist::xml_read::property_list_with_data;
use crate::rng::glibc_rand;
use crate::storage::lists;
use crate::storage::lmdb::{Database, DatabaseEnvironment};
use crate::world::custom_rules::{generate_custom_rule_game_options, generate_custom_rule_world_gen_options};

/// The 21 arguments of `initWithNewWorldName:...`.
#[derive(Clone, Debug, Default)]
pub struct InitParams {
    /// Already uppercased by main.
    pub new_name: Option<String>,
    pub load_name: Option<String>,
    pub world_id: Option<String>,
    pub port: Option<String>,
    pub max_players: i32,
    pub rename_from_name: Option<String>,
    /// Already uppercased by main.
    pub rename_to_name: Option<String>,
    pub delete_world_name: Option<String>,
    pub list_worlds: bool,
    pub force_delete: bool,
    pub save_delay: i32,
    /// -1, 1, 4, 16, 104 or 116.
    pub width_multiplier: i32,
    pub credit: i32,
    pub cloud_salt: Option<String>,
    pub owner_name: Option<String>,
    pub privacy: Option<String>,
    pub custom_rules_json: Option<String>,
    pub convert_to_custom_rules: bool,
    pub expert_mode: bool,
    pub no_exit: bool,
}

/// The app delegate of the CLI.
pub struct CommandLineDelegate {
    /// Owned (ObjC retains it); the controller only holds a `Weak` back reference.
    game_controller: RefCell<Option<Rc<GameController>>>,
    stdin_registered: Cell<bool>,
    me: Weak<CommandLineDelegate>,
}

/// `getLine` on an arbitrary reader: `fgets(buf, len)` semantics (at most `len - 1` bytes, stops after a newline,
/// NUL-terminated), then the rest of an overlong line is discarded.
fn get_line_from(file: &mut dyn std::io::BufRead, buf: &mut [u8]) -> i32 {
    if buf.is_empty() {
        return 1;
    }
    let max = buf.len() - 1;
    let mut n = 0usize;
    let mut got_any = false;
    while n < max {
        let (take, hit_nl) = {
            let avail = match file.fill_buf() {
                Ok(a) => a,
                Err(_) => break,
            };
            if avail.is_empty() {
                break;
            }
            let room = max - n;
            let lim = avail.len().min(room);
            match avail[..lim].iter().position(|&b| b == b'\n') {
                Some(k) => {
                    buf[n..n + k + 1].copy_from_slice(&avail[..k + 1]);
                    (k + 1, true)
                }
                None => {
                    buf[n..n + lim].copy_from_slice(&avail[..lim]);
                    (lim, false)
                }
            }
        };
        file.consume(take);
        n += take;
        got_any = true;
        if hit_nl {
            break;
        }
    }
    if !got_any {
        return 1;
    }
    buf[n] = 0;
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    if len > 0 && buf[len - 1] == b'\n' {
        buf[len - 1] = 0;
        return 0;
    }
    let mut extra = false;
    let mut byte = [0u8; 1];
    loop {
        match file.read(&mut byte) {
            Ok(1) if byte[0] != b'\n' => extra = true,
            _ => break,
        }
    }
    if extra {
        2
    } else {
        0
    }
}

/// C `getLine(prmpt, buff, sz)`: 0 ok, 1 EOF, 2 overlong. The prompt goes to stdout and is flushed.
pub fn get_line(prompt: Option<&str>, buf: &mut [u8]) -> i32 {
    use std::io::Write;
    prompt.into_iter().for_each(|p| {
        let mut out = std::io::stdout().lock();
        // The C string ends at the first NUL.
        let end = p.as_bytes().iter().position(|&b| b == 0).unwrap_or(p.len());
        let _ = out.write_all(&p.as_bytes()[..end]);
        let _ = out.flush();
    });
    get_line_from(&mut std::io::stdin().lock(), buf)
}

/// Delete confirmation: an answer that is not valid UTF-8 confirms (nil receiver compares equal), otherwise
/// `caseInsensitiveCompare:@"yes"`.
fn answer_confirms(buf: &[u8]) -> bool {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    match std::str::from_utf8(&buf[..end]) {
        Err(_) => true,
        Ok(s) => nsstring::case_insensitive_compare(s, "yes") == 0,
    }
}

/// World width in macro blocks for a `--world_width` multiplier (1, 4, 16, 104 = 1/4, 116 = 1/16; anything else is 1).
fn width_macro(multiplier: i32) -> i32 {
    match multiplier {
        4 => 2048,
        16 => 8192,
        104 => 128,
        116 => 32,
        _ => 512,
    }
}

/// An empty dictionary (all lookups give nil).
fn empty_dict() -> PlistDict {
    PlistDict { map: GsDict::new(), tag: ClassTag { origin: Origin::Built, mutable: true, alt: false } }
}

fn get<'a>(d: &'a PlistDict, k: &str) -> Option<&'a PlistValue> {
    d.map.get(&k.to_string())
}

/// `%@` rendering of a possibly-nil dictionary value.
fn text(v: Option<&PlistValue>) -> String {
    match v {
        None => "(null)".to_string(),
        Some(PlistValue::String(s)) => s.s.clone(),
        Some(o) => o.description(),
    }
}

fn opt_text(v: Option<&PlistValue>) -> Option<String> {
    v.map(|v| match v {
        PlistValue::String(s) => s.s.clone(),
        o => o.description(),
    })
}

fn appsup_path() -> String {
    paths::appsup()
}

/// World id formula: `md5hex("random%lu%d%s%.2f")` with `r = (float)rand() / 2^31`.
pub fn compute_world_id(unix_time: u64, pid: i32, name: &str, first_rand: i32) -> String {
    let r = (first_rand as f32) / 2147483648.0f32;
    md5::string_from_md5(&format!("random{}{}{}{}", unix_time, pid, name, cfmt::fmt_f(r as f64, 2)))
}

/// Write bytes with temp file + rename (`writeToFile:atomically:YES`).
fn write_atomic(path: &str, bytes: &[u8]) -> bool {
    let tmp = format!("{}.tmp{}", path, std::process::id());
    if std::fs::write(&tmp, bytes).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return false;
    }
    if std::fs::rename(&tmp, path).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return false;
    }
    true
}

impl CommandLineDelegate {
    /// `-initWithNewWorldName:...`. `None` == ObjC nil.
    pub fn init_with_new_world_name(p: InitParams) -> Option<Rc<CommandLineDelegate>> {
        let d = Rc::new_cyclic(|w| CommandLineDelegate {
            game_controller: RefCell::new(None),
            stdin_registered: Cell::new(false),
            me: w.clone(),
        });
        if d.run_init(p) {
            Some(d)
        } else {
            None
        }
    }

    /// Body of the initializer; `false` means the object would be nil.
    fn run_init(self: &Rc<Self>, p: InitParams) -> bool {
        generate_custom_rule_world_gen_options();
        generate_custom_rule_game_options();
        let mut world_width_macro = width_macro(p.width_multiplier);
        glibc_rand::srand(clock::unix_time() as u32);
        // PIXEL_SCALE = 1.0f and IS_MULTI_CORE_CPU = 1 are fixed in the rest of the port (no globals to set).
        if p.delete_world_name.is_none() {
            self.register_stdin();
        }
        let appsup = appsup_path();
        if !Path::new(&appsup).exists() {
            let _ = std::fs::create_dir_all(&appsup);
        }

        // list
        if p.list_worlds {
            let saves = self.game_saves();
            if saves.is_empty() {
                nslog!("There are currently no worlds.");
            } else {
                for d in &saves {
                    nslog!("\"{}\" stored in directory named {}", text(get(d, "worldName")), text(get(d, "saveID")));
                }
            }
            return false;
        }

        // create
        let mut load_name = p.load_name.clone();
        if let Some(n) = &p.new_name {
            nslog!("Creating new world named \"{}\"", n);
            load_name = self.create_new_world_with_name(
                n,
                world_width_macro,
                p.world_id.as_deref(),
                p.custom_rules_json.as_deref(),
                p.expert_mode,
            );
        }

        // rename-from
        if let (Some(rf), Some(rt)) = (&p.rename_from_name, &p.rename_to_name) {
            let saves = self.game_saves();
            let found = Self::find_save(&saves, rf);
            match found {
                None => {
                    nslog!("No world with the id {} was found.", rf);
                    return false;
                }
                Some(f) => {
                    let id = opt_text(get(f, "saveID")).unwrap_or_default();
                    self.update_save_game_value(PlistValue::string(rt.clone()), "worldName", &id);
                }
            }
        }

        // delete
        if let Some(del) = &p.delete_world_name {
            let saves = self.game_saves();
            if let Some(f) = Self::find_save(&saves, del) {
                let mut confirmed = p.force_delete;
                if !p.force_delete {
                    let prompt = format!(
                        "Are you sure you want to delete the world \"{}\"? (Please enter yes or no)\n>",
                        nsstring::uppercase_string(del)
                    );
                    let mut buf = [0u8; 10];
                    let rc = get_line(Some(&prompt), &mut buf);
                    if rc == 0 {
                        confirmed = answer_confirms(&buf);
                    }
                }
                if !confirmed {
                    nslog!("Exiting, leaving world in tact.");
                } else {
                    let id = text(get(f, "saveID"));
                    let dir = format!("{}/saves/{}/", appsup_path(), id);
                    if Path::new(&dir).exists() {
                        let _ = std::fs::remove_dir_all(&dir);
                    }
                    nslog!("world \"{}\" Deleted", nsstring::uppercase_string(del));
                }
                return false;
            }
            nslog!("No world with the id or name {} was found.", del);
        }

        // load
        let mut found_world_to_load = false;
        if let Some(ln) = &load_name {
            let mut save_dict = self.load_save_dict_for_world_with_id(ln);
            if save_dict.is_some() {
                let mut sd = save_dict.take().unwrap_or_else(empty_dict);
                let mut reloaded_nil = false;
                if let Some(rt) = &p.rename_to_name {
                    let wn = opt_text(get(&sd, "worldName"));
                    if nsstring::safe_case_insensitive_compare(Some(rt), wn.as_deref()) != 0 {
                        let id = opt_text(get(&sd, "saveID")).unwrap_or_default();
                        self.update_save_game_value(PlistValue::string(rt.clone()), "worldName", &id);
                        match self.load_save_dict_for_world_with_id(&id) {
                            Some(n) => sd = n,
                            None => {
                                sd = empty_dict();
                                reloaded_nil = true;
                            }
                        }
                    }
                }
                let _ = reloaded_nil;
                let mut port = opt_text(get(&sd, "hostPort"));
                if let Some(pa) = &p.port {
                    if nsstring::int_value(pa) > 0 {
                        port = Some(pa.clone());
                    }
                }
                let port = port.unwrap_or_else(|| "15151".to_string());
                let save_id = opt_text(get(&sd, "saveID"));
                world_width_macro = get(&sd, "worldWidthMacro").map_or(0, |v| v.int_value());
                let mut max_players = get(&sd, "maxPlayers").map_or(0, |v| v.int_value());
                if p.max_players > 0 {
                    max_players = p.max_players.min(32);
                }
                nslog!(
                    "Loading world named \"{}\" on port {} with id:{}",
                    text(get(&sd, "worldName")),
                    port,
                    text(get(&sd, "saveID"))
                );
                let mut convert = p.convert_to_custom_rules;
                if convert {
                    if get(&sd, "customRulesDict").is_none() {
                        nslog!("Converting world to custom rules.");
                    } else {
                        convert = false;
                    }
                }
                self.load_world_impl(
                    &sd,
                    save_id.as_deref(),
                    &port,
                    max_players,
                    p.save_delay,
                    world_width_macro,
                    p.credit,
                    p.cloud_salt.as_deref(),
                    p.owner_name.as_deref(),
                    p.privacy.as_deref(),
                    convert,
                    p.no_exit,
                );
                found_world_to_load = true;
            }
            if !found_world_to_load {
                nslog!("No world found with id \"{}\"", ln);
                return false;
            }
        }
        if !found_world_to_load {
            nslog!("Please supply a world id to load.");
        }
        true
    }

    /// Two passes (by `saveID`, then `worldName`), stored value as receiver (nil receiver matches).
    fn find_save<'a>(saves: &'a [PlistDict], name: &str) -> Option<&'a PlistDict> {
        for key in ["saveID", "worldName"] {
            for d in saves {
                let v = opt_text(get(d, key));
                if nsstring::safe_case_insensitive_compare(v.as_deref(), Some(name)) == 0 {
                    return Some(d);
                }
            }
        }
        None
    }

    /// Register the console reader (no read happens before the run loop).
    fn register_stdin(self: &Rc<Self>) {
        self.attach_console(StdinHandle::file_handle_with_standard_input());
    }

    /// Install `handle` as the console input of the run loop (stdin for the CLI, a channel for hosts).
    pub fn attach_console(self: &Rc<Self>, handle: Rc<StdinHandle>) {
        let me = self.me.clone();
        let hw = Rc::downgrade(&handle);
        handle.add_read_completion_observer(Rc::new(move |data: &[u8]| {
            if let Some(d) = me.upgrade() {
                d.command_typed(data);
            }
            if let Some(h) = hw.upgrade() {
                h.read_in_background_and_notify();
            }
        }));
        handle.read_in_background_and_notify();
        RunLoop::current().set_stdin_source(handle);
        self.stdin_registered.set(true);
    }

    /// `-done`.
    pub fn done(&self) -> bool {
        self.game_controller.borrow().is_none()
    }

    /// For hosts that run with `no_exit`: after `/stop` the world is gone, which is where the vanilla idle check
    /// ends the process; do that here (`exit(0)`, which unwinds to the host). Otherwise true.
    pub fn keep_running(&self, no_exit: bool) -> bool {
        if no_exit && self.game_controller.borrow().as_ref().is_some_and(|g| g.world().is_none()) {
            let old = self.game_controller.borrow_mut().take();
            std::mem::forget(old);
            crate::foundation::sys::exit(0);
        }
        true
    }

    /// `-commandTyped:` (the caller re-arms the read).
    pub fn command_typed(&self, data: &[u8]) {
        let s = match std::str::from_utf8(data) {
            Ok(s) => s.to_string(),
            Err(_) => return,
        };
        let len = nsstring::length(&s);
        if len <= 1 {
            return;
        }
        // Without a game controller the message goes to nil, which does nothing.
        let gc = self.game_controller.borrow().clone();
        for gc in gc {
            let starts_with_slash = nsstring::range_of_string(&s, "/").map(|r| r.0) == Some(0);
            if !starts_with_slash && !gc.is_waiting_for_multi_part_command_response() {
                let mut msg = s.clone();
                if nsstring::range_of_string(&s, "\n").map(|r| r.0) == Some(len - 1) {
                    msg = nsstring::substring_to_index(&s, len - 1);
                }
                gc.send_chat_message_send_to_clients(Some(&msg), None);
            } else {
                gc.handle_command_issue_client(&s, None);
            }
        }
    }

    /// `-gameSaves`: newest first, may raise (`nil argument for compare:`).
    pub fn game_saves(&self) -> Vec<PlistDict> {
        let mut result: Vec<PlistDict> = Vec::new();
        let saves = paths::saves_dir();
        if !Path::new(&saves).is_dir() {
            return result;
        }
        for e in std::fs::read_dir(&saves).into_iter().flatten().flatten() {
            let item = e.file_name().to_string_lossy().into_owned();
            let d = match self.load_save_dict_for_world_with_id(&item) {
                Some(d) => d,
                None => continue,
            };
            let date = match get(&d, "saveDate") {
                Some(PlistValue::Date(t)) => Some(*t),
                _ => None,
            };
            let mut inserted = false;
            if let Some(dt) = date {
                for i in 0..result.len() {
                    let other = match get(&result[i], "saveDate") {
                        Some(PlistValue::Date(t)) => *t,
                        _ => invalid_argument("nil argument for compare:"),
                    };
                    if dt > other {
                        result.insert(i, d.clone());
                        inserted = true;
                        break;
                    }
                }
            }
            if !inserted {
                result.push(d);
            }
        }
        result
    }

    /// `-loadSaveDictForWorldWithID:`.
    pub fn load_save_dict_for_world_with_id(&self, id: &str) -> Option<PlistDict> {
        let full = paths::save_dir(id);
        match std::fs::metadata(&full) {
            Ok(m) if m.is_dir() => {}
            _ => return None,
        }
        let env = DatabaseEnvironment::new(&format!("{}/world_db/", full), 4, 1024);
        let mut dict: Option<PlistValue> = env
            .as_ref()
            .and_then(|e| Database::new(e, "main"))
            .and_then(|db| db.data_for_key(Some("worldv2")))
            .and_then(|d| property_list_from_data(&d));
        if dict.is_none() {
            let p = paths::append_path_component(&full, "worldv2");
            if Path::new(&p).exists() {
                let bytes = std::fs::read(&p).unwrap_or_default();
                dict = property_list_with_data(&bytes, 0).value;
            }
            if dict.is_none() {
                let wp = paths::append_path_component(&full, "world");
                if let Some(v) = dictionary_with_contents_of_file(&wp) {
                    dict = Some(PlistValue::Dict(v));
                }
            }
        }
        match dict {
            Some(PlistValue::Dict(d)) => {
                if get(&d, "saveVersion").map_or(0, |v| v.int_value()) >= 400
                    && !get(&d, "remoteGame").is_some_and(|v| v.bool_value())
                {
                    return Some(d);
                }
                None
            }
            _ => None,
        }
    }

    /// `-updateSaveGameValue:forKey:save:`.
    pub fn update_save_game_value(&self, value: PlistValue, key: &str, save_id: &str) {
        let full = paths::save_dir(save_id);
        let env = DatabaseEnvironment::new(&format!("{}/world_db/", full), 4, 1024);
        let db: Option<Database> = env.as_ref().and_then(|e| Database::new(e, "main"));
        let mut save_dict: Option<PlistValue> =
            db.as_ref().and_then(|db| db.data_for_key(Some("worldv2"))).and_then(|d| property_list_from_data(&d));
        if save_dict.is_none() {
            let f = paths::append_path_component(&full, "worldv2");
            save_dict = std::fs::read(&f).ok().and_then(|bytes| property_list_from_data(&bytes));
        }
        // A missing record and a non-dictionary one are reported alike (a non-dictionary plist would make
        // `initWithDictionary:` raise in the original; not observed).
        let Some(PlistValue::Dict(d)) = save_dict else { return nslog!("Unable to load world data to update") };
        let mut m = PlistDict {
            map: GsDict::copy_from(&d.map),
            tag: ClassTag { origin: Origin::Built, mutable: true, alt: false },
        };
        m.map.insert(key.to_string(), value);
        let blob = data_from_property_list(&PlistValue::Dict(m));
        for db in &db {
            db.set_data(blob.as_deref(), Some("worldv2"));
        }
    }

    /// `-createNewWorldWithName:...`.
    pub fn create_new_world_with_name(
        &self,
        name: &str,
        world_width_macro: i32,
        world_id: Option<&str>,
        custom_rules_json: Option<&str>,
        expert_mode: bool,
    ) -> Option<String> {
        let world_id = match world_id {
            Some(w) => w.to_string(),
            None => {
                let t = clock::unix_time() as u64;
                let pid = std::process::id() as i32;
                compute_world_id(t, pid, name, glibc_rand::rand())
            }
        };
        let mut dict =
            PlistDict { map: GsDict::new(), tag: ClassTag { origin: Origin::Built, mutable: true, alt: false } };
        dict.map.insert("worldName".to_string(), PlistValue::string(name));
        dict.map.insert("saveID".to_string(), PlistValue::string(world_id.clone()));
        dict.map.insert("saveVersion".to_string(), PlistValue::int(1100));
        dict.map.insert("saveDate".to_string(), PlistValue::date(clock::reference_time()));
        dict.map.insert("creationDate".to_string(), PlistValue::date(clock::reference_time()));
        dict.map.insert("expertMode".to_string(), PlistValue::boolean(expert_mode));
        dict.map.insert("worldWidthMacro".to_string(), PlistValue::int(world_width_macro as i64));
        dict.map.insert("migrationComplete_1.7".to_string(), PlistValue::boolean(true));
        if let Some(j) = custom_rules_json {
            let r = json_object_with_data(j.as_bytes());
            match r.value {
                None => nslog!(
                    "Failed to load json data to dict data - {}",
                    r.error_description.as_deref().unwrap_or("(null)")
                ),
                Some(v) => {
                    nslog!("custom rules world");
                    dict.map.insert("customRulesDict".to_string(), v);
                }
            }
        }
        let appsup = appsup_path();
        let dir = format!("{}/saves/{}", appsup, world_id);
        let path = format!("{}/worldv2", dir);
        let _ = std::fs::create_dir_all(&dir);
        let xml = data_from_property_list(&PlistValue::Dict(dict)).unwrap_or_default();
        if !write_atomic(&path, &xml) {
            nslog!("Failed to create file at path:{}", path);
            return None;
        }
        lists::create_new_world_lists(Path::new(&appsup), &world_id);
        Some(world_id)
    }

    /// Implementation with a possibly-nil save id (empty dict after a failed reload).
    #[allow(clippy::too_many_arguments)]
    fn load_world_impl(
        &self,
        sd: &PlistDict,
        save_id: Option<&str>,
        port: &str,
        max_players: i32,
        save_delay: i32,
        world_width_macro: i32,
        credit: i32,
        cloud_salt: Option<&str>,
        owner_name: Option<&str>,
        privacy: Option<&str>,
        convert_to_custom_rules: bool,
        no_exit: bool,
    ) {
        let world_width_macro = if world_width_macro == 0 { 512 } else { world_width_macro };
        let privacy = privacy.map(|s| s.to_string()).or_else(|| opt_text(get(sd, "privacy")));
        let delegate: Weak<dyn GameControllerDelegate> = self.me.clone();
        let gc = GameController::init(GameControllerArgs {
            name: opt_text(get(sd, "worldName")),
            save_id: save_id.map(|s| s.to_string()),
            app_delegate: delegate,
            port: Some(port.to_string()),
            max_players,
            save_delay,
            world_width_macro,
            credit,
            cloud_salt: cloud_salt.map(|s| s.to_string()),
            owner_name: owner_name.map(|s| s.to_string()),
            privacy,
            convert_to_custom_rules,
            expert_mode: get(sd, "expertMode").is_some_and(|v| v.bool_value()),
            no_exit,
        });
        *self.game_controller.borrow_mut() = gc;
        lists::ensure_admin_mod_files(Path::new(&appsup_path()), save_id.unwrap_or("(null)"));
    }
}

impl GameControllerDelegate for CommandLineDelegate {
    /// `removeGameController:`: drop the reference and `exit(0)` (no output, no cleanup).
    fn remove_game_controller(&self, _gc: &Rc<GameController>) {
        let old = self.game_controller.borrow_mut().take();
        // The original only autoreleases: nothing is torn down before the process exits.
        std::mem::forget(old);
        crate::foundation::sys::exit(0);
    }

    /// Empty.
    fn load_complete_for_game_controller(&self, _gc: &Rc<GameController>) {}

    /// Empty.
    fn players_changed(&self) {}

    /// Empty.
    fn clear_chat(&self) {}

    /// Echoes `"<alias>: <message>"` through NSLog.
    fn chat_message_recieved_from_game_controller_message_dict(&self, _gc: &Rc<GameController>, d: &PlistDict) {
        nslog!("{}: {}", text(get(d, "alias")), text(get(d, "message")));
    }
}

/// A world as stored on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedWorld {
    pub id: String,
    pub name: String,
}

fn detached_delegate() -> Rc<CommandLineDelegate> {
    Rc::new_cyclic(|w| CommandLineDelegate {
        game_controller: RefCell::new(None),
        stdin_registered: Cell::new(false),
        me: w.clone(),
    })
}

/// Worlds in the data directory, newest first (host API; same data as `--list`).
pub fn list_saved_worlds() -> Vec<SavedWorld> {
    let saves = catch_ns(|| detached_delegate().game_saves()).unwrap_or_default();
    saves.iter().map(|d| SavedWorld { id: text(get(d, "saveID")), name: text(get(d, "worldName")) }).collect()
}

/// Create a world on disk without starting it (host API; same as the creation step of `--new`).
/// `width_multiplier` is one of 1, 4, 16, 104 (1/4 size) or 116 (1/16 size); anything else means 1.
pub fn create_saved_world(name: &str, width_multiplier: i32, expert_mode: bool) -> Option<SavedWorld> {
    let macro_width = width_macro(width_multiplier);
    glibc_rand::srand(clock::unix_time() as u32);
    let appsup = appsup_path();
    let _ = std::fs::create_dir_all(&appsup);
    let upper = nsstring::uppercase_string(name);
    let id = catch_ns(|| detached_delegate().create_new_world_with_name(&upper, macro_width, None, None, expert_mode))
        .ok()
        .flatten()?;
    Some(SavedWorld { id, name: upper })
}

/// Delete a world by id or name (host API; same as `--delete --force`). False when no such world exists.
pub fn delete_saved_world(id_or_name: &str) -> bool {
    let d = detached_delegate();
    let saves = catch_ns(|| d.game_saves()).unwrap_or_default();
    match CommandLineDelegate::find_save(&saves, id_or_name) {
        Some(f) => {
            let id = text(get(f, "saveID"));
            let dir = format!("{}/saves/{}/", appsup_path(), id);
            if Path::new(&dir).exists() {
                return std::fs::remove_dir_all(&dir).is_ok();
            }
            true
        }
        None => false,
    }
}

/// `+[NSDictionary dictionaryWithContentsOfFile:]`: nil silently when unreadable; binary plists, garbage and
/// non-dictionary roots give nil plus one NSLog warning (A4).
pub fn dictionary_with_contents_of_file(path: &str) -> Option<PlistDict> {
    let bytes = std::fs::read(path).ok()?;
    let parsed = if bytes.starts_with(b"bplist") {
        None
    } else {
        catch_ns(|| property_list_with_data(&bytes, 0).value).ok().flatten()
    };
    match parsed {
        Some(PlistValue::Dict(d)) => Some(d),
        _ => {
            nslog!(
                "File NSDictionary.m: 635. In -[NSDictionary initWithContentsOfFile:] Contents of file '{}' does not contain a dictionary",
                path
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dict_with_contents_of_file_cases() {
        let d = std::env::temp_dir().join(format!("bh_dwcof_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("world");
        let fp = f.to_str().unwrap();
        assert!(dictionary_with_contents_of_file(fp).is_none()); // missing: silent
        std::fs::write(&f, b"{saveVersion=1100; worldName=OPENSTEP;}").unwrap();
        assert!(dictionary_with_contents_of_file(fp).is_some());
        std::fs::write(&f, b"bplist00garbage").unwrap();
        assert!(dictionary_with_contents_of_file(fp).is_none());
        std::fs::write(&f, b"").unwrap();
        assert!(dictionary_with_contents_of_file(fp).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    fn run_get_line(input: &[u8]) -> (i32, Vec<u8>) {
        let mut buf = [0u8; 10];
        let rc = get_line_from(&mut std::io::Cursor::new(input.to_vec()), &mut buf);
        let n = buf.iter().position(|&b| b == 0).unwrap_or(10);
        (rc, buf[..n].to_vec())
    }

    /// `getLine` table.
    #[test]
    fn get_line_table() {
        assert_eq!(run_get_line(b"yes\n"), (0, b"yes".to_vec()));
        assert_eq!(run_get_line(b"yes"), (0, b"yes".to_vec()));
        assert_eq!(run_get_line(b"yesyesyes\n"), (0, b"yesyesyes".to_vec()));
        assert_eq!(run_get_line(b"yesyesyesy\n").0, 2);
        assert_eq!(run_get_line(b"yesyesyesyesyes\n").0, 2);
        assert_eq!(run_get_line(b"").0, 1);
    }

    /// A reader whose every read fails.
    struct Failing;
    impl std::io::Read for Failing {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("read failed"))
        }
    }

    #[test]
    fn value_rendering_and_width_table() {
        assert_eq!(text(Some(&PlistValue::int(5))), "5");
        assert_eq!(opt_text(Some(&PlistValue::int(5))).as_deref(), Some("5"));
        assert_eq!(opt_text(None), None);
        assert_eq!(text(None), "(null)");
        let widths: Vec<i32> = [4, 16, 104, 116, 1, -1, 0].map(width_macro).to_vec();
        assert_eq!(widths, [2048, 8192, 128, 32, 512, 512, 512]);
    }

    #[test]
    fn get_line_edge_cases() {
        assert_eq!(get_line_from(&mut std::io::Cursor::new(b"abc\n".to_vec()), &mut []), 1); // no room at all
        let mut buf = [0u8; 4];
        assert_eq!(get_line_from(&mut std::io::BufReader::new(Failing), &mut buf), 1);
        // a failing read is end of input
    }

    /// Answer rules of the delete prompt.
    #[test]
    fn confirm_rules() {
        assert!(answer_confirms(b"yes\0"));
        assert!(answer_confirms(b"YES\0"));
        assert!(answer_confirms(b"Yes"));
        assert!(!answer_confirms(b"y\0"));
        assert!(!answer_confirms(b"no\0"));
        assert!(!answer_confirms(b"\0"));
        assert!(answer_confirms(b"\xff\0"));
        assert!(answer_confirms(b"y\xffs\0"));
        assert!(answer_confirms(b"\xe2\x82\0"));
    }

    /// World id formula.
    #[test]
    fn world_id_formula() {
        glibc_rand::srand(1792193084);
        let r = glibc_rand::rand();
        assert_eq!(compute_world_id(1792193084, 7, "IDTEST", r), "ab3921b867257e3dede515042f081520");
    }

    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    /// A fresh data directory for this thread (`paths::appsup()` points at it until the thread ends).
    fn data_dir(tag: &str) -> std::path::PathBuf {
        let base = std::env::var_os("TMPDIR").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = base.join(format!("bh_delegate_{tag}_{}_{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        paths::set_data_dir_override(Some(d.to_str().unwrap().to_string()));
        d
    }

    /// Write a save the way an old version stored it: a plain `worldv2` property list in `saves/<dir>/`.
    fn plain_save(root: &Path, dir: &str, build: impl FnOnce(&mut PlistDict)) {
        let mut d =
            PlistDict { map: GsDict::new(), tag: ClassTag { origin: Origin::Built, mutable: true, alt: false } };
        d.map.insert("saveID".to_string(), PlistValue::string(dir));
        d.map.insert("worldName".to_string(), PlistValue::string(dir.to_uppercase()));
        d.map.insert("saveVersion".to_string(), PlistValue::int(1100));
        build(&mut d);
        let full = root.join("saves").join(dir);
        std::fs::create_dir_all(&full).unwrap();
        std::fs::write(full.join("worldv2"), data_from_property_list(&PlistValue::Dict(d)).unwrap()).unwrap();
    }

    fn ids(saves: &[PlistDict]) -> Vec<String> {
        saves.iter().map(|d| text(get(d, "saveID"))).collect()
    }

    #[test]
    fn create_world_variants_and_failures() {
        let root = data_dir("create");
        let d = detached_delegate();
        assert_eq!(d.create_new_world_with_name("ONE", 32, Some("id1"), None, false).as_deref(), Some("id1"));
        let generated = d.create_new_world_with_name("TWO", 512, None, Some("{\"a\":1}"), true).unwrap();
        assert_eq!(generated.len(), 32);
        let broken = d.create_new_world_with_name("BAD", 512, Some("id3"), Some("{nope"), false).unwrap();
        let saved = d.load_save_dict_for_world_with_id(&broken).unwrap();
        assert!(get(&saved, "customRulesDict").is_none());
        let custom = d.load_save_dict_for_world_with_id(&generated).unwrap();
        assert!(get(&custom, "customRulesDict").is_some());
        assert!(get(&custom, "expertMode").unwrap().bool_value());
        // A directory where the world file belongs makes the final rename fail; a file where `saves` belongs the write.
        std::fs::create_dir_all(root.join("saves/id4/worldv2")).unwrap();
        assert_eq!(d.create_new_world_with_name("FOUR", 512, Some("id4"), None, false), None);
        std::fs::remove_dir_all(root.join("saves")).unwrap();
        std::fs::write(root.join("saves"), b"x").unwrap();
        assert_eq!(d.create_new_world_with_name("FIVE", 512, Some("id5"), None, false), None);
        assert!(!write_atomic(root.join("missing/dir/file").to_str().unwrap(), b"x"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn listing_orders_newest_first_and_raises_on_a_missing_date() {
        let root = data_dir("order");
        let d = detached_delegate();
        assert!(d.game_saves().is_empty()); // no saves directory yet
        std::fs::create_dir_all(root.join("saves")).unwrap();
        std::fs::write(root.join("saves/stray"), b"not a save").unwrap(); // a file is not a save
        std::fs::create_dir_all(root.join("saves/empty")).unwrap(); // nor is an empty directory
        for (id, t) in [("a", 10.0), ("b", 30.0), ("c", 20.0)] {
            plain_save(&root, id, |m| {
                m.map.insert("saveDate".to_string(), PlistValue::date(t));
            });
        }
        assert_eq!(ids(&d.game_saves()), ["b", "c", "a"]);
        assert_eq!(list_saved_worlds().iter().map(|w| w.id.as_str()).collect::<Vec<_>>(), ["b", "c", "a"]);
        // Undated saves are appended; a dated save that is not newer than everything before it reaches one and raises,
        // which the host listing absorbs. The directory order decides when that happens, so add pairs until it has.
        let raised = (0..40).any(|round| {
            plain_save(&root, &format!("u{round}"), |_| ());
            plain_save(&root, &format!("z{round}"), |m| {
                m.map.insert("saveDate".to_string(), PlistValue::date(1.0));
            });
            catch_ns(|| d.game_saves()).is_err()
        });
        assert!(raised);
        assert!(list_saved_worlds().is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn save_dictionary_sources_and_filters() {
        let root = data_dir("load");
        let d = detached_delegate();
        assert!(d.load_save_dict_for_world_with_id("nothing").is_none());
        plain_save(&root, "ok", |_| ());
        plain_save(&root, "old", |m| {
            m.map.insert("saveVersion".to_string(), PlistValue::int(399));
        });
        plain_save(&root, "remote", |m| {
            m.map.insert("remoteGame".to_string(), PlistValue::boolean(true));
        });
        assert!(d.load_save_dict_for_world_with_id("ok").is_some());
        assert!(d.load_save_dict_for_world_with_id("old").is_none());
        assert!(d.load_save_dict_for_world_with_id("remote").is_none());
        // The oldest layout: an OpenStep property list called `world`.
        let legacy = root.join("saves/legacy");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("world"), b"{saveVersion=1100; saveID=legacy; worldName=LEGACY;}").unwrap();
        assert!(d.load_save_dict_for_world_with_id("legacy").is_some());
        // A property list that is not a dictionary.
        let array = root.join("saves/array");
        std::fs::create_dir_all(&array).unwrap();
        std::fs::write(array.join("worldv2"), b"(1, 2)").unwrap();
        assert!(d.load_save_dict_for_world_with_id("array").is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn update_save_value_writes_to_the_database() {
        let root = data_dir("update");
        let d = detached_delegate();
        d.update_save_game_value(PlistValue::string("x"), "worldName", "missing"); // logs, creates nothing useful
        plain_save(&root, "w1", |_| ());
        d.update_save_game_value(PlistValue::string("RENAMED"), "worldName", "w1");
        assert_eq!(text(get(&d.load_save_dict_for_world_with_id("w1").unwrap(), "worldName")), "RENAMED");
        // A non-dictionary root cannot be updated.
        let array = root.join("saves/array");
        std::fs::create_dir_all(&array).unwrap();
        std::fs::write(array.join("worldv2"), b"(1, 2)").unwrap();
        d.update_save_game_value(PlistValue::string("x"), "worldName", "array");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn host_api_create_list_delete() {
        let root = data_dir("host");
        let w = create_saved_world("my world", 116, false).unwrap();
        assert_eq!(w.name, "MY WORLD");
        for (m, name) in [(4, "A"), (16, "B"), (104, "C"), (1, "D")] {
            assert!(create_saved_world(name, m, true).is_some());
        }
        assert_eq!(list_saved_worlds().len(), 5);
        assert!(!delete_saved_world("nope"));
        assert!(delete_saved_world("my world"));
        assert!(delete_saved_world(&list_saved_worlds()[0].id.clone()));
        assert_eq!(list_saved_worlds().len(), 3);
        // A save whose stored id differs from its directory has nothing to remove.
        plain_save(&root, "dir", |m| {
            m.map.insert("saveID".to_string(), PlistValue::string("other"));
            // Dated, so the listing never compares it with a missing date whatever order the directory is read in.
            m.map.insert("saveDate".to_string(), PlistValue::Date(1.0));
        });
        assert!(delete_saved_world("other"));
        // Creating needs a writable saves directory.
        std::fs::remove_dir_all(root.join("saves")).unwrap();
        std::fs::write(root.join("saves"), b"x").unwrap();
        assert!(create_saved_world("blocked", 1, false).is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn typed_input_without_a_world_is_ignored() {
        let _root = data_dir("typed");
        let d = detached_delegate();
        d.command_typed(b"\xff\xfe");
        d.command_typed(b"\n");
        d.command_typed(b"hello\n"); // no game controller: the nil receiver swallows it
        assert!(d.done());
    }

    #[test]
    fn typed_lines_reach_the_hosted_world() {
        let _root = data_dir("typed2");
        let w = create_saved_world("typed", 116, false).unwrap();
        let d = detached_delegate();
        let free = std::net::UdpSocket::bind("0.0.0.0:0").unwrap().local_addr().unwrap().port().to_string();
        let me: Weak<dyn GameControllerDelegate> = Rc::downgrade(&d) as Weak<CommandLineDelegate>;
        let gc = GameController::init(GameControllerArgs {
            name: Some(w.name),
            save_id: Some(w.id),
            app_delegate: me,
            port: Some(free),
            max_players: 0,
            save_delay: 10,
            world_width_macro: 32,
            credit: 0,
            cloud_salt: None,
            owner_name: None,
            privacy: None,
            convert_to_custom_rules: false,
            expert_mode: false,
            no_exit: true,
        });
        *d.game_controller.borrow_mut() = gc;
        d.command_typed(b"hello\n"); // chat with the newline stripped
        d.command_typed(b"no newline"); // chat as typed
        d.command_typed(b"/pvp-off\n"); // a command
        d.command_typed(b"x"); // too short
        assert!(!d.done());
    }
}
