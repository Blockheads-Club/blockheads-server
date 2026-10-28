//! What the server offers modded clients (0xBC `MOD_LIST`): every enabled package in the mods directory with a
//! client half, plus the loaded server-only mods for information. Built once when the host loads.
//!
//! Rules: a client half whose server half did not load is not offered; a client half that needs `native` is listed
//! (with its hash, so a server can require it) but its bytes are never sent: a server cannot push native code, the
//! player installs it from the public index (same SHA-256) after confirming; packages above [`MAX_PACKAGE`] or
//! beyond [`MAX_TOTAL`] are not offered. Unpacked mod directories are packed with the
//! deterministic `.bhmod` writer, so they have a hash and can be downloaded like zips.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use bhmod_core::config::{ModsConfig as AdminConfig, FILE_NAME};
use bhmod_core::discover::{discover, Package};
use bhmod_manifest::manifest::{Manifest, PlayerPermission, WorldAccess};
use bhmod_manifest::package::DirSource;
use bhmod_proto::{mods_hash, Hash, ModInfo, ModPermissions};

/// Largest package offered for download.
pub const MAX_PACKAGE: u64 = 32 * 1024 * 1024;
/// Largest sum of all offered packages.
pub const MAX_TOTAL: u64 = 128 * 1024 * 1024;

/// Where a package's bytes come from.
#[derive(Debug, Clone)]
pub enum Bytes {
    /// A `.bhmod` in the mods directory (read on demand).
    File(PathBuf),
    /// An unpacked directory, packed in memory.
    Memory(Rc<Vec<u8>>),
}

/// One listed mod.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The `MOD_LIST` entry.
    pub info: ModInfo,
    /// Its package; `None` for listed-only mods (server-only, and native client mods), which are never served.
    pub bytes: Option<Bytes>,
}

impl Entry {
    /// True when the entry has a client half (served, or native and installed by the player from the index).
    pub fn is_client(&self) -> bool {
        self.info.sides.iter().any(|s| s == "client")
    }

    /// Reads `len` bytes at `offset` of the package.
    pub fn read(&self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        let mut buf = vec![0; len];
        match &self.bytes {
            Some(Bytes::File(p)) => {
                let mut f = File::open(p)?;
                f.seek(SeekFrom::Start(offset))?;
                f.read_exact(&mut buf)?;
            }
            Some(Bytes::Memory(m)) => {
                let mut c = Cursor::new(m.as_slice());
                c.seek(SeekFrom::Start(offset))?;
                c.read_exact(&mut buf)?;
            }
            None => return Err(io::Error::other("not served")),
        }
        Ok(buf)
    }
}

/// The offered mods.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    /// Sorted by id.
    pub entries: Vec<Entry>,
    /// `modsHash` over the client-required entries.
    pub mods_hash: Hash,
    /// True when some entry is client-required.
    pub client_required: bool,
}

/// Parses 64 hex characters.
pub fn parse_hash(hex: &str) -> Option<Hash> {
    if hex.len() != 64 || !hex.is_ascii() {
        return None;
    }
    let mut h = [0u8; 32];
    for (i, b) in h.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(h)
}

fn player_permission(p: PlayerPermission) -> &'static str {
    match p {
        PlayerPermission::Chat => "chat",
        PlayerPermission::Teleport => "teleport",
        PlayerPermission::Kick => "kick",
        PlayerPermission::Ban => "ban",
        PlayerPermission::Inventory => "inventory",
        PlayerPermission::Health => "health",
    }
}

fn info(m: &Manifest, sides: Vec<&str>, client_required: bool, sha256: Hash, size: u64) -> ModInfo {
    let p = &m.permissions;
    ModInfo {
        id: m.id.clone(),
        version: m.version.clone(),
        name: m.name.clone(),
        description: m.description.clone(),
        sides: sides.into_iter().map(str::to_string).collect(),
        client_required,
        sha256,
        size,
        permissions: ModPermissions {
            world: match p.world {
                WorldAccess::None => "none",
                WorldAccess::Read => "read",
                WorldAccess::Write => "write",
            }
            .to_string(),
            players: p.players.iter().map(|x| player_permission(*x).to_string()).collect(),
            net_http: p.net_http.clone(),
            channels: p.channels,
            native: p.native,
            voice: p.voice,
        },
        // The server does not read the public index; clients check it themselves.
        source_url: None,
        index_listed: false,
    }
}

/// The package bytes and their hash: a zip as it is, a directory packed in memory.
fn package_bytes(p: &Package) -> io::Result<(Bytes, Hash, u64)> {
    match p.sha256.as_deref().and_then(parse_hash) {
        Some(h) => Ok((Bytes::File(p.origin.clone()), h, std::fs::metadata(&p.origin)?.len())),
        None => {
            let mut src = DirSource::open(&p.origin)?;
            let zip = bhmod_manifest::pack::write_zip(&mut src, Cursor::new(Vec::new()))?.into_inner();
            let h = parse_hash(&bhmod_manifest::pack::sha256_hex(&zip)).expect("sha256 hex");
            let n = zip.len() as u64;
            Ok((Bytes::Memory(Rc::new(zip)), h, n))
        }
    }
}

/// Builds the catalog of `dir`. `server_loaded`: ids whose server half the host loaded. Problems go to `log`.
pub fn build(dir: &Path, server_loaded: &[String], log: &mut dyn FnMut(String)) -> Catalog {
    let admin =
        std::fs::read_to_string(dir.join(FILE_NAME)).ok().and_then(|t| AdminConfig::parse(&t).ok()).unwrap_or_default();
    let (packages, _) = discover(dir);
    from_packages(packages, &admin, server_loaded, (MAX_PACKAGE, MAX_TOTAL), log)
}

/// [`build`] over discovered packages, with `(per package, total)` size limits.
pub fn from_packages(
    packages: Vec<Package>,
    admin: &AdminConfig,
    server_loaded: &[String],
    (max_package, max_total): (u64, u64),
    log: &mut dyn FnMut(String),
) -> Catalog {
    let mut entries: BTreeMap<String, Entry> = BTreeMap::new();
    let mut total = 0u64;
    for p in packages {
        let m = &p.manifest;
        let id = m.id.clone();
        let loaded = server_loaded.contains(&id);
        let client = m.sides.client && m.entry.client.is_some();
        let config = admin.get(&id);
        if !config.enabled || entries.contains_key(&id) || !(client || loaded) {
            continue;
        }
        if client && m.entry.server.is_some() && !loaded {
            log(format!("not offering {id} to clients: its server half is not loaded"));
            continue;
        }
        let (bytes, sha, size) = match package_bytes(&p) {
            Ok(x) => x,
            Err(e) => {
                log(format!("not offering {id}: {e}"));
                continue;
            }
        };
        let entry = if client {
            // Native code is never sent: the entry only carries the hash the player's own install must match.
            let native = m.permissions.native;
            if native {
                log(format!("listing {id} without bytes: native client mods are installed by players from the index"));
            } else if size > max_package || total + size > max_total {
                log(format!("not offering {id} to clients: package too large ({size} bytes)"));
                continue;
            } else {
                total += size;
            }
            let sides = if m.entry.server.is_some() { vec!["server", "client"] } else { vec!["client"] };
            Entry {
                info: info(m, sides, bhmod_core::config::client_required(m, &config), sha, size),
                bytes: (!native).then_some(bytes),
            }
        } else {
            Entry { info: info(m, vec!["server"], false, sha, size), bytes: None }
        };
        entries.insert(id, entry);
    }
    let entries: Vec<Entry> = entries.into_values().collect();
    let required: Vec<&ModInfo> = entries.iter().map(|e| &e.info).filter(|i| i.client_required).collect();
    let mods_hash = mods_hash(required.iter().map(|i| (i.id.as_str(), i.version.as_str(), &i.sha256)));
    Catalog { client_required: !required.is_empty(), mods_hash, entries }
}

impl Catalog {
    /// The servable entry with this hash.
    pub fn served(&self, sha256: &Hash) -> Option<&Entry> {
        self.entries.iter().find(|e| e.bytes.is_some() && &e.info.sha256 == sha256)
    }

    /// True when nothing is listed.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests;
