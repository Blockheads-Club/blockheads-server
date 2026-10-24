//! LMDB wrapper: `DatabaseEnvironment` and `Database`.
//! Raw FFI on purpose. Single-threaded: types are `!Send`/`!Sync`.

use crate::nslog;
use lmdb_master_sys as ffi;
use std::cell::Cell;
use std::ffi::{CStr, CString};
use std::os::raw::{c_int, c_void};
use std::ptr;
use std::rc::Rc;

/// `MDB_MAP_FULL`.
const MDB_MAP_FULL: c_int = -30792;
/// `MDB_NOTFOUND`.
const MDB_NOTFOUND: c_int = -30798;

/// The result code of one LMDB call. Under test a call can be made to fail (see `inject`), which reaches the error
/// paths that real LMDB only takes when it runs out of resources.
fn call(op: &'static str, real: &mut dyn FnMut() -> c_int) -> c_int {
    #[cfg(test)]
    if op == "txn_commit" {
        // A failed commit still ends the transaction: run it, then report the injected code.
        let rc = real();
        return inject::take(op).unwrap_or(rc);
    }
    #[cfg(test)]
    if let Some(rc) = inject::take(op) {
        return rc;
    }
    let _ = op;
    real()
}

/// `mdb_strerror(rc)` as a Rust string.
fn strerror(rc: c_int) -> String {
    // SAFETY: `mdb_strerror` returns a static string, or libc's `strerror`, for every code; never NULL.
    unsafe { CStr::from_ptr(ffi::mdb_strerror(rc)).to_string_lossy().into_owned() }
}

/// Log `"<what> failed:<rc> - <strerror>"`.
fn log_fail(what: &str, rc: c_int) {
    nslog!("{} failed:{} - {}", what, rc, strerror(rc));
}

/// `-[NSString UTF8String]` as a `CString`: the bytes up to the first NUL.
fn c_string_prefix(s: &str) -> CString {
    let b = s.as_bytes();
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    CString::new(&b[..end]).expect("no interior NUL")
}

/// Die like the original's NULL dereference: default SIGSEGV disposition (Rust's runtime installs a
/// stack-overflow handler that would swallow a raised SIGSEGV), then raise it. Exit status 139.
fn faithful_segv() -> ! {
    crate::foundation::sys::faithful_segv()
}

/// Map-full handling shared by all commit paths.
fn map_full_exit() -> ! {
    nslog!("Database map is full. Exiting, will resize on next launch.");
    crate::foundation::sys::exit(0)
}

/// `DatabaseEnvironment`.
pub struct DatabaseEnvironment {
    env: *mut ffi::MDB_env,
    bulk_txn: Cell<*mut ffi::MDB_txn>,
    #[allow(dead_code)]
    path: String,
}

impl DatabaseEnvironment {
    /// `-initWithPath:maxDatabases:maxMapSizeInMB:`. `None` == nil.
    pub fn new(path: &str, max_databases: i32, max_map_size_mb: usize) -> Option<Rc<DatabaseEnvironment>> {
        Self::new_impl(path, max_databases, max_map_size_mb, cfg!(all(feature = "faithful-crashes", not(test))))
    }

    /// Body of [`DatabaseEnvironment::new`]; `crash_on_failed_open` selects the faithful SIGSEGV on a
    /// failed `mdb_env_open`.
    fn new_impl(
        path: &str,
        max_databases: i32,
        max_map_size_mb: usize,
        crash_on_failed_open: bool,
    ) -> Option<Rc<DatabaseEnvironment>> {
        // The unit tests of a 32-bit build run many worlds in one process, and every environment maps its
        // whole size: at the 1 GiB the world asks for, a handful of them (some leaked in reference cycles)
        // use up the 4 GiB address space. Test-only; production maps what the original maps.
        #[cfg(all(test, target_pointer_width = "32"))]
        let max_map_size_mb = max_map_size_mb.min(32);
        let mut mb: usize = if max_map_size_mb < 2 { 1 } else { max_map_size_mb };
        let _ = std::fs::create_dir_all(path);
        unsafe {
            let mut env: *mut ffi::MDB_env = ptr::null_mut();
            let rc = call("env_create", &mut || ffi::mdb_env_create(&mut env));
            if rc != 0 {
                log_fail("mdb_env_create", rc);
                return None;
            }
            let rc = call("set_mapsize", &mut || ffi::mdb_env_set_mapsize(env, 0x1000));
            if rc != 0 {
                log_fail("mdb_env_set_mapsize", rc);
                ffi::mdb_env_close(env);
                return None;
            }
            let rc = call("set_maxdbs", &mut || ffi::mdb_env_set_maxdbs(env, max_databases as ffi::MDB_dbi));
            if rc != 0 {
                log_fail("mdb_env_set_maxdbs", rc);
                ffi::mdb_env_close(env);
                return None;
            }
            // `[path UTF8String]`: a C string ends at the first NUL.
            let cpath = c_string_prefix(path);
            let open_rc = call("env_open", &mut || ffi::mdb_env_open(env, cpath.as_ptr(), 0, 0o664));
            let mut info: ffi::MDB_envinfo = std::mem::zeroed();
            // : mdb_env_info runs even if the open failed. After a failed open LMDB's
            // `me_metas` are NULL/dangling, so the original dereferences them and dies with SIGSEGV
            // before any log line (observed: reference `--load` with `world_db` replaced by a regular
            // file exits 139 with empty output). Reproduced under `faithful-crashes`; otherwise the
            // info call is skipped and the spec's nil/log path is taken.
            if open_rc == 0 {
                ffi::mdb_env_info(env, &mut info);
            } else if crash_on_failed_open {
                faithful_segv();
            }
            // Wrapping unsigned grow loop.
            while mb.wrapping_sub(4).wrapping_mul(1 << 20) < info.me_mapsize as usize && mb < 0x4000 {
                mb <<= 1;
            }
            let rc2 = call("set_mapsize", &mut || ffi::mdb_env_set_mapsize(env, mb << 20));
            if rc2 != 0 {
                log_fail("mdb_env_set_mapsize", rc2);
                ffi::mdb_env_close(env);
                return None;
            }
            if open_rc != 0 {
                log_fail("mdb_env_open", open_rc);
                ffi::mdb_env_close(env);
                return None;
            }
            Some(Rc::new(DatabaseEnvironment { env, bulk_txn: Cell::new(ptr::null_mut()), path: path.to_string() }))
        }
    }

    /// `-startBulkTransaction`.
    pub fn start_bulk_transaction(&self) -> bool {
        if self.bulk_txn.get().is_null() {
            let mut t: *mut ffi::MDB_txn = ptr::null_mut();
            let rc = call("txn_begin", &mut || unsafe { ffi::mdb_txn_begin(self.env, ptr::null_mut(), 0, &mut t) });
            if rc != 0 {
                log_fail("mdb startTransaction", rc);
                return false;
            }
            self.bulk_txn.set(t);
        }
        !self.bulk_txn.get().is_null()
    }

    /// `-finishBulkTransaction`.
    pub fn finish_bulk_transaction(&self) -> bool {
        let t = self.bulk_txn.get();
        if !t.is_null() {
            let rc = call("txn_commit", &mut || unsafe { ffi::mdb_txn_commit(t) });
            self.bulk_txn.set(ptr::null_mut());
            if rc != 0 {
                log_fail("finishTransaction", rc);
                if rc == MDB_MAP_FULL {
                    map_full_exit();
                }
                return false;
            }
        }
        true
    }

    /// Raw env handle.
    pub fn env(&self) -> *mut ffi::MDB_env {
        self.env
    }

    /// Current bulk txn (null if none).
    pub fn bulk_transaction(&self) -> *mut ffi::MDB_txn {
        self.bulk_txn.get()
    }

    /// `getTransaction`: bulk txn, else a fresh RW txn.
    fn get_transaction(&self) -> *mut ffi::MDB_txn {
        let b = self.bulk_txn.get();
        if !b.is_null() {
            return b;
        }
        let mut t: *mut ffi::MDB_txn = ptr::null_mut();
        let rc = call("txn_begin", &mut || unsafe { ffi::mdb_txn_begin(self.env, ptr::null_mut(), 0, &mut t) });
        if rc != 0 {
            log_fail("mdb startTransaction", rc);
            return ptr::null_mut();
        }
        t
    }

    /// `finishTransaction:`.
    fn finish_transaction(&self, t: *mut ffi::MDB_txn) -> bool {
        if t == self.bulk_txn.get() {
            return true;
        }
        let rc = call("txn_commit", &mut || unsafe { ffi::mdb_txn_commit(t) });
        if rc == 0 {
            return true;
        }
        log_fail("finishTransaction", rc);
        if rc == MDB_MAP_FULL {
            map_full_exit();
        }
        false
    }
}

impl Drop for DatabaseEnvironment {
    fn drop(&mut self) {
        self.finish_bulk_transaction();
        unsafe { ffi::mdb_env_close(self.env) };
    }
}

/// `Database`: one named sub-database.
pub struct Database {
    env: Rc<DatabaseEnvironment>,
    dbi: ffi::MDB_dbi,
    #[allow(dead_code)]
    name: String,
}

fn val(b: &[u8]) -> ffi::MDB_val {
    ffi::MDB_val { mv_size: b.len(), mv_data: b.as_ptr() as *mut c_void }
}

impl Database {
    /// `-initWithEnvironment:name:`. `None` == nil.
    pub fn new(env: &Rc<DatabaseEnvironment>, name: &str) -> Option<Database> {
        unsafe {
            // `[n UTF8String]`: a C string ends at the first NUL (converted before the txn so no
            // early return can leak a write txn).
            let cname = c_string_prefix(name);
            let mut t: *mut ffi::MDB_txn = ptr::null_mut();
            let rc = call("txn_begin", &mut || ffi::mdb_txn_begin(env.env, ptr::null_mut(), 0, &mut t));
            if rc != 0 {
                log_fail("mdb_txn_begin", rc);
                return None;
            }
            let mut dbi: ffi::MDB_dbi = 0;
            let rc = call("dbi_open", &mut || ffi::mdb_dbi_open(t, cname.as_ptr(), ffi::MDB_CREATE, &mut dbi));
            if rc != 0 {
                log_fail("mdb_dbi_open", rc);
                ffi::mdb_txn_abort(t);
                return None;
            }
            let rc = call("txn_commit", &mut || ffi::mdb_txn_commit(t));
            if rc != 0 {
                log_fail("mdb_txn_commit", rc);
                return None;
            }
            Some(Database { env: env.clone(), dbi, name: name.to_string() })
        }
    }

    /// `-dataForKey:`.
    pub fn data_for_key(&self, key: Option<&str>) -> Option<Vec<u8>> {
        let key = match key {
            Some(k) => k,
            None => {
                nslog!("Error, nil key in dataForKey");
                return None;
            }
        };
        let txn = self.env.get_transaction();
        if txn.is_null() {
            return None;
        }
        let mut k = val(key.as_bytes());
        let mut v = ffi::MDB_val { mv_size: 0, mv_data: ptr::null_mut() };
        let rc = unsafe { ffi::mdb_get(txn, self.dbi, &mut k, &mut v) };
        let out = if rc == 0 {
            Some(if v.mv_size == 0 {
                Vec::new()
            } else {
                unsafe { std::slice::from_raw_parts(v.mv_data as *const u8, v.mv_size) }.to_vec()
            })
        } else {
            None
        };
        self.env.finish_transaction(txn);
        out
    }

    /// `-mutableDataForKey:` (same bytes).
    pub fn mutable_data_for_key(&self, key: Option<&str>) -> Option<Vec<u8>> {
        self.data_for_key(key)
    }

    /// `-setData:forKey:`; `None` data removes.
    pub fn set_data(&self, data: Option<&[u8]>, key: Option<&str>) -> bool {
        let key = match key {
            Some(k) => k,
            None => {
                nslog!("Error, nil key in setData");
                return false;
            }
        };
        let data = match data {
            Some(d) => d,
            None => return self.remove_data_for_key(Some(key)),
        };
        let txn = self.env.get_transaction();
        if txn.is_null() {
            return false;
        }
        let mut k = val(key.as_bytes());
        let mut v = val(data);
        let rc = call("put", &mut || unsafe { ffi::mdb_put(txn, self.dbi, &mut k, &mut v, 0) });
        if rc == 0 {
            return self.env.finish_transaction(txn);
        }
        log_fail("mdb setData", rc);
        self.env.finish_transaction(txn);
        if rc == MDB_MAP_FULL {
            map_full_exit();
        }
        false
    }

    /// `-removeDataForKey:`.
    pub fn remove_data_for_key(&self, key: Option<&str>) -> bool {
        let key = match key {
            Some(k) => k,
            None => {
                nslog!("Error, nil key in removeDataForKey");
                return false;
            }
        };
        let txn = self.env.get_transaction();
        if txn.is_null() {
            return false;
        }
        let mut k = val(key.as_bytes());
        let rc = call("del", &mut || unsafe { ffi::mdb_del(txn, self.dbi, &mut k, ptr::null_mut()) });
        self.env.finish_transaction(txn);
        if rc == 0 || rc == MDB_NOTFOUND {
            true
        } else {
            log_fail("mdb removeObject", rc);
            false
        }
    }

    /// `-hasKey:`.
    pub fn has_key(&self, key: Option<&str>) -> bool {
        let key = match key {
            Some(k) => k,
            None => {
                nslog!("Error, nil key in hasKey");
                return false;
            }
        };
        let txn = self.env.get_transaction();
        if txn.is_null() {
            return false;
        }
        let mut k = val(key.as_bytes());
        let mut v = ffi::MDB_val { mv_size: 0, mv_data: ptr::null_mut() };
        let rc = unsafe { ffi::mdb_get(txn, self.dbi, &mut k, &mut v) };
        self.env.finish_transaction(txn);
        rc == 0
    }

    /// `-dbi`.
    pub fn dbi(&self) -> u32 {
        self.dbi
    }

    /// Convenience for `data_for_key(Some(key))`.
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.data_for_key(Some(key))
    }
    /// Convenience for `set_data(Some(data), Some(key))`.
    pub fn put(&self, key: &str, data: &[u8]) -> bool {
        self.set_data(Some(data), Some(key))
    }
    /// Convenience for `remove_data_for_key(Some(key))`.
    pub fn remove(&self, key: &str) -> bool {
        self.remove_data_for_key(Some(key))
    }

    /// Tools/tests only: all pairs in LMDB byte order.
    #[doc(hidden)]
    pub fn entries(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut out = Vec::new();
        unsafe {
            let bulk = self.env.bulk_txn.get();
            let mut t = bulk;
            if t.is_null() {
                let rc = call("txn_begin", &mut || {
                    ffi::mdb_txn_begin(self.env.env, ptr::null_mut(), ffi::MDB_RDONLY, &mut t)
                });
                if rc != 0 {
                    return out;
                }
            }
            let mut cur: *mut ffi::MDB_cursor = ptr::null_mut();
            if call("cursor_open", &mut || ffi::mdb_cursor_open(t, self.dbi, &mut cur)) == 0 {
                let mut k = ffi::MDB_val { mv_size: 0, mv_data: ptr::null_mut() };
                let mut v = ffi::MDB_val { mv_size: 0, mv_data: ptr::null_mut() };
                let mut op = ffi::MDB_FIRST;
                while ffi::mdb_cursor_get(cur, &mut k, &mut v, op) == 0 {
                    op = ffi::MDB_NEXT;
                    let kb = std::slice::from_raw_parts(k.mv_data as *const u8, k.mv_size).to_vec();
                    let vb = if v.mv_size == 0 {
                        Vec::new()
                    } else {
                        std::slice::from_raw_parts(v.mv_data as *const u8, v.mv_size).to_vec()
                    };
                    out.push((kb, vb));
                }
                ffi::mdb_cursor_close(cur);
            }
            if bulk.is_null() {
                ffi::mdb_txn_abort(t);
            }
        }
        out
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        self.env.finish_bulk_transaction();
        unsafe { ffi::mdb_dbi_close(self.env.env, self.dbi) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> String {
        let p = std::env::temp_dir().join(format!("bh_storage_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        format!("{}/", p.display())
    }

    fn txnid(e: &DatabaseEnvironment) -> usize {
        unsafe {
            let mut i: ffi::MDB_envinfo = std::mem::zeroed();
            ffi::mdb_env_info(e.env(), &mut i);
            i.me_last_txnid as usize
        }
    }

    #[test]
    fn env_open_mapsize_and_files() {
        let p = tmp("open");
        let e = DatabaseEnvironment::new(&p, 4, 16).unwrap();
        unsafe {
            let mut i: ffi::MDB_envinfo = std::mem::zeroed();
            ffi::mdb_env_info(e.env(), &mut i);
            assert_eq!(i.me_mapsize as usize, 16 << 20);
        }
        assert!(std::path::Path::new(&format!("{p}data.mdb")).exists());
        assert!(std::path::Path::new(&format!("{p}lock.mdb")).exists());
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn a_four_megabyte_request_grows_to_eight() {
        // The grow loop keeps the map more than 4 MB above what LMDB reports right after opening.
        let p = tmp("grow");
        let e = DatabaseEnvironment::new(&p, 1, 4).unwrap();
        unsafe {
            let mut i: ffi::MDB_envinfo = std::mem::zeroed();
            ffi::mdb_env_info(e.env(), &mut i);
            assert_eq!(i.me_mapsize as usize, 8 << 20);
        }
        drop(e);
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn mb_small_is_one() {
        for m in [0usize, 1] {
            let p = tmp(&format!("mb{m}"));
            let e = DatabaseEnvironment::new(&p, 1, m).unwrap();
            unsafe {
                let mut i: ffi::MDB_envinfo = std::mem::zeroed();
                ffi::mdb_env_info(e.env(), &mut i);
                assert_eq!(i.me_mapsize as usize, 1 << 20);
            }
            drop(e);
            let _ = std::fs::remove_dir_all(&p);
        }
    }

    #[test]
    fn open_on_file_fails() {
        let p = tmp("file");
        let f = p.trim_end_matches('/').to_string();
        std::fs::write(&f, b"x").unwrap();
        assert!(DatabaseEnvironment::new_impl(&f, 1, 4, false).is_none());
        let _ = std::fs::remove_file(&f);
    }

    ///  / T3: with `faithful-crashes` a failed open dies with SIGSEGV and no log
    /// (observed reference exit 139). Runs the crashing call in a child test process.
    #[cfg(unix)]
    #[test]
    fn open_on_file_crashes_faithfully() {
        use std::os::unix::process::ExitStatusExt;
        if let Ok(f) = std::env::var("BH_STORAGE_CRASH_PATH") {
            let _ = DatabaseEnvironment::new_impl(&f, 1, 4, true);
            std::process::exit(3);
        }
        let p = tmp("crash");
        let f = p.trim_end_matches('/').to_string();
        std::fs::write(&f, b"x").unwrap();
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "storage::lmdb::tests::open_on_file_crashes_faithfully",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("BH_STORAGE_CRASH_PATH", &f)
            .output()
            .unwrap();
        let _ = std::fs::remove_file(&f);
        assert_eq!(out.status.signal(), Some(libc::SIGSEGV));
        assert!(!String::from_utf8_lossy(&out.stderr).contains("failed:"));
    }

    #[test]
    fn failing_calls_are_logged_and_cleaned_up() {
        use inject::queue;
        let p = tmp("fail");
        // Each set-up call, failing in turn; the environment is closed and nothing is returned.
        for (op, codes) in [
            ("env_create", vec![Some(12)]),
            ("set_mapsize", vec![Some(22)]),
            ("set_maxdbs", vec![Some(22)]),
            ("set_mapsize", vec![None, Some(22)]),
            ("env_open", vec![Some(2)]),
        ] {
            queue(op, &codes);
            assert!(DatabaseEnvironment::new(&p, 4, 16).is_none(), "{op}");
        }
        let e = DatabaseEnvironment::new(&p, 4, 16).unwrap();
        // Opening a sub-database: begin, open and commit can each fail.
        for op in ["txn_begin", "dbi_open", "txn_commit"] {
            queue(op, &[Some(12)]);
            assert!(Database::new(&e, "main").is_none(), "{op}");
        }
        let d = Database::new(&e, "main").unwrap();
        assert!(d.put("a", b"1"));
        // Without a transaction every operation gives up quietly.
        queue("txn_begin", &[Some(12)]);
        assert_eq!(d.get("a"), None);
        queue("txn_begin", &[Some(12)]);
        assert!(!d.put("b", b"2"));
        queue("txn_begin", &[Some(12)]);
        assert!(!d.remove("a"));
        queue("txn_begin", &[Some(12)]);
        assert!(!d.has_key(Some("a")));
        // A commit that fails (not for lack of space) is reported to the caller.
        queue("txn_commit", &[Some(5)]);
        assert!(!d.put("c", b"3"));
        queue("del", &[Some(22)]);
        assert!(!d.remove("a"));
        // Bulk transactions: begin and commit failures.
        queue("txn_begin", &[Some(12)]);
        assert!(!e.start_bulk_transaction());
        assert!(e.start_bulk_transaction());
        assert!(e.start_bulk_transaction()); // already running
        queue("txn_commit", &[Some(5)]);
        assert!(!e.finish_bulk_transaction());
        assert!(e.finish_bulk_transaction()); // nothing running
                                              // The read-only snapshot used by `entries` can fail to start or to open a cursor.
        queue("txn_begin", &[Some(12)]);
        assert!(d.entries().is_empty());
        queue("cursor_open", &[Some(12)]);
        assert!(d.entries().is_empty());
        // Inside a bulk transaction `entries` reads through it.
        assert!(e.start_bulk_transaction());
        assert!(!d.entries().is_empty());
        assert!(e.finish_bulk_transaction());
        assert_eq!(inject::take("never queued"), None);
        assert_eq!(d.mutable_data_for_key(Some("a")), d.data_for_key(Some("a")));
        assert_eq!(d.dbi(), d.dbi());
    }

    /// The commit paths exit the process (status 0, after one log line) when the map is full; runs in a child.
    #[test]
    fn a_full_map_ends_the_process() {
        use inject::queue;
        if let Ok(kind) = std::env::var("BH_STORAGE_FULL") {
            let p = tmp(&format!("full_{kind}"));
            let e = DatabaseEnvironment::new(&p, 4, 16).unwrap();
            let d = Database::new(&e, "main").unwrap();
            match kind.as_str() {
                "put" => queue("put", &[Some(MDB_MAP_FULL)]),
                "commit" => queue("txn_commit", &[Some(MDB_MAP_FULL)]),
                _ => {
                    queue("txn_commit", &[Some(MDB_MAP_FULL)]);
                    assert!(e.start_bulk_transaction());
                    e.finish_bulk_transaction();
                }
            }
            d.put("a", b"1");
            std::process::exit(3);
        }
        for kind in ["put", "commit", "bulk"] {
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "storage::lmdb::tests::a_full_map_ends_the_process",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("BH_STORAGE_FULL", kind)
                .output()
                .unwrap();
            assert_eq!(out.status.code(), Some(0), "{kind}");
            assert!(String::from_utf8_lossy(&out.stderr).contains("Database map is full."), "{kind}");
        }
    }

    #[test]
    fn roundtrip_and_nil() {
        let p = tmp("rt");
        let e = DatabaseEnvironment::new(&p, 4, 16).unwrap();
        let d = Database::new(&e, "main").unwrap();
        assert!(d.put("a", b"hello"));
        assert_eq!(d.get("a").unwrap(), b"hello");
        assert!(d.has_key(Some("a")));
        assert!(d.set_data(Some(&[]), Some("e")));
        assert_eq!(d.get("e"), Some(vec![]));
        assert!(d.set_data(None, Some("a")));
        assert!(!d.has_key(Some("a")));
        assert!(d.remove("missing"));
        assert_eq!(d.get("missing"), None);
        assert!(!d.set_data(Some(b"x"), None));
        assert_eq!(d.data_for_key(None), None);
        assert!(!d.remove_data_for_key(None));
        assert!(!d.has_key(None));
        // key sizes
        assert!(!d.put("", b"x"));
        assert!(!d.put(&"k".repeat(512), b"x"));
        assert!(d.put(&"k".repeat(511), b"x"));
        assert_eq!(d.get(""), None);
        let ents = d.entries();
        assert_eq!(ents.len(), 2);
        drop(d);
        drop(e);
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn bulk_and_txnid() {
        let p = tmp("bulk");
        let e = DatabaseEnvironment::new(&p, 4, 16).unwrap();
        let d = Database::new(&e, "main").unwrap();
        let t0 = txnid(&e);
        assert!(d.put("a", b"1"));
        assert_eq!(txnid(&e), t0 + 1);
        d.get("a");
        assert_eq!(txnid(&e), t0 + 1);
        assert!(e.start_bulk_transaction());
        let t = e.bulk_transaction();
        assert!(e.start_bulk_transaction());
        assert_eq!(t, e.bulk_transaction());
        d.put("b", b"2");
        d.put("c", b"3");
        assert!(e.finish_bulk_transaction());
        assert!(e.bulk_transaction().is_null());
        assert_eq!(txnid(&e), t0 + 2);
        assert!(e.finish_bulk_transaction());
        e.start_bulk_transaction();
        d.get("a");
        e.finish_bulk_transaction();
        assert_eq!(txnid(&e), t0 + 2);
        // drop commits pending bulk
        e.start_bulk_transaction();
        d.put("z", b"9");
        drop(d);
        assert!(e.bulk_transaction().is_null());
        let d = Database::new(&e, "main").unwrap();
        assert_eq!(d.get("z").unwrap(), b"9");
        drop(d);
        drop(e);
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn multi_db_and_dbs_full() {
        let p = tmp("multi");
        let e = DatabaseEnvironment::new(&p, 2, 16).unwrap();
        let a = Database::new(&e, "main").unwrap();
        a.put("k", b"v");
        let b = Database::new(&e, "blocks").unwrap();
        assert!(Database::new(&e, "dw").is_none());
        let a2 = Database::new(&e, "main").unwrap();
        assert_eq!(a2.get("k").unwrap(), b"v");
        assert_eq!(b.get("k"), None);
        drop((a, b, a2));
        drop(e);
        let _ = std::fs::remove_dir_all(&p);
    }
}

/// Failure injection for the unit tests: the next calls of an operation return the queued codes instead of running
/// (`None` lets one call through).
#[cfg(test)]
pub(crate) mod inject {
    use std::cell::RefCell;
    use std::collections::{HashMap, VecDeque};
    use std::os::raw::c_int;

    thread_local! {
        static QUEUES: RefCell<HashMap<&'static str, VecDeque<Option<c_int>>>> = RefCell::new(HashMap::new());
    }

    /// Queue `codes` for `op`.
    pub fn queue(op: &'static str, codes: &[Option<c_int>]) {
        QUEUES.with(|q| q.borrow_mut().entry(op).or_default().extend(codes.iter().copied()));
    }

    /// The injected code for this call of `op`, if any.
    pub fn take(op: &'static str) -> Option<c_int> {
        QUEUES.with(|q| q.borrow_mut().get_mut(op).and_then(|v| v.pop_front()).flatten())
    }
}
