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
