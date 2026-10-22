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

Coverage: `cargo llvm-cov --workspace`. The mod host seam has its own 100 % line gate: `scripts/mods-coverage.sh`.

### Developing against a local mod-sdk

`bhmod-core` and `bhmod-wasm` come from `https://github.com/Blockheads-Club/mod-sdk` at the `rev` pinned in the
workspace `Cargo.toml`. To build against a local checkout instead (for example `../mod-sdk` next to this repository),
patch the git source on the command line; nothing in the repository changes except `Cargo.lock`, which you must not
commit in that state:

```sh
P='patch."https://github.com/Blockheads-Club/mod-sdk"'
cargo test --workspace \
  --config "$P.bhmod-core.path=\"../mod-sdk/crates/bhmod-core\"" \
  --config "$P.bhmod-wasm.path=\"../mod-sdk/crates/bhmod-wasm\""
git checkout Cargo.lock   # before committing
```

When a mod-sdk change lands, bump both `rev`s to its commit and run `cargo update -p bhmod-core -p bhmod-wasm`.

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
