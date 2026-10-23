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

Coverage: `scripts/coverage-gate.sh` (what CI runs; needs cargo-llvm-cov). The policy is 100 % line coverage. The mod
host seam (with the 0xBC protocol) and the test client's modded mode are gated at 100 %, file by file; the rest of the
port is gated at per-crate floors (server 80 %, test client library 70 %, measured in CI 81.07 % and 70.6 %) that only go up.
The probe tools and the command-line mains of the test client are not measured; the docker-only parts of the harness
count as they are (their tests skip without the reference environment). `scripts/mods-coverage.sh` is the quick local
check of the 100 % part.

### Developing against a local mod-sdk

`bhmod-core`, `bhmod-wasm`, `bhmod-manifest` and `bhmod-proto` come from `https://github.com/Blockheads-Club/mod-sdk` at the `rev` pinned in the
workspace `Cargo.toml`. To build against a local checkout instead (for example `../mod-sdk` next to this repository),
patch the git source on the command line; nothing in the repository changes except `Cargo.lock`, which you must not
commit in that state:

```sh
P='patch."https://github.com/Blockheads-Club/mod-sdk"'
cargo test --workspace \
  --config "$P.bhmod-core.path=\"../mod-sdk/crates/bhmod-core\"" \
  --config "$P.bhmod-wasm.path=\"../mod-sdk/crates/bhmod-wasm\"" \
  --config "$P.bhmod-manifest.path=\"../mod-sdk/crates/bhmod-manifest\"" \
  --config "$P.bhmod-proto.path=\"../mod-sdk/crates/bhmod-proto\""
git checkout Cargo.lock   # before committing
```

When a mod-sdk change lands, bump the four `rev`s to its commit and run
`cargo update -p bhmod-core -p bhmod-wasm -p bhmod-manifest -p bhmod-proto`.

### The 0xBC mod protocol (server side)

`crates/blockheads_server/src/mods/net.rs` implements the server side of mod-sdk `docs/protocol-0xbc.md`;
`mods/catalog.rs` builds what is offered. Everything is encoded by `bhmod-proto`. The seam adds two calls:
`ModHooks::mod_packet` (the dispatcher routes `0xBC` from approved and waiting peers, only when a mod host is
installed) and the `bhcModLoader` argument of `ModHooks::pre_login` (the 0x1f check, then `player-pre-login`).

- Offered: every enabled package with a client half (its server half, if any, must have loaded), plus loaded
  server-only mods for information. Unpacked folders are packed in memory with the deterministic `.bhmod` writer.
  Never offered: client mods with `native` (logged), packages over 32 MiB or beyond 128 MiB in total.
  `indexListed` is always false (the server does not read the index; clients check it themselves).
- Downloads: 8 chunks in flight per peer, sliding with every `FILE_REQUEST`; at most 8 peers download at once
  (`FILE_ERROR` BUSY); a lower offset restarts there (resume).
- Channels: the server owns the wire ids (assigned on first use, never reused). `MOD_LIST` carries the open channels;
  later ones reach clients in the game as `CHANNEL_REGISTER` (before the first message on it, and on the next poll).
  A client `CHANNEL_REGISTER` is ignored. Server mods sending to a player without the loader are a silent no-op.
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
releases are cut by pushing a signed tag `vX.Y.Z` on `dev`: `.github/workflows/release.yml` builds
every asset into a draft GitHub release, which is published by hand once CI is green.

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
- Android: the APK download opens in the browser; Android asks the user to confirm. This needs every APK signed
  with the same persistent release key (`ANDROID_KEYSTORE_*` secrets).
- Debug builds can point the check at another feed with `BHS_UPDATE_FEED=<url>`.

Release side: `createUpdaterArtifacts` is on, so `tauri build` needs `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (repository secrets; `release.yml` fails without them). The desktop jobs upload
the signed artifacts with their `.sig` files (macOS: `blockheads-server_<version>_<arch>.app.tar.gz`, renamed because
Tauri gives both architectures the same name; spaces in names become dots as on GitHub), and the publish job writes
`latest.json` for all six desktop targets with `.github/scripts/latest-json.sh` before `SHA256SUMS.txt`. Local
bundle builds without the key: `bun tauri build -c '{"bundle":{"createUpdaterArtifacts":false}}'`, or sign with a
throw-away key from `bun tauri signer generate`.
