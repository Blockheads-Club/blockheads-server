//! `DatabaseConvertor`: the one-shot, incremental migrator of pre-1.7 loose-file saves into the LMDB databases,
//! including the faithful SIGSEGV of the light-block background job.
//!
//!
//! The `World.database_convertor` ivar still has the placeholder type `state::DatabaseConvertor` (world_state R-A);
//! the real object is therefore kept in a per-thread slot reachable through [`convertor`] / [`set_convertor`], which
//! is the single place to change once the placeholder is replaced by `pub use` of this type.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::foundation::gzip::{gzip_deflate, gzip_inflate};
use crate::foundation::paths;
use crate::plist::helpers::{data_from_property_list, property_list_from_data};
use crate::plist::value::{ClassTag, Origin, PlistDict, PlistValue};
use crate::storage::lmdb::Database;
use crate::world::lifecycle::{mutable_copy_of_dict, new_mutable_array};
use crate::world::state::World;

thread_local! {
    static CONVERTOR: RefCell<Option<Rc<DatabaseConvertor>>> = const { RefCell::new(None) };
}

/// The live convertor (`World.databaseConvertor`), `None` = nil.
pub fn convertor() -> Option<Rc<DatabaseConvertor>> {
    CONVERTOR.with(|c| c.borrow().clone())
}

/// Install / clear `World.databaseConvertor`.
pub fn set_convertor(c: Option<Rc<DatabaseConvertor>>) {
    CONVERTOR.with(|s| *s.borrow_mut() = c);
}

impl World {
    /// `World.databaseConvertor` (nil until stage 14 creates it; the real object lives in the side slot, see module
    /// docs). The one place to change when the field gets the real type.
    pub(crate) fn core_convertor(&self) -> Option<Rc<DatabaseConvertor>> {
        if self.database_convertor.borrow().is_some() {
            convertor()
        } else {
            None
        }
    }
}

/// One queued old light-block directory: `{player, path}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LightBlockJob {
    /// `dict[@"player"]`: the three directory levels concatenated.
    pub player: String,
    /// `dict[@"path"]`: the directory holding the `*_lightBlock` files.
    pub path: String,
}

/// One queued old player file: `{key, path}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerFileJob {
    pub key: String,
    pub path: String,
}

/// The result dictionary delivered from the background job (`{playerID, remove[], archive?, index?}`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LightBlockResult {
    pub player_id: String,
    pub remove: Vec<String>,
    pub archive: Option<Vec<u8>>,
    pub index: Option<Vec<u8>>,
}

/// `@interface DatabaseConvertor: NSObject` (instance size 0x88).
pub struct DatabaseConvertor {
    /// 0x08 world (assigned, not retained)
    #[allow(dead_code)]
    world: Weak<World>,
    /// 0x10 main DB
    world_database: Rc<Database>,
    /// 0x18
    dynamic_object_database: Rc<Database>,
    /// 0x20
    block_database: Rc<Database>,
    /// 0x28 (`None` disables light-block migration)
    light_block_database: Option<Rc<Database>>,
    /// 0x30 (`None` disables server (player) migration)
    server_database: Option<Rc<Database>>,
    /// 0x38 `"<appsup>/saves/<id>/"`
    world_dir: String,
    /// 0x40 worldDir + "blocks"
    blocks_dir: String,
    /// 0x48 names of the entries of worldDir, worked from the END
    main_dir_contents_to_check: RefCell<Vec<String>>,
    /// 0x50 names of the entries of blocksDir (`None` when the blocks dir was empty at init)
    world_block_dirs_to_check: RefCell<Option<Vec<String>>>,
    /// 0x58
    files_to_remove: RefCell<Vec<String>>,
    /// 0x60
    light_block_files_to_remove: RefCell<Vec<String>>,
    /// 0x68 (only used if `light_block_database` is `Some`)
    light_block_files: RefCell<Vec<LightBlockJob>>,
    /// 0x70 (only used if `server_database` is `Some`)
    server_files: RefCell<Vec<ServerFileJob>>,
    /// 0x78, set only in `new`
    world_migration_complete: Cell<bool>,
    /// 0x79, set by `get_new_light_block_files`
    light_block_migration_complete: Cell<bool>,
    /// 0x7a, set by `get_new_server_files`
    server_migration_complete: Cell<bool>,
    /// 0x7b
    background_light_block_migration_in_progress: Cell<bool>,
    /// 0x80
    light_block_main_thread_operation: RefCell<Option<LightBlockResult>>,
}

/// `[fm contentsOfDirectoryAtPath:p error:nil]`: names in raw `readdir` order, `None`/empty for a missing or
/// non-directory path (nil is treated as empty everywhere). Not sorted.
fn contents(path: &str) -> Vec<String> {
    match std::fs::read_dir(path) {
        Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect(),
        Err(_) => Vec::new(),
    }
}

/// `stringByAppendingPathComponent:`
fn join(base: &str, c: &str) -> String {
    paths::append_path_component(base, c)
}

/// `[fm removeItemAtPath:p error:nil]` (files and directories, errors ignored).
fn remove_item(p: &str) {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => {
            let _ = std::fs::remove_dir_all(p);
        }
        Ok(_) => {
            let _ = std::fs::remove_file(p);
        }
        Err(_) => {}
    }
}

/// `-[DatabaseConvertor safeDataWithContentsOfFile:]`: `nil` for a missing path or a directory.
pub fn safe_data_with_contents_of_file(file_name: &str) -> Option<Vec<u8>> {
    let md = std::fs::metadata(file_name).ok()?;
    if md.is_dir() {
        return None;
    }
    std::fs::read(file_name).ok()
}

/// `[NSScanner scanInt:]` followed by `isAtEnd` on the whole token: optional leading whitespace, optional sign,
/// at least one digit, then only (skippable) whitespace. INFERRED for the whitespace rules (: "accepts leading
/// whitespace and sign").
fn scan_int_whole(s: &str) -> bool {
    let b = s.as_bytes();
    let ws = |c: u8| matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c);
    let mut i = 0;
    while i < b.len() && ws(b[i]) {
        i += 1;
    }
    if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
        i += 1;
    }
    let start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == start {
        return false;
    }
    while i < b.len() && ws(b[i]) {
        i += 1;
    }
    i == b.len()
}

impl DatabaseConvertor {
    /// `-initWithWorld:worldDatabase:dynamicObjectDatabase:blockDatabase:lightBlockDatabase:serverDatabase:`
    /// (naming exception, 6 keywords).
    pub fn new(
        world: &Rc<World>,
        world_database: Rc<Database>,
        dynamic_object_database: Rc<Database>,
        block_database: Rc<Database>,
        light_block_database: Option<Rc<Database>>,
        server_database: Option<Rc<Database>>,
    ) -> Rc<DatabaseConvertor> {
        let id = world.save_id.borrow().clone().unwrap_or_default();
        let world_dir = format!("{}/saves/{}/", paths::appsup(), id);
        remove_item(&join(&world_dir, "blockCache"));
        remove_item(&join(&world_dir, "dynamicCache"));
        let main_dir = contents(&world_dir);
        let blocks_dir = join(&world_dir, "blocks");
        let names = contents(&blocks_dir);
        let c = DatabaseConvertor {
            world: Rc::downgrade(world),
            world_database,
            dynamic_object_database,
            block_database,
            light_block_database,
            server_database,
            world_dir,
            blocks_dir: blocks_dir.clone(),
            main_dir_contents_to_check: RefCell::new(main_dir),
            world_block_dirs_to_check: RefCell::new(None),
            files_to_remove: RefCell::new(Vec::new()),
            light_block_files_to_remove: RefCell::new(Vec::new()),
            light_block_files: RefCell::new(Vec::new()),
            server_files: RefCell::new(Vec::new()),
            world_migration_complete: Cell::new(false),
            light_block_migration_complete: Cell::new(false),
            server_migration_complete: Cell::new(false),
            background_light_block_migration_in_progress: Cell::new(false),
            light_block_main_thread_operation: RefCell::new(None),
        };
        if names.is_empty() {
            if std::path::Path::new(&blocks_dir).exists() {
                remove_item(&blocks_dir);
            }
            c.world_migration_complete.set(true);
        } else {
            *c.world_block_dirs_to_check.borrow_mut() = Some(names);
        }
        if c.light_block_database.is_some() {
            c.get_new_light_block_files();
        }
        if c.server_database.is_some() {
            c.get_new_server_files();
        }
        Rc::new(c)
    }

    /// `-getNewLightBlockFiles`: refills `light_block_files` with up to 40 entries.
    pub fn get_new_light_block_files(&self) {
        if self.light_block_database.is_none() || self.light_block_migration_complete.get() {
            return;
        }
        self.light_block_files.borrow_mut().clear();
        let old_dir = join(&self.blocks_dir, "playerLightBlocks");
        let top = contents(&old_dir);
        if top.is_empty() {
            self.light_block_files_to_remove.borrow_mut().push(old_dir);
            self.light_block_migration_complete.set(true);
            return;
        }
        let max = 40;
        let mut i = 0;
        for first_char in &top {
            let first_dir = join(&old_dir, first_char);
            let first = contents(&first_dir);
            if first.is_empty() {
                self.light_block_files_to_remove.borrow_mut().push(first_dir.clone());
            }
            for second_char in &first {
                let second_dir = join(&first_dir, second_char);
                let second = contents(&second_dir);
                if second.is_empty() {
                    self.light_block_files_to_remove.borrow_mut().push(second_dir.clone());
                }
                for rest in &second {
                    let path = join(&second_dir, rest);
                    self.light_block_files
                        .borrow_mut()
                        .push(LightBlockJob { player: format!("{}{}{}", first_char, second_char, rest), path });
                    i += 1;
                    if i >= max {
                        return;
                    }
                }
            }
        }
    }

    /// `-getNewServerFiles`: refills `server_files` with up to 1000 entries.
    pub fn get_new_server_files(&self) {
        if self.server_database.is_none() || self.server_migration_complete.get() {
            return;
        }
        self.server_files.borrow_mut().clear();
        let old_dir = join(&self.world_dir, "players");
        let top = contents(&old_dir);
        if top.is_empty() {
            self.files_to_remove.borrow_mut().push(old_dir);
            self.server_migration_complete.set(true);
            return;
        }
        let max = 1000;
        let mut i = 0;
        for first_char in &top {
            let first_dir = join(&old_dir, first_char);
            let first = contents(&first_dir);
            if first.is_empty() {
                self.files_to_remove.borrow_mut().push(first_dir.clone());
            }
            for second_char in &first {
                let second_dir = join(&first_dir, second_char);
                let second = contents(&second_dir);
                if second.is_empty() {
                    self.files_to_remove.borrow_mut().push(second_dir.clone());
                }
                for rest in &second {
                    let player_dir = join(&second_dir, rest);
                    let files = contents(&player_dir);
                    if files.is_empty() {
                        self.files_to_remove.borrow_mut().push(player_dir.clone());
                    }
                    for name in &files {
                        let full = join(&player_dir, name);
                        let postfix = if name == "playerInfo.plist" { "info" } else { name.as_str() };
                        let key = format!("{}{}{}_{}", first_char, second_char, rest, postfix);
                        self.server_files.borrow_mut().push(ServerFileJob { key, path: full });
                        i += 1;
                        if i >= max {
                            return;
                        }
                    }
                }
            }
        }
    }

    /// `-convertLightBlocks`: main-thread driver of the light-block packer, called by every `pre_update` with a server.
    /// With `faithful-crashes` the first dispatched job kills the process with SIGSEGV exactly like the original
    /// (section 6.2); without it the job runs synchronously and its result is consumed by the NEXT call.
    pub fn convert_light_blocks(&self) {
        if self.light_block_database.is_none() || self.light_block_migration_complete.get() {
            return;
        }
        let op = self.light_block_main_thread_operation.borrow_mut().take();
        if let Some(op) = op {
            if let (Some(archive), Some(index), Some(db)) = (&op.archive, &op.index, &self.light_block_database) {
                db.set_data(Some(archive), Some(&format!("{}_archiveData", op.player_id)));
                db.set_data(Some(index), Some(&format!("{}_archiveKeys", op.player_id)));
            }
            self.light_block_files_to_remove.borrow_mut().extend(op.remove);
            self.background_light_block_migration_in_progress.set(false);
        }
        if !self.background_light_block_migration_in_progress.get()
            && self.light_block_database.is_some()
            && !self.light_block_files.borrow().is_empty()
        {
            let dict = self.light_block_files.borrow_mut().pop().expect("non-empty");
            self.background_light_block_migration_in_progress.set(true);
            #[cfg(feature = "faithful-crashes")]
            {
                let _ = dict;
                light_block_job_sigsegv();
            }
            #[cfg(not(feature = "faithful-crashes"))]
            {
                let result = self.light_block_job(&dict);
                *self.light_block_main_thread_operation.borrow_mut() = Some(result);
            }
        }
    }

    /// `__39-[DatabaseConvertor convertLightBlocks]_block_invoke`: pack one player's `*_lightBlock` files.
    pub fn light_block_job(&self, dict: &LightBlockJob) -> LightBlockResult {
        let mut result =
            LightBlockResult { player_id: dict.player.clone(), remove: Vec::new(), archive: None, index: None };
        let player_dir = &dict.path;
        let files = contents(player_dir);
        if !files.is_empty() {
            let mut index_array: Vec<PlistValue> = Vec::new();
            let mut block_data: Vec<u8> = Vec::new();
            for name in &files {
                let full = join(player_dir, name);
                if name.contains("lightBlock") {
                    let parts: Vec<&str> = name.split('_').collect();
                    if parts.len() == 3 {
                        let raw = safe_data_with_contents_of_file(&full);
                        let data = raw.and_then(|r| gzip_inflate(&r));
                        if let Some(d) = data {
                            if d.len() == 0x400 {
                                index_array.push(PlistValue::string(format!("{}_{}", parts[0], parts[1])));
                                block_data.extend_from_slice(&d);
                            }
                        }
                    }
                }
            }
            if !block_data.is_empty() {
                result.archive = Some(gzip_deflate(&block_data));
                let plist = data_from_property_list(&new_mutable_array(index_array)).unwrap_or_default();
                result.index = Some(gzip_deflate(&plist));
            }
        }
        result.remove.push(player_dir.clone());
        result
    }

    /// `-convertWorld`: up to 10 small steps per call.
    pub fn convert_world(&self) {
        if !(self.light_block_migration_complete.get()
            && (!self.world_migration_complete.get() || !self.server_migration_complete.get()))
        {
            return;
        }
        for _ in 0..10 {
            if self.server_database.is_some() && !self.server_files.borrow().is_empty() {
                let dict = self.server_files.borrow().last().cloned().expect("non-empty");
                let db_key = &dict.key;
                let db: &Rc<Database> = if db_key.contains("_info") {
                    self.server_database.as_ref().expect("some")
                } else {
                    &self.world_database
                };
                if !db.has_key(Some(db_key)) {
                    if let Some(data) = safe_data_with_contents_of_file(&dict.path) {
                        db.set_data(Some(&data), Some(db_key));
                    }
                }
                self.files_to_remove.borrow_mut().push(dict.path.clone());
                self.server_files.borrow_mut().pop();
                continue;
            }
            let main_last = self.main_dir_contents_to_check.borrow().last().cloned();
            if let Some(file_name) = main_last {
                self.convert_main_dir_entry(&file_name);
                self.main_dir_contents_to_check.borrow_mut().pop();
                continue;
            }
            let block_last = self.world_block_dirs_to_check.borrow().as_ref().and_then(|v| v.last().cloned());
            if let Some(name) = block_last {
                self.convert_block_dir_entry(&name);
                if let Some(v) = self.world_block_dirs_to_check.borrow_mut().as_mut() {
                    v.pop();
                }
            }
        }
    }

    /// The `mainDirContentsToCheck` step of `convertWorld`.
    fn convert_main_dir_entry(&self, file_name: &str) {
        let full_path = join(&self.world_dir, file_name);
        let comps: Vec<&str> = file_name.split('_').collect();
        if comps.len() != 2 {
            return;
        }
        let mut is_dyn_dir = false;
        if scan_int_whole(comps[0]) && scan_int_whole(comps[1]) {
            is_dyn_dir = true;
            for dyn_file in contents(&full_path) {
                let db_key1 = join(file_name, &dyn_file);
                if self.dynamic_object_database.has_key(Some(&db_key1)) || dyn_file == "21" {
                    continue;
                }
                let p = join(&full_path, &dyn_file);
                let mut data = safe_data_with_contents_of_file(&p);
                if let (Some(d), true) = (&data, dyn_file == "17") {
                    if let Some(cleaned) = clean_dynamic_objects_17(d) {
                        data = Some(cleaned);
                    }
                }
                // executed even when `data` is nil
                self.dynamic_object_database.set_data(data.as_deref(), Some(&db_key1));
            }
            self.files_to_remove.borrow_mut().push(full_path.clone());
        }
        if !is_dyn_dir && comps[0] == "trainchest" {
            if let Some(data) = safe_data_with_contents_of_file(&full_path) {
                if !self.dynamic_object_database.has_key(Some(file_name)) {
                    self.dynamic_object_database.set_data(Some(&data), Some(file_name));
                }
                self.files_to_remove.borrow_mut().push(full_path);
            }
        }
    }

    /// The `worldBlockDirsToCheck` step of `convertWorld`.
    fn convert_block_dir_entry(&self, name: &str) {
        let path = join(&self.blocks_dir, name);
        let parts: Vec<&str> = name.split('_').collect();
        if parts.len() == 3 {
            let key = format!("{}_{}", parts[0], parts[1]);
            if !self.block_database.has_key(Some(&key)) {
                let compressed = parts[2] == "compressedBlock";
                if compressed || parts[2] == "physicalBlock" {
                    if let Some(blk) = safe_data_with_contents_of_file(&path) {
                        let blk = if compressed { Some(blk) } else { Some(gzip_deflate(&blk)) };
                        if let Some(b) = blk {
                            self.block_database.set_data(Some(&b), Some(&key));
                        }
                    }
                }
            }
        }
        if name != "playerLightBlocks" {
            self.files_to_remove.borrow_mut().push(path);
        }
    }

    /// `-removeWorldFiles`. Called by `pre_update` only after the bulk transaction committed.
    pub fn remove_world_files(&self) {
        let list: Vec<String> = std::mem::take(&mut *self.files_to_remove.borrow_mut());
        for p in &list {
            remove_item(p);
        }
        if self.server_files.borrow().len() < 10 {
            self.get_new_server_files();
        }
    }

    /// `-removeLightBlockFiles`.
    pub fn remove_light_block_files(&self) {
        let list: Vec<String> = std::mem::take(&mut *self.light_block_files_to_remove.borrow_mut());
        for p in &list {
            remove_item(p);
        }
        if self.light_block_files.borrow().len() < 4 {
            self.get_new_light_block_files();
        }
    }

    /// `-migrationComplete`
    pub fn migration_complete(&self) -> bool {
        self.world_migration_complete.get()
            && self.light_block_migration_complete.get()
            && self.server_migration_complete.get()
    }
    /// `-worldMigrationComplete`
    pub fn world_migration_complete(&self) -> bool {
        self.world_migration_complete.get()
    }
    /// `-lightBlockMigrationComplete`
    pub fn light_block_migration_complete(&self) -> bool {
        self.light_block_migration_complete.get()
    }
    /// `-serverMigrationComplete`
    pub fn server_migration_complete(&self) -> bool {
        self.server_migration_complete.get()
    }

    /// Test access: queued light-block jobs.
    #[cfg(test)]
    pub(crate) fn pending_light_block_jobs(&self) -> usize {
        self.light_block_files.borrow().len()
    }
}

/// The "17" dynamic-object file cleaning: remove `contributionGridData` / `addedToTiles` from every object's
/// `lightDict`; `Some(new bytes)` only if something changed.
fn clean_dynamic_objects_17(data: &[u8]) -> Option<Vec<u8>> {
    let save_data = property_list_from_data(data)?;
    let objs = match save_data.object_for_key("dynamicObjects") {
        Some(PlistValue::Array(a)) => a.items.clone(),
        _ => return None,
    };
    let mut new_objs: Vec<PlistValue> = Vec::new();
    let mut changed = false;
    for obj in objs {
        let light = obj.object_for_key("lightDict");
        let has_grid = light.and_then(|l| l.object_for_key("contributionGridData")).is_some();
        if !has_grid {
            new_objs.push(obj);
        } else {
            changed = true;
            let ld = match light {
                Some(PlistValue::Dict(d)) => d,
                _ => unreachable!(),
            };
            let mut m = mutable_copy_of_dict(ld);
            m.map.remove(&"contributionGridData".to_string());
            m.map.remove(&"addedToTiles".to_string());
            let mut o: PlistDict = match &obj {
                PlistValue::Dict(d) => mutable_copy_of_dict(d),
                _ => PlistDict {
                    map: crate::foundation::gsdict::GsDict::new(),
                    tag: ClassTag { origin: Origin::Built, mutable: true, alt: false },
                },
            };
            o.map.insert("lightDict".to_string(), PlistValue::Dict(m));
            new_objs.push(PlistValue::Dict(o));
        }
    }
    if !changed {
        return None;
    }
    let mut s: PlistDict = match &save_data {
        PlistValue::Dict(d) => mutable_copy_of_dict(d),
        _ => return None,
    };
    s.map.insert("dynamicObjects".to_string(), new_mutable_array(new_objs));
    data_from_property_list(&PlistValue::Dict(s))
}

/// The original's SIGSEGV: GNUstep prints the autorelease warning from the worker thread (no pool), then the
/// thread faults on the dead `__block` byref.
#[cfg(feature = "faithful-crashes")]
fn light_block_job_sigsegv() -> ! {
    crate::nslog!(
        "autorelease called without pool for object ({:p}) of class GSMutableDictionary in thread <NSThread: {:p}>",
        0x7f00_0000_1000usize as *const (),
        0x7f00_0000_1100usize as *const ()
    );
    // Rust installs its own SIGSEGV handler; without SIG_DFL a raised signal would return.
    crate::foundation::sys::faithful_segv()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::lmdb::DatabaseEnvironment;

    fn tmpdir(tag: &str) -> String {
        let p = format!("{}/bhwc_conv_{}_{}", std::env::temp_dir().display(), tag, std::process::id());
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// Build a world whose save dir lives under a private HOME so `appsup()` points into the sandbox.
    fn setup(
        tag: &str,
    ) -> (Rc<World>, String, Vec<Rc<Database>>, (Rc<DatabaseEnvironment>, std::sync::MutexGuard<'static, ()>)) {
        let guard = crate::world::lifecycle::testkit::HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = tmpdir(tag);
        crate::foundation::paths::set_home_for_test(&home);
        let world = World::alloc(32);
        *world.save_id.borrow_mut() = Some("sv".to_string());
        let wd = format!("{}/saves/sv", paths::appsup());
        std::fs::create_dir_all(&wd).unwrap();
        let env = DatabaseEnvironment::new(&format!("{}/dbs", home), 8, 64).unwrap();
        let dbs: Vec<Rc<Database>> =
            ["main", "blocks", "dw", "lb", "srv"].iter().map(|n| Rc::new(Database::new(&env, n).unwrap())).collect();
        (world, wd, dbs, (env, guard))
    }

    #[test]
    fn scanner_rules() {
        assert!(scan_int_whole("12"));
        assert!(scan_int_whole("-3"));
        assert!(scan_int_whole(" 7"));
        assert!(!scan_int_whole("1a"));
        assert!(!scan_int_whole(""));
        assert!(!scan_int_whole("world"));
    }

    #[test]
    fn empty_blocks_dir_completes_world_migration_and_is_removed() {
        let (world, wd, dbs, _env) = setup("empty_blocks");
        std::fs::create_dir_all(format!("{}/blocks", wd)).unwrap();
        std::fs::create_dir_all(format!("{}/blockCache", wd)).unwrap();
        let c = DatabaseConvertor::new(&world, dbs[0].clone(), dbs[2].clone(), dbs[1].clone(), None, None);
        assert!(c.world_migration_complete());
        assert!(!std::path::Path::new(&format!("{}/blocks", wd)).exists());
        assert!(!std::path::Path::new(&format!("{}/blockCache", wd)).exists());
        // light/server databases are nil: those flags never become YES, so the whole migration is not complete
        assert!(!c.migration_complete());
    }

    #[test]
    fn legacy_world_migration_end_to_end_without_light_blocks() {
        let (world, wd, dbs, _env) = setup("legacy");
        let (main, blocks, dw, lb, srv) = (&dbs[0], &dbs[1], &dbs[2], &dbs[3], &dbs[4]);
        // players/ab/cd/efgh/{playerInfo.plist,inventory}
        std::fs::create_dir_all(format!("{}/players/ab/cd/efgh", wd)).unwrap();
        std::fs::write(format!("{}/players/ab/cd/efgh/playerInfo.plist", wd), b"INFO").unwrap();
        std::fs::write(format!("{}/players/ab/cd/efgh/inventory", wd), b"INV").unwrap();
        std::fs::write(format!("{}/trainchest_9", wd), b"TC").unwrap();
        std::fs::create_dir_all(format!("{}/10_20", wd)).unwrap();
        std::fs::write(format!("{}/10_20/5", wd), b"FIVE").unwrap();
        std::fs::write(format!("{}/10_20/21", wd), b"DROP").unwrap();
        let xml17 = br#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//GNUstep//DTD plist 0.9//EN" "http://www.gnustep.org/plist-0_9.xml">
<plist version="0.9"><dict><key>dynamicObjects</key><array><dict><key>lightDict</key><dict><key>contributionGridData</key><data>AAAA</data><key>addedToTiles</key><true/><key>keep</key><string>x</string></dict></dict></array></dict></plist>"#;
        std::fs::write(format!("{}/10_20/17", wd), xml17).unwrap();
        std::fs::create_dir_all(format!("{}/blocks", wd)).unwrap();
        std::fs::write(format!("{}/blocks/3_4_compressedBlock", wd), b"CB").unwrap();
        std::fs::write(format!("{}/blocks/1_2_physicalBlock", wd), b"PB").unwrap();
        std::fs::write(format!("{}/blocks/junkfile", wd), b"J").unwrap();
        std::fs::create_dir_all(format!("{}/blocks/playerLightBlocks", wd)).unwrap();

        let c = DatabaseConvertor::new(
            &world,
            main.clone(),
            dw.clone(),
            blocks.clone(),
            Some(lb.clone()),
            Some(srv.clone()),
        );
        assert!(!c.world_migration_complete());
        // empty playerLightBlocks: light block migration completes in init
        assert!(c.light_block_migration_complete());
        assert!(!c.server_migration_complete());
        for _ in 0..10 {
            c.convert_world();
        }
        assert_eq!(srv.data_for_key(Some("abcdefgh_info")).unwrap(), b"INFO");
        assert_eq!(main.data_for_key(Some("abcdefgh_inventory")).unwrap(), b"INV");
        assert_eq!(dw.data_for_key(Some("trainchest_9")).unwrap(), b"TC");
        assert_eq!(dw.data_for_key(Some("10_20/5")).unwrap(), b"FIVE");
        assert!(!dw.has_key(Some("10_20/21")));
        let seventeen = dw.data_for_key(Some("10_20/17")).unwrap();
        let v = property_list_from_data(&seventeen).unwrap();
        let ld = match v.object_for_key("dynamicObjects") {
            Some(PlistValue::Array(a)) => a.items[0].object_for_key("lightDict").unwrap().clone(),
            _ => panic!("no array"),
        };
        assert!(ld.object_for_key("contributionGridData").is_none());
        assert!(ld.object_for_key("addedToTiles").is_none());
        assert_eq!(ld.object_for_key("keep").and_then(|k| k.as_str()), Some("x"));
        assert_eq!(blocks.data_for_key(Some("3_4")).unwrap(), b"CB");
        assert_eq!(gzip_inflate(&blocks.data_for_key(Some("1_2")).unwrap()).unwrap(), b"PB");
        assert!(!blocks.has_key(Some("junkfile")));
        // removal queue then removal
        assert!(std::path::Path::new(&format!("{}/trainchest_9", wd)).exists());
        c.remove_world_files();
        assert!(!std::path::Path::new(&format!("{}/trainchest_9", wd)).exists());
        assert!(!std::path::Path::new(&format!("{}/10_20", wd)).exists());
        assert!(!std::path::Path::new(&format!("{}/blocks/junkfile", wd)).exists());
        assert!(std::path::Path::new(&format!("{}/blocks/playerLightBlocks", wd)).exists());
        // completion is detected only at init: the flags of this instance stay as they were
        assert!(!c.world_migration_complete());
    }

    #[test]
    fn light_block_job_packs_and_skips_bad_files() {
        let (world, wd, dbs, _env) = setup("lbjob");
        let c =
            DatabaseConvertor::new(&world, dbs[0].clone(), dbs[2].clone(), dbs[1].clone(), Some(dbs[3].clone()), None);
        let pdir = format!("{}/blocks/playerLightBlocks/a/b/cdefgh", wd);
        std::fs::create_dir_all(&pdir).unwrap();
        std::fs::write(format!("{}/3_4_lightBlock", pdir), gzip_deflate(&[7u8; 1024])).unwrap();
        std::fs::write(format!("{}/5_6_lightBlock", pdir), gzip_deflate(&[1u8; 10])).unwrap(); // wrong length
        std::fs::write(format!("{}/bad_name", pdir), b"x").unwrap();
        let job = LightBlockJob { player: "abcdefgh".to_string(), path: pdir.clone() };
        let r = c.light_block_job(&job);
        assert_eq!(r.player_id, "abcdefgh");
        assert_eq!(r.remove, vec![pdir]);
        assert_eq!(gzip_inflate(&r.archive.unwrap()).unwrap(), vec![7u8; 1024]);
        let idx = property_list_from_data(&gzip_inflate(&r.index.unwrap()).unwrap()).unwrap();
        match idx {
            PlistValue::Array(a) => {
                assert_eq!(a.items.len(), 1);
                assert_eq!(a.items[0].as_str(), Some("3_4"));
            }
            _ => panic!("array expected"),
        }
    }

    #[test]
    fn light_block_queue_collects_player_dirs() {
        let (world, wd, dbs, _env) = setup("lbqueue");
        let pdir = format!("{}/blocks/playerLightBlocks/a/b/cdefgh", wd);
        std::fs::create_dir_all(&pdir).unwrap();
        std::fs::write(format!("{}/3_4_lightBlock", pdir), gzip_deflate(&[7u8; 1024])).unwrap();
        let c =
            DatabaseConvertor::new(&world, dbs[0].clone(), dbs[2].clone(), dbs[1].clone(), Some(dbs[3].clone()), None);
        assert!(!c.light_block_migration_complete());
        assert_eq!(c.pending_light_block_jobs(), 1);
        // convertWorld does nothing until the light-block migration is complete
        c.convert_world();
        assert!(!dbs[1].has_key(Some("3_4")));
    }

    #[cfg(not(feature = "faithful-crashes"))]
    #[test]
    fn light_block_job_result_is_consumed_by_the_next_call() {
        let (world, wd, dbs, _env) = setup("lbnext");
        let pdir = format!("{}/blocks/playerLightBlocks/a/b/cdefgh", wd);
        std::fs::create_dir_all(&pdir).unwrap();
        std::fs::write(format!("{}/3_4_lightBlock", pdir), gzip_deflate(&[7u8; 1024])).unwrap();
        let c =
            DatabaseConvertor::new(&world, dbs[0].clone(), dbs[2].clone(), dbs[1].clone(), Some(dbs[3].clone()), None);
        c.convert_light_blocks();
        assert!(!dbs[3].has_key(Some("abcdefgh_archiveData")));
        c.convert_light_blocks();
        assert!(dbs[3].has_key(Some("abcdefgh_archiveData")));
        assert!(dbs[3].has_key(Some("abcdefgh_archiveKeys")));
        c.remove_light_block_files();
        assert!(!std::path::Path::new(&pdir).exists());
    }

    /// Faithful SIGSEGV: the first light-block job dispatch kills the process with signal 11 after
    /// the GNUstep autorelease warning. The scenario runs in a child process (re-exec of the test binary).
    #[cfg(all(unix, feature = "faithful-crashes"))]
    #[test]
    fn faithful_sigsegv_on_first_light_block_job() {
        use std::os::unix::process::ExitStatusExt;
        if std::env::var("BH_WC_CRASH_CHILD").is_ok() {
            let (world, wd, dbs, _keep) = setup("crash_child");
            let pdir = format!("{}/blocks/playerLightBlocks/a/b/cdefgh", wd);
            std::fs::create_dir_all(&pdir).unwrap();
            std::fs::write(format!("{}/3_4_lightBlock", pdir), gzip_deflate(&[7u8; 1024])).unwrap();
            let c = DatabaseConvertor::new(
                &world,
                dbs[0].clone(),
                dbs[2].clone(),
                dbs[1].clone(),
                Some(dbs[3].clone()),
                None,
            );
            c.convert_light_blocks();
            std::process::exit(0); // not reached
        }
        let exe = std::env::current_exe().unwrap();
        let out = std::process::Command::new(exe)
            .args([
                "--exact",
                "world::db_convertor::tests::faithful_sigsegv_on_first_light_block_job",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("BH_WC_CRASH_CHILD", "1")
            .output()
            .unwrap();
        assert_eq!(out.status.signal(), Some(11), "status {:?}", out.status);
        let err = String::from_utf8_lossy(&out.stderr);
        let last = err.lines().filter(|l| !l.trim().is_empty()).last().unwrap_or("");
        assert!(
            last.contains("autorelease called without pool for object (0x")
                && last.contains("of class GSMutableDictionary in thread <NSThread: 0x"),
            "{}",
            err
        );
    }
}
