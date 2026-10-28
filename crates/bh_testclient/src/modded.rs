//! Modded client mode: the client side of the 0xBC mod protocol (mod-sdk `docs/protocol-0xbc.md`) on top of a
//! [`Session`]: `HELLO` after 0x23, the mod list, chunked downloads into a cache with resume and SHA-256 check,
//! `READY`, the 0x1f join with `bhcModLoader`, and mod channels. Every packet goes through `bhmod-proto`.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use bhmod_proto::{
    hex, Channel, FileError, FileRequest, Hash, Hello, Message, ModInfo, ModList, Ready, RejectText, ServerHello,
    ServiceCall, ServiceFailure, ServiceResult, CHUNK_SIZE, HELLO_TIMEOUT_MS, PACKET_TYPE, PROTO,
};
use blockheads_server::protocol::codec::t;
use sha2::{Digest, Sha256};

use crate::client::Rx;
use crate::session::{JoinOutcome, Session};

/// How the modded client behaves.
#[derive(Clone, Debug)]
pub struct LoaderOptions {
    /// `HELLO.proto`.
    pub proto: u64,
    /// `HELLO.loader`.
    pub loader: String,
    /// `HELLO.platform`.
    pub platform: String,
    /// `HELLO.api`.
    pub api: String,
    /// `HELLO.runtimes`.
    pub runtimes: Vec<String>,
    /// Package cache: `<sha256 hex>.bhmod` (complete) and `<sha256 hex>.part` (partial).
    pub cache_dir: PathBuf,
    /// Accept the server's mods. False declines: no download, no `READY`, no `bhcModLoader` for required mods.
    pub accept: bool,
    /// Ask for the mod list even when no client mod is required.
    pub always_list: bool,
    /// Packages of native client mods the player installed from the index (and confirmed). A server lists native
    /// mods without bytes, so they are never downloaded; a required native mod that is not in here is missing and
    /// the client does not join (like the game's loader).
    pub native_installed: Vec<Hash>,
    /// Stop a download once this many bytes are held (an interrupted transfer; the join then fails).
    pub interrupt_after: Option<u64>,
    /// How long to wait for `SERVER_HELLO`.
    pub hello_timeout_ms: u64,
    /// `HELLO.features` (optional protocol features such as `voice`, `settings`).
    pub features: Vec<String>,
}

impl LoaderOptions {
    /// Defaults: protocol 1, a PC loader with Luau and WASM, accepting everything.
    pub fn new(cache_dir: &Path) -> LoaderOptions {
        LoaderOptions {
            proto: PROTO,
            loader: "0.1.0".into(),
            platform: "pc-linux-x86_64".into(),
            api: "0.2.0".into(),
            runtimes: vec!["wasm".into(), "lua".into()],
            cache_dir: cache_dir.to_path_buf(),
            accept: true,
            always_list: false,
            native_installed: Vec::new(),
            interrupt_after: None,
            hello_timeout_ms: HELLO_TIMEOUT_MS,
            features: Vec::new(),
        }
    }
}

/// One package transfer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Download {
    /// The package.
    pub sha256: Hash,
    /// The offset of the first `FILE_REQUEST` (non-zero when resumed).
    pub resumed_from: u64,
    /// Offsets of the chunks received, in order.
    pub chunk_offsets: Vec<u64>,
    /// Complete and verified.
    pub complete: bool,
}

/// The modded client state of one connection.
#[derive(Clone, Debug)]
pub struct ModClient {
    /// Behaviour.
    pub opts: LoaderOptions,
    /// The server's `SERVER_HELLO` (`None`: vanilla server, or not asked yet).
    pub server: Option<ServerHello>,
    /// The server's `MOD_LIST`.
    pub list: Option<ModList>,
    /// The last `REJECT_TEXT`.
    pub reject: Option<RejectText>,
    /// `FILE_ERROR`s received.
    pub file_errors: Vec<FileError>,
    /// Transfers.
    pub downloads: Vec<Download>,
    /// Channel name -> wire id (`MOD_LIST.channels`, `CHANNEL_REGISTER`).
    pub channels: BTreeMap<String, u16>,
}

fn is_op(rx: &Rx, op: u8) -> bool {
    rx.data.len() >= 2 && rx.data[0] == PACKET_TYPE && rx.data[1] == op
}

fn bad(msg: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

fn decode(rx: &Rx) -> io::Result<Message> {
    Message::decode(&rx.data).map_err(|e| bad(format!("0xBC: {e}")))
}

impl ModClient {
    /// A client with these options.
    pub fn new(opts: LoaderOptions) -> ModClient {
        ModClient {
            opts,
            server: None,
            list: None,
            reject: None,
            file_errors: Vec::new(),
            downloads: Vec::new(),
            channels: BTreeMap::new(),
        }
    }

    fn path(&self, sha: &Hash, ext: &str) -> PathBuf {
        self.opts.cache_dir.join(format!("{}.{ext}", hex(sha)))
    }

    /// True when the cache holds the complete package.
    pub fn is_cached(&self, sha: &Hash) -> bool {
        self.path(sha, "bhmod").is_file()
    }

    /// Hashes of the complete packages in the cache.
    pub fn cached(&self) -> Vec<Hash> {
        let mut out = Vec::new();
        for e in fs::read_dir(&self.opts.cache_dir).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some(h) = name.strip_suffix(".bhmod") else { continue };
            if h.len() == 64 {
                let mut sha = [0u8; 32];
                if (0..32).all(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).map(|b| sha[i] = b).is_ok()) {
                    out.push(sha);
                }
            }
        }
        out.sort();
        out
    }

    /// The `HELLO` this client sends.
    pub fn hello(&self) -> Message {
        Message::Hello(Hello {
            proto: self.opts.proto,
            loader: self.opts.loader.clone(),
            platform: self.opts.platform.clone(),
            api: self.opts.api.clone(),
            runtimes: self.opts.runtimes.clone(),
            cached: self.cached(),
            features: self.opts.features.clone(),
        })
    }

    /// The full modded join: waits for 0x23, sends `HELLO`, then (modded server) list, downloads and `READY`, and
    /// the 0x1f with `bhcModLoader`; against a vanilla server it carries on as a vanilla client.
    pub fn join(&mut self, s: &mut Session, timeout_ms: u64) -> io::Result<JoinOutcome> {
        if s.wait_for(t::SERVER_INITIAL_CONTACT, timeout_ms)?.is_none() {
            return Ok(if s.state.disconnected { JoinOutcome::Disconnected } else { JoinOutcome::NoContact });
        }
        fs::create_dir_all(&self.opts.cache_dir)?;
        s.client.send(&self.hello().encode())?;
        let reply = s.wait_for_matching(self.opts.hello_timeout_ms, &mut |rx| is_op(rx, 0x01) || is_op(rx, 0x08))?;
        let Some(reply) = reply else {
            // No SERVER_HELLO: a vanilla server.
            s.id.mod_loader = None;
            s.send_player_info()?;
            return self.outcome(s, timeout_ms);
        };
        let hello = match decode(&reply)? {
            Message::ServerHello(h) => h,
            // REJECT_TEXT (protocol mismatch), then 0x26.
            _ => return self.outcome(s, timeout_ms),
        };
        self.server = Some(hello.clone());
        let mut accepted = !hello.client_required;
        if hello.client_required || self.opts.always_list {
            let list = self.request_list(s, timeout_ms)?;
            let missing = list
                .mods
                .iter()
                .any(|m| m.permissions.native && m.client_required && !self.opts.native_installed.contains(&m.sha256));
            if self.opts.accept && !missing {
                for m in list.mods.iter().filter(|m| m.sides.iter().any(|x| x == "client") && !m.permissions.native) {
                    if !self.is_cached(&m.sha256) && !self.download(s, m, timeout_ms)? {
                        return Ok(JoinOutcome::Disconnected);
                    }
                }
                let loaded = list
                    .mods
                    .iter()
                    .filter(|m| m.sides.iter().any(|x| x == "client"))
                    .map(|m| (m.id.clone(), m.version.clone()))
                    .collect();
                let ready = Ready { accepted_hash: hello.mods_hash, loaded };
                s.client.send(&Message::Ready(ready).encode())?;
                accepted = true;
            }
        }
        s.id.mod_loader = accepted.then(|| (PROTO as i64, hex(&hello.mods_hash)));
        s.send_player_info()?;
        self.outcome(s, timeout_ms)
    }

    /// Waits for the join result (0x01 or 0x26) and picks up a `REJECT_TEXT`.
    fn outcome(&mut self, s: &mut Session, timeout_ms: u64) -> io::Result<JoinOutcome> {
        let r = s.wait_for_any(&[t::WORLD_SUMMARY, t::SERVER_CONNECTION_REJECTION], timeout_ms)?;
        for rx in s.received().iter().filter(|rx| is_op(rx, 0x08)) {
            let Message::RejectText(r) = decode(rx)? else { continue };
            self.reject = Some(r);
        }
        Ok(match r {
            Some(rx) if rx.data[0] == t::WORLD_SUMMARY => JoinOutcome::Approved,
            Some(rx) => JoinOutcome::Rejected { reason: rx.data.get(1).copied().unwrap_or(0) },
            None if s.state.disconnected => JoinOutcome::Disconnected,
            None => JoinOutcome::NoContact,
        })
    }

    /// `LIST_REQUEST` -> `MOD_LIST`.
    pub fn request_list(&mut self, s: &mut Session, timeout_ms: u64) -> io::Result<ModList> {
        s.client.send(&Message::ListRequest.encode())?;
        let rx =
            s.wait_for_matching(timeout_ms, &mut |rx| is_op(rx, 0x03))?.ok_or_else(|| bad("no MOD_LIST".into()))?;
        let Message::ModList(list) = decode(&rx)? else { return Err(bad("not a MOD_LIST".into())) };
        for c in &list.channels {
            self.channels.insert(c.name.clone(), c.id);
        }
        self.list = Some(list.clone());
        Ok(list)
    }

    /// Downloads one package into the cache (resuming a partial file). False when the server sent `FILE_ERROR` or
    /// the transfer was interrupted on purpose ([`LoaderOptions::interrupt_after`]).
    pub fn download(&mut self, s: &mut Session, m: &ModInfo, timeout_ms: u64) -> io::Result<bool> {
        let part = self.path(&m.sha256, "part");
        let mut have = fs::read(&part).unwrap_or_default();
        have.truncate(have.len() / CHUNK_SIZE * CHUNK_SIZE);
        let mut d =
            Download { sha256: m.sha256, resumed_from: have.len() as u64, chunk_offsets: Vec::new(), complete: false };
        let request = |s: &mut Session, offset: u64| {
            s.client.send(&Message::FileRequest(FileRequest { sha256: m.sha256, offset }).encode())
        };
        request(s, d.resumed_from)?;
        let mut since_ack = 0;
        let result = loop {
            if have.len() as u64 >= m.size {
                break true;
            }
            let rx = s
                .wait_for_matching(timeout_ms, &mut |rx| is_op(rx, 0x05) || is_op(rx, 0x06))?
                .ok_or_else(|| bad("download stalled".into()))?;
            match decode(&rx)? {
                Message::FileChunk(c) if c.sha256 == m.sha256 => {
                    if c.offset != have.len() as u64 || c.total != m.size {
                        return Err(bad(format!("chunk at {} of {}, expected {}", c.offset, c.total, have.len())));
                    }
                    d.chunk_offsets.push(c.offset);
                    have.extend_from_slice(&c.data);
                    fs::write(&part, &have)?;
                    since_ack += 1;
                    if self.opts.interrupt_after.is_some_and(|n| have.len() as u64 >= n) {
                        break false;
                    }
                    if since_ack >= 4 && (have.len() as u64) < m.size {
                        request(s, have.len() as u64)?;
                        since_ack = 0;
                    }
                }
                Message::FileError(e) => {
                    self.file_errors.push(e);
                    break false;
                }
                _ => {}
            }
        };
        if result {
            let got: Hash = Sha256::digest(&have).into();
            if got != m.sha256 {
                let _ = fs::remove_file(&part);
                return Err(bad(format!("{}: SHA-256 mismatch", m.id)));
            }
            fs::rename(&part, self.path(&m.sha256, "bhmod"))?;
            request(s, m.size)?;
            d.complete = true;
        }
        self.downloads.push(d);
        Ok(result)
    }

    fn learn_channels(&mut self, s: &Session) {
        for rx in s.received().iter().filter(|rx| is_op(rx, 0x11)) {
            let Ok(Message::ChannelRegister(c)) = Message::decode(&rx.data) else { continue };
            self.channels.insert(c.name, c.id);
        }
    }

    /// Sends a channel payload by name.
    pub fn send_channel(&mut self, s: &mut Session, name: &str, data: &[u8]) -> io::Result<()> {
        s.step()?;
        self.learn_channels(s);
        let id = *self.channels.get(name).ok_or_else(|| bad(format!("channel {name} is not known")))?;
        s.client.send(&Message::Channel(Channel { channel_id: id, payload: data.to_vec() }).encode())
    }

    /// Waits for the next channel payload: `(name, data)`.
    pub fn wait_channel(&mut self, s: &mut Session, timeout_ms: u64) -> io::Result<Option<(String, Vec<u8>)>> {
        let Some(rx) = s.wait_for_matching(timeout_ms, &mut |rx| is_op(rx, 0x10))? else { return Ok(None) };
        let Message::Channel(c) = decode(&rx)? else { return Err(bad("not a CHANNEL".into())) };
        self.learn_channels(s);
        let name = self.channels.iter().find(|(_, id)| **id == c.channel_id).map(|(n, _)| n.clone());
        Ok(Some((name.ok_or_else(|| bad(format!("unknown channel id {}", c.channel_id)))?, c.payload)))
    }

    /// Sends a `SERVICE_CALL` (a client mod calls a service of a server mod).
    pub fn call_service(&mut self, s: &mut Session, call: ServiceCall) -> io::Result<()> {
        s.client.send(&Message::ServiceCall(call).encode())
    }

    /// Waits for the next `SERVICE_RESULT` (the answer to a call of this client).
    pub fn wait_service_result(&mut self, s: &mut Session, timeout_ms: u64) -> io::Result<Option<ServiceResult>> {
        let Some(rx) = s.wait_for_matching(timeout_ms, &mut |rx| is_op(rx, 0x14))? else { return Ok(None) };
        let Message::ServiceResult(r) = decode(&rx)? else { return Err(bad("not a SERVICE_RESULT".into())) };
        Ok(Some(r))
    }

    /// Waits for the next `SERVICE_CALL` (a server mod calls a service of this client).
    pub fn wait_service_call(&mut self, s: &mut Session, timeout_ms: u64) -> io::Result<Option<ServiceCall>> {
        let Some(rx) = s.wait_for_matching(timeout_ms, &mut |rx| is_op(rx, 0x13))? else { return Ok(None) };
        let Message::ServiceCall(c) = decode(&rx)? else { return Err(bad("not a SERVICE_CALL".into())) };
        Ok(Some(c))
    }

    /// Answers a `SERVICE_CALL` of the server: the result value (canonical CBOR) or a failure `(code, message)`.
    pub fn answer_service(&mut self, s: &mut Session, id: u32, outcome: Result<Vec<u8>, (u8, &str)>) -> io::Result<()> {
        let outcome = outcome.map_err(|(code, message)| ServiceFailure { code, message: message.to_string() });
        s.client.send(&Message::ServiceResult(ServiceResult { id, outcome }).encode())
    }
}
