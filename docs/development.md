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

Coverage: `cargo llvm-cov --workspace`.

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
Only the app is updated; the game server code is not involved.

- Feed: `https://api.github.com/repos/Blockheads-Club/blockheads-server/releases`. Drafts are ignored. The stable
  channel takes plain `vX.Y.Z` tags that are not marked as prerelease; the "Stable and test versions" setting also
  takes prereleases and `vX.Y.Z-pre.N` tags. Automatic checks run at start and at most once a day.
- Assets are matched by the Tauri bundle names that `release.yml` uploads: `*_x64-setup.exe` / `*_arm64-setup.exe`
  (Windows), `*_aarch64.dmg` / `*_x64.dmg` (macOS), `*_amd64.AppImage` / `*_aarch64.AppImage`, `*_amd64.deb` /
  `*_arm64.deb`, `*.x86_64.rpm` (Linux) and `*-android.apk`. Renaming bundles in the workflow breaks this matching.
- Desktop: the asset is downloaded into `<app data>/updates`, checked against the release's `SHA256SUMS.txt` (a
  release without it, or without a matching asset, falls back to opening the download page), then the restart
  saves and stops the server, copies the loaded world to `<data dir>/update-backups/<world>-<ms>` (newest three
  kept), and hands over: Windows runs the NSIS setup, macOS opens the disk image, Linux opens the .deb/.rpm with the
  system installer or, inside an AppImage (`$APPIMAGE`), swaps the AppImage file and starts it. A note in
  `updates/pending.json` tells the next start which version was expected and whether to start the server again.
- Android: the APK download opens in the browser; Android asks the user to confirm the install. This needs the APK
  to be signed with the same persistent release key every time (see the `ANDROID_KEYSTORE_*` secrets).
- Debug builds can point the check at another feed with `BHS_UPDATE_FEED=<url>`.

Fully in-place updates (download, install and relaunch without an installer window) would use the Tauri updater
plugin, which needs a signing key that this repository does not have yet. To enable it: generate a key pair with
`bun tauri signer generate -w ~/.tauri/blockheads-server.key`, keep the private key and its password only as the
repository secrets `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, put the public key in
`tauri.conf.json` under `plugins.updater.pubkey` with an endpoint for a `latest.json`, set
`bundle.createUpdaterArtifacts: true`, and have `release.yml` upload the `.sig` files and `latest.json`. The
installer hand-off above can then be replaced by `tauri-plugin-updater` on desktop.
