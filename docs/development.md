# Developing Blockheads Server

Build, test and release notes for contributors.

## Interoperability

This is an independent reimplementation written for interoperability: it speaks the same wire protocol, reads and
writes the same save format and prints the same command-line help as the original dedicated server. It contains
**no original binaries or assets**. Compatibility tables that are required for protocol and save compatibility
(item ids, recipes, wire and save layouts, loot and price tables, name lists, CLI option table and help text) are
included as data. Parts of the numeric and text layers are ports of third-party code (glibc, fdlibm, GNUstep-base);
see [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md).

## Build and test

```sh
cargo build --workspace
cargo test --workspace
```

Workspace crates: `blockheads_server` (library and binary), `bh_testclient`
(protocol test client and differential harness), `bh_tools` (probes).

Some tests compare against reference material that is not part of this
repository. They skip cleanly unless these environment variables are set:

- `BH_PRIVATE_DOCS` - directory with optional golden fixtures (`protocol_vectors/`, `plist_golden/`, `app_cli_getopt/`, `cli_golden.json`, `enums.md`, ...)
- `BH_REFENV` - reference environment directory (needs docker)
- `BH_REF_BINARY` - reference binary for gdb RNG traces
- `BH_PARITY=1` - enable the docker-based parity tests

Coverage: `scripts/coverage-gate.sh` (what CI runs, without the reference environment or docker; needs cargo-llvm-cov).
The whole workspace is gated at 100 % line coverage: `blockheads_server` and `bh_testclient` at 100 % each, and the mod
host seam (with the 0xBC protocol) and the test client's modded mode file by file as well. The floors never go down. Not
measured: the probe tools, the command-line mains of the test client, and `*_golden.rs` (tests that replay oracle
fixtures; they skip in CI). `scripts/mods-coverage.sh` is the quick local check of the mod host part.

Keeping it at 100 %. `llvm-cov report` scores every function by its instantiations: a function counts its most covered
instantiation per file group, and the copies compiled into different test binaries are different instantiations. So:
- **One-binary rule.** Cover every function (and every closure in it) completely inside ONE test binary, preferably the
  `blockheads_server` lib unit tests. Covering half of it from the lib tests and half from `tests/` or from
  `bh_testclient` leaves lines missed in the report although a merged view shows them covered.
- Generic functions (`impl Into`, `impl AsRef`, `impl FnOnce`, `impl Iterator` parameters) are one function per type: keep
  a thin generic wrapper and put the body in a non-generic function.
- Closures that never run count as missed lines: no `unwrap_or_else(|| panic!(..))` or `.map_or(0, |x| ..)` whose
  closure never gets called in a test; use `expect`, eager `unwrap_or`, or a function path (`.map_or(0, Vec::len)`,
  `.and_then(PlistValue::as_str)`); lock guards use `unwrap_or_else(std::sync::PoisonError::into_inner)`. In test
  helpers, no `panic!` arms and no `.all(|..|)` over a list that is always empty.
- `let .. else { unreachable!() }` on one line (scrutinee in its own `let`), and no `else if let` that always matches.
- Tests that need the reference fixtures go into `<module>_golden.rs` (`#[cfg(test)] #[path = "x_golden.rs"] mod golden;`).
- Test code is measured like any other: platform-specific tests are gated with `cfg` (the gate runs on Linux).

### Developing against a local mod-sdk

`bhmod-core`, `bhmod-wasm`, `bhmod-manifest`, `bhmod-proto` and `bhmod-content` come from `https://github.com/Blockheads-Club/mod-sdk` at the `rev` pinned in the
workspace `Cargo.toml`. To build against a local checkout instead (for example `../mod-sdk` next to this repository),
patch the git source on the command line; nothing in the repository changes except `Cargo.lock`, which you must not
commit in that state:

```sh
P='patch."https://github.com/Blockheads-Club/mod-sdk"'
cargo test --workspace \
  --config "$P.bhmod-core.path=\"../mod-sdk/crates/bhmod-core\"" \
  --config "$P.bhmod-wasm.path=\"../mod-sdk/crates/bhmod-wasm\"" \
  --config "$P.bhmod-manifest.path=\"../mod-sdk/crates/bhmod-manifest\"" \
  --config "$P.bhmod-proto.path=\"../mod-sdk/crates/bhmod-proto\"" \
  --config "$P.bhmod-content.path=\"../mod-sdk/crates/bhmod-content\""
git checkout Cargo.lock   # before committing
```

When a mod-sdk change lands, bump the five `rev`s to its commit and run
`cargo update -p bhmod-core -p bhmod-wasm -p bhmod-manifest -p bhmod-proto -p bhmod-content`.

### Hot reload, `/mods` and the watcher

`mods/admin.rs` implements the admin command `/mods` (list, `reload [id]`, `timings [id]`, `timings reset [id]`); admins
and the console only, handled before the built-in and the mods' own commands (`mods` is in `BUILTIN_COMMANDS`). The work
is `bhmod_core::ModHost::reload` / `timings` (mod-sdk `docs/hot-reload.md`). After a reload that changed anything,
`ServerMods::refresh_catalog` builds the offered client packages again (players who join later get the new ones; the
content registry is not rebuilt). `BLOCKHEADS_MODS_WATCH=1` creates a `bhmod_core::watch::Watcher` that `poll()` looks at;
there is no CLI flag because the help text is frozen. Live tests: `crates/bh_testclient/tests/mods_reload.rs`
(port 15230/15231), part of `scripts/mods-coverage.sh`.

### Mod settings

`bhmod_core::ModHost` owns mod settings (mod-sdk `docs/settings.md`): `/mods settings <mod>` and
`/mods set <mod> <key> <value>` go to `ModHost::settings_command` from the server's own `/mods` (admins from the
server's lists and the console; `Bridge::player_role`); `/mod` is never touched, it stays the built-in modlist command.
Hot reload of `[<id>.settings]` in `mods.toml` from `poll()` and the `<id>:settings-changed` event. The catalog honours
the admin's `client-required` override (`bhmod_core::config::client_required`). The host app edits `mods.toml` with
`toml_edit` (`mods::edit_config`, `bhmod_core::config::write_setting`), so comments and every mod's settings survive
on/off and grant changes. 0xBC `SETTINGS_SCHEMA` / `SETTINGS_SET` / `SETTINGS_RESULT` go through
`bhmod_core::netsettings::SettingsNet` (admin check = the server's admin list), only while `settings` is served.

### Voice (API 0.3.4)

Voice frames travel only over the dedicated UDP voice port; control (`VOICE_KEY`, `VOICE_STATE`, `VOICE_CTRL`) stays
on reliable 0xBC, and an in-band `VOICE_FRAME` (op 0x16) is dropped. `mods/voice.rs` holds the socket and its thread;
`mods/voice_glue.rs` glues mod-sdk's `bhmod_voice::server::VoiceServer` to it: `VoiceGlue` is `GameBridge::voice`
(the voice mod's policy; of `set-transport` only the port counts), `UdpGlue` the socket handler (binds a peer to
the address it authenticated from; reliable output of the UDP thread is queued for the server thread).
`ServerMods::sync_voice` (at load, after a reload, every poll) binds the socket while a loaded mod holds the `voice`
permission, on `VoiceGlue::wanted_port`: the mod's non-zero `udp_port`, else `HostConfig.voice.udp_port`, else the
game port + 1. `SERVER_HELLO.features` lists `voice` (and `voiceUdpPort`) only while the socket is bound; a port that
cannot be opened is logged once (`voice: UDP port N could not be opened (...); voice chat is off`) and voice stays
off. No status channel carries the voice port to the host app yet (it sees the log line). `settings` is listed while
a running mod has server settings: without such mods nothing new goes on the wire. The ENet unsequenced send of the
earlier in-band design was removed. The bridge also answers `tile_solid` (`tileIsSolid`), `world_width`,
`platform`, `player_sleeping`, `player_alive` (the player's first blockhead, as `player_pos`) and `player_has_mod`
(`READY.loaded`); each world step compares the joined players' blockheads with the last step and fires
`player:sleep|wake|respawn` (read only). The voice mod keeps admin mutes and gains in its own storage. Live test:
`crates/bh_testclient/tests/voice_mod.rs` (game ports 15490-15499, voice on the next port; the fixture
`tests/data/mods/voice/voice.wasm` is mod-sdk's `examples/voice-rust` built by `scripts/build-wasm.sh`).

### The 0xBC mod protocol (server side)

`crates/blockheads_server/src/mods/net.rs` implements the server side of mod-sdk `docs/protocol-0xbc.md`;
`mods/catalog.rs` builds what is offered. Everything is encoded by `bhmod-proto`. The seam adds two calls:
`ModHooks::mod_packet` (the dispatcher routes `0xBC` from approved and waiting peers, only when a mod host is
installed) and the `bhcModLoader` argument of `ModHooks::pre_login` (the 0x1f check, then `player-pre-login`).

- Offered: every enabled package with a client half (its server half, if any, must have loaded), plus loaded
  server-only mods for information. Unpacked folders are packed in memory with the deterministic `.bhmod` writer.
  Listed without bytes (never served, logged): client mods with `native`; never offered: packages over 32 MiB or beyond 128 MiB in total.
  `indexListed` is always false (the server does not read the index; clients check it themselves).
- Downloads: 8 chunks in flight per peer, sliding with every `FILE_REQUEST`; at most 8 peers download at once
  (`FILE_ERROR` BUSY); a lower offset restarts there (resume).
- Channels: the server owns the wire ids (assigned on first use, never reused). `MOD_LIST` carries the open channels;
  later ones reach clients in the game as `CHANNEL_REGISTER` (before the first message on it, and on the next poll).
  A client `CHANNEL_REGISTER` is ignored. Server mods sending to a player without the loader are a silent no-op.
  `SERVICE_CALL` / `SERVICE_RESULT` (API 0.3.1) are accepted from modded players in the game only (`Inbound` in
  `net.rs`): a call goes to `ModHost::remote_call_in` (the caller mod must be in the player's `READY.loaded`; the provider's
  `remote = true`, the per-player token bucket from `mods::remote_limits()` and the provider's time budget apply) and the
  result is sent back at once; server mod calls to a player go out through `GameBridge::service_send` (`unavailable`
  for a player without the loader). Tests: `crates/bh_testclient/tests/mods_services.rs`.
- `bhcModLoader` is never stored: the player info keeps only the listed vanilla keys.
- Rejections: modded clients get `REJECT_TEXT` before `0x26` for a protocol mismatch, missing or stale acceptance
  and `player-pre-login` refusals; vanilla clients never get a 0xBC byte.
- Vanilla clients on a server with client-required mods (no `HELLO`, no `bhcModLoader`) are not rejected at the
  `bhcModLoader` check but held in a limbo (`ModHooks::limbo_join`): the server sends a 0x01 world summary (the
  world's own summary for a plain player, `welcomeMessage` = `mods::LIMBO_TEXT`; the vanilla game always shows it as
  its "WELCOME TO <WORLD>!" popup), without approving the player (no `connected_clients` entry, no player record, no
  `client_connected`, no `player-join` / `player-pre-login`). After `mods::LIMBO_GRACE` (20 s), or at once when the
  peer sends any packet, it is disconnected without any `0x26` (a reason would replace the readable text with a misleading one); a disconnect just forgets it. Servers without
  client-required mods are unchanged.

The test client's modded mode (`bh_testclient::ModClient`, `bh_client join ADDR --alias A --modded CACHE_DIR`)
does the client side: HELLO, list, downloads into a cache (resume, SHA-256 check), READY, the 0x1f key and
channels. Tests: `crates/bh_testclient/tests/mods_protocol.rs` (also compares packets with the mod-sdk vectors; set
`BHMOD_SDK_DIR` to a mod-sdk checkout to check the copies).

### Mod runtimes on every target

- WASM (`bhmod-wasm`, wasmtime): Cranelift on x86_64 and arm64, the Pulley interpreter on armv7. armv7 builds use
  `--cfg pulley_assume_llvm_makes_tail_calls` (set in `.cargo/config.toml`), which makes Pulley 1.3–2x faster.
  Compiled mods are cached in `<mods dir>/.cache/`.
- Luau (`mlua`) is C++: Android builds link the static C++ runtime (`CXXSTDLIB_<target>=c++_static` in
  `.cargo/config.toml`, `c++abi` from the build script) so the app ships no `libc++_shared.so`.

Commits follow [Conventional Commits](https://www.conventionalcommits.org);
releases are automatic. Once CI passes for a push to `dev`, `.github/workflows/auto-release.yml` runs
semantic-release (`.releaserc.json`): `fix`/`perf` commits cut a patch release, `feat` a minor one,
`BREAKING CHANGE` a major one, other types none. It sets the version (`scripts/set-version.ts`), commits
`chore(release): vX.Y.Z`, tags it, opens a draft release with the generated notes and dispatches
`.github/workflows/release.yml` with `publish`, which attaches every asset, publishes the release, rebuilds the
website and pushes the docker image. A hand-pushed tag `vX.Y.Z` still builds into a draft release to publish by hand.

The `faithful-crashes` cargo feature (emulating original crash behaviour) is
off by default.

## Data directory

Worlds live in `<home>/GNUstep/Library/ApplicationSupport/TheBlockheads` on every OS (`<home>` is `$HOME`, or
`%USERPROFILE%` on Windows), exactly like the original. Set `BLOCKHEADS_DATA_DIR` to use another directory as that
`TheBlockheads` folder (the Docker image uses `/data`). There is no CLI flag for it, so `--help` stays identical to
the original.

## Embedding

`blockheads_server::host` runs the server inside another program (used by the desktop and Android host app):
`start(config, log_callback)` returns a handle with `status()`, `send_command()`, `stop()`, and
`list_worlds` / `create_world` / `delete_world` manage worlds. A server can be started again after it stopped,
in the same process.

`HostConfig::shared_names` (not in the original, `false` = vanilla) makes the player name the identity, like a
Minecraft offline-mode server: a join under a known name from a new device plays as that player (the player id is the
MD5 of the lower-cased name, so blockheads, inventory, ownership and admin/mod/white/black state already follow the
name), and a session that is online under that name is told "You logged in from another location." and replaced.
The new join is checked first (password, white/black list, IP bans), so a failing join never logs anyone out. The
host app turns it on by default (Settings, "Same name from any device"); the CLI keeps the vanilla rules.

## Host app (desktop and Android)

`app/` is a Tauri 2 + SvelteKit app ("Blockheads Server") that runs the server in-process on a background thread:
world list/create/delete, start/stop with live log and connected players, LAN address + QR code for phones, admin/mod/
white/black list editors, English and Turkish UI. On Android a Kotlin foreground service (notification, partial wake
lock, Wi-Fi lock) keeps it running in the background; the APK contains arm64-v8a, armeabi-v7a and x86_64 libraries.

```sh
cd app
bun install
bun tauri dev                                   # desktop
bun tauri build                                 # desktop bundle
bun tauri android build --debug --apk --target aarch64 --target armv7 --target x86_64
bun run test && bun run test:e2e                # vitest + Playwright (mocked Tauri commands)
(cd src-tauri && cargo test --lib)              # command layer
```

Android builds need JDK 17 to 21 (Gradle does not support newer JDKs yet), the Android SDK and NDK 27.
Set `CARGO_INCREMENTAL=0` when building several ABIs in parallel.

### App updates

The app checks the GitHub releases of this repository (`src-tauri/src/update.rs`, UI in `src/lib/update.svelte.ts`).
Only the app is updated; the game server code is not involved. Like the Blockheads PC launcher, a check (by hand,
or automatically at start and at most once a day) only shows the update: nothing is downloaded or installed until
the user clicks to update, and the user then picks when the server restarts (now, when nobody is online, or in five
minutes with a chat warning).

- Feed: `https://api.github.com/repos/Blockheads-Club/blockheads-server/releases`. Drafts are ignored. The stable
  channel takes plain `vX.Y.Z` tags that are not marked as prerelease; "Stable and test versions" also takes
  prereleases and `vX.Y.Z-pre.N` tags.
- In place (Windows, macOS, AppImage): `tauri-plugin-updater` reads the chosen release's `latest.json`, downloads the
  artifact, checks its minisign signature against the public key in `tauri.conf.json` (`plugins.updater.pubkey`),
  installs it and relaunches (Windows: the NSIS installer in passive mode restarts the app).
- Fallback (Linux .deb/.rpm, or a release without `latest.json`): the platform asset is downloaded into
  `<app data>/updates`, checked against the release's `SHA256SUMS.txt` and handed to the system installer (a
  release without checksums opens the download page instead). Assets are matched by the bundle names of
  `release.yml` (`*_x64-setup.exe`, `*_arm64-setup.exe`, `*_aarch64.dmg`, `*_x64.dmg`, `*_amd64.AppImage`,
  `*_aarch64.AppImage`, `*_amd64.deb`, `*_arm64.deb`, `*.x86_64.rpm`, `*-android.apk`).
- Before any restart the server is saved and stopped and the loaded world is copied to
  `<data dir>/update-backups/<world>-<ms>` (newest three kept). `updates/pending.json` tells the next start which
  version was expected and whether to start the server again.
- Android: checked at every launch. The universal `*-android.apk` (all three ABIs) is downloaded in the app with
  progress and verified against `SHA256SUMS.txt` (a release without it opens the browser instead). On the user's
  tap the app saves and stops the server, backs up the world and hands the file to the system installer through the
  hostservice plugin (`installApk`, a `FileProvider` URI under `files/updates/`). When "install unknown apps" is not
  allowed yet (`REQUEST_INSTALL_PACKAGES`), the settings screen for it opens before anything is stopped and the
  download is kept. This needs every APK signed with the same persistent release key (`ANDROID_KEYSTORE_*` secrets).
- Debug builds can point the check at another feed with `BHS_UPDATE_FEED=<url>`.

Release side: `createUpdaterArtifacts` is on, so `tauri build` needs `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (repository secrets; `release.yml` fails without them). The desktop jobs upload
the signed artifacts with their `.sig` files (macOS: `blockheads-server_<version>_<arch>.app.tar.gz`, renamed because
Tauri gives both architectures the same name; spaces in names become dots as on GitHub), and the publish job writes
`latest.json` for all six desktop targets with `.github/scripts/latest-json.sh` before `SHA256SUMS.txt`. Local
bundle builds without the key: `bun tauri build -c '{"bundle":{"createUpdaterArtifacts":false}}'`, or sign with a
throw-away key from `bun tauri signer generate`.
