# blockheads-server

A clean-room Rust reimplementation of **The Blockheads 1.7.1 dedicated server**.
Licensed under GPL-3.0-only.

It speaks the same wire protocol, reads and writes the same save format, and
prints the same command-line help as the original server, so existing clients
and worlds work unchanged.

## Platforms

| OS      | x86_64 | aarch64 | armv7 |
|---------|:------:|:-------:|:-----:|
| Linux   | yes    | yes     | yes (Raspberry Pi) |
| Windows | yes    | yes     | -     |
| macOS   | yes    | yes     | -     |
| Android | -      | yes (host app) | - |

An Android host app (and the same app as a desktop GUI) is built from this
repository as well; see the project releases.

## Interoperability and legal notice

This is an independent reimplementation written for interoperability: it speaks the same wire protocol, reads and
writes the same save format and prints the same command-line help as the original dedicated server. It contains
**no original binaries or assets**. Compatibility tables that are required for protocol and save compatibility
(item ids, recipes, wire and save layouts, loot and price tables, name lists, CLI option table and help text) are
included as data. Parts of the numeric and text layers are ports of third-party code (glibc, fdlibm, GNUstep-base);
see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

This project is **not affiliated with, endorsed by, or connected to Majic Jungle or Noodlecake Studios**.
"The Blockheads" is a trademark of its respective owners. You need your own legitimate copy of the game client to play.

## Quick start

Build from source (Rust stable, via [rustup](https://rustup.rs)):

```sh
cargo build --release -p blockheads_server
```

**Linux / macOS**

```sh
./target/release/blockheads_server --help
./target/release/blockheads_server --new world1 --port 15151
```

**Windows (PowerShell)**

```powershell
.\target\release\blockheads_server.exe --help
.\target\release\blockheads_server.exe --new world1 --port 15151
```

Prebuilt per-target archives, the Android APK, desktop bundles and a multi-arch
Docker image (`ghcr.io/blockheads-club/blockheads-server`) are attached to each
release. Run `--help` for the exact options; its output matches the original.

### Android clients

Modern Android builds of the game client need the popup fix from the
[Blockheads Patcher](https://github.com/Blockheads-Club/blockheads-patcher)
before they can connect.

### Data directory

Worlds live in `<home>/GNUstep/Library/ApplicationSupport/TheBlockheads` on every OS (`<home>` is `$HOME`, or
`%USERPROFILE%` on Windows), exactly like the original. Set `BLOCKHEADS_DATA_DIR` to use another directory as that
`TheBlockheads` folder (the Docker image uses `/data`). There is no CLI flag for it, so `--help` stays identical to
the original.

### Docker

```sh
docker run -i --rm -p 15151:15151/udp -v bh-data:/data ghcr.io/blockheads-club/blockheads-server:alpha --new MYWORLD
```

Keep `-i` so the console (`/stop`, chat) can be used.

### Embedding

`blockheads_server::host` runs the server inside another program (used by the desktop and Android host app):
`start(config, log_callback)` returns a handle with `status()`, `send_command()`, `stop()`, and
`list_worlds` / `create_world` / `delete_world` manage worlds. A server can be started again after it stopped,
in the same process.

## Development

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

Coverage: `cargo llvm-cov --workspace`.

Commits follow [Conventional Commits](https://www.conventionalcommits.org);
releases are cut by pushing a signed tag `vX.Y.Z` on `dev`: `.github/workflows/release.yml` builds
every asset into a draft GitHub release, which is published by hand once CI is green.

The `faithful-crashes` cargo feature (emulating original crash behaviour) is
off by default.

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
