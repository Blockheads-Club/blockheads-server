<p align="center">
  <img width="120" height="120" src="app/static/server-icon.svg" alt="Blockheads Server logo">
</p>

<h1 align="center">Blockheads Server</h1>

<p align="center">
  Host your own <b>Blockheads</b> server for friends — on your PC, a Linux box, Docker or even your phone. 🌍
</p>

<p align="center">
  <img src="https://img.shields.io/maintenance/yes/2026?style=for-the-badge" alt="Maintained">
  <a href="https://github.com/Blockheads-Club/blockheads-server/releases"><img src="https://img.shields.io/github/v/release/Blockheads-Club/blockheads-server?include_prereleases&style=for-the-badge" alt="Release"></a>
  <a href="https://github.com/Blockheads-Club/blockheads-server/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/Blockheads-Club/blockheads-server/ci.yml?branch=dev&style=for-the-badge&label=CI" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/Blockheads-Club/blockheads-server?style=for-the-badge" alt="License"></a>
</p>

---

## 🚀 Project Status
**Current Status:** Alpha (v0.1.1 pre-release). A from-scratch rewrite of the official Blockheads 1.7.1 dedicated
server: same network protocol, same save files, same commands. Existing game clients and worlds work unchanged.

It comes as a command-line server (like the original) and as the **Blockheads Server** app for desktop and Android,
with a world list, start/stop, live log, connected players and a QR code for phones on your Wi-Fi.

---

## 📦 Other Blockheads Club Repositories
- [**blockheads-pc**](https://github.com/Blockheads-Club/blockheads-pc) - Play The Blockheads on your computer
- [**blockheads-patcher**](https://github.com/Blockheads-Club/blockheads-patcher) - Fix crashes and add small improvements to the game on your phone
- [**website**](https://github.com/Blockheads-Club/website) - The [Blockheads Club website](https://blockheads-club.github.io/website/): downloads, guides and docs

---

## ⚡ Trying Blockheads Server
Download from the [latest release](https://github.com/Blockheads-Club/blockheads-server/releases/latest):

- **App (easiest):** Windows `.exe`, macOS `.dmg`, Linux `.AppImage` / `.deb` / `.rpm`, or the Android `.apk`.
- **Command line:** the `.tar.gz` / `.zip` for your system (Linux x86_64, arm64 and armv7 / Raspberry Pi, Windows, macOS).

```bash
./blockheads_server --new MYWORLD --port 15151
./blockheads_server --help      # same options as the original server
```

- **Docker:**

```bash
docker run -i --rm -p 15151:15151/udp -v bh-data:/data ghcr.io/blockheads-club/blockheads-server:0.1.1 --new MYWORLD
```

Players join with Join World -> Advanced... -> Online IP/Port. On Android 10+ the game client needs the popup fix from
the [Blockheads Patcher](https://github.com/Blockheads-Club/blockheads-patcher) before it can connect.
Guides and docs: [blockheads-club.github.io/website/docs](https://blockheads-club.github.io/website/docs/).

---

## 🛠️ Requirements
**To host:** Linux, Windows or macOS (x86_64 or arm64; armv7 on Linux), or an Android phone. Players need their own
copy of the game.

**For contributors / development:** Rust stable ([rustup](https://rustup.rs)). For the app also [bun](https://bun.sh);
for Android the Android SDK, NDK 27 and JDK 17 to 21.

---

## 🤝 Contributing
Everyone can open [issues](https://github.com/Blockheads-Club/blockheads-server/issues) and pull requests. Irrelevant
or abusive issues may be closed or removed. Commits follow [Conventional Commits](https://www.conventionalcommits.org).

### Development Guide
```bash
git clone https://github.com/Blockheads-Club/blockheads-server.git
cd blockheads-server
cargo build --release -p blockheads_server
cargo test --workspace
```

More: [docs/development.md](docs/development.md) (tests, data directory, embedding, the desktop and Android app,
releases).

---

## 📄 License
Blockheads Server is licensed under **GNU GPLv3** (GPL-3.0-only). See the [LICENSE](LICENSE) file for details.
Third-party code ports are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

Unofficial, independent reimplementation for interoperability. It contains no original binaries or assets and is not
affiliated with Majic Jungle or Noodlecake Studios.
