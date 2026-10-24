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

/// `mdb_strerror(rc)` as a Rust string.
fn strerror(rc: c_int) -> String {
    unsafe {
        let p = ffi::mdb_strerror(rc);
        if p.is_null() {
            String::new()
        } else {
            CStr::from_ptr(p).to_string_lossy().into_owned()
        }
    }
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
            let rc = ffi::mdb_env_create(&mut env);
            if rc != 0 {
                log_fail("mdb_env_create", rc);
                return None;
            }
            let rc = ffi::mdb_env_set_mapsize(env, 0x1000);
            if rc != 0 {
                log_fail("mdb_env_set_mapsize", rc);
                ffi::mdb_env_close(env);
                return None;
            }
            let rc = ffi::mdb_env_set_maxdbs(env, max_databases as ffi::MDB_dbi);
            if rc != 0 {
                log_fail("mdb_env_set_maxdbs", rc);
                ffi::mdb_env_close(env);
                return None;
            }
            // `[path UTF8String]`: a C string ends at the first NUL.
            let cpath = c_string_prefix(path);
            let open_rc = ffi::mdb_env_open(env, cpath.as_ptr(), 0, 0o664);
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
            let rc2 = ffi::mdb_env_set_mapsize(env, mb << 20);
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
            let rc = unsafe { ffi::mdb_txn_begin(self.env, ptr::null_mut(), 0, &mut t) };
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
            let rc = unsafe { ffi::mdb_txn_commit(t) };
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
        let rc = unsafe { ffi::mdb_txn_begin(self.env, ptr::null_mut(), 0, &mut t) };
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
        let rc = unsafe { ffi::mdb_txn_commit(t) };
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
            let rc = ffi::mdb_txn_begin(env.env, ptr::null_mut(), 0, &mut t);
            if rc != 0 {
                log_fail("mdb_txn_begin", rc);
                return None;
            }
            let mut dbi: ffi::MDB_dbi = 0;
            let rc = ffi::mdb_dbi_open(t, cname.as_ptr(), ffi::MDB_CREATE, &mut dbi);
            if rc != 0 {
                log_fail("mdb_dbi_open", rc);
                ffi::mdb_txn_abort(t);
                return None;
            }
            let rc = ffi::mdb_txn_commit(t);
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
        let rc = unsafe { ffi::mdb_put(txn, self.dbi, &mut k, &mut v, 0) };
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
        let rc = unsafe { ffi::mdb_del(txn, self.dbi, &mut k, ptr::null_mut()) };
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
                let rc = ffi::mdb_txn_begin(self.env.env, ptr::null_mut(), ffi::MDB_RDONLY, &mut t);
                if rc != 0 {
                    return out;
                }
            }
            let mut cur: *mut ffi::MDB_cursor = ptr::null_mut();
            if ffi::mdb_cursor_open(t, self.dbi, &mut cur) == 0 {
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
