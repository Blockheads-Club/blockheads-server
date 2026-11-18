# Server mods

How the server runs mods. Making mods: [mod SDK](https://github.com/Blockheads-Club/mod-sdk). For server admins: [blockheads.club/docs/guides/admins-mods](https://blockheads.club/docs/guides/admins-mods).

Server-side Lua (Luau) and WASM (Rust, C, Zig, Go) mods from the [mod SDK](https://github.com/Blockheads-Club/mod-sdk): put `.bhmod` files or
unpacked mod folders in `<data dir>/mods/` (or the folder named by `BLOCKHEADS_MODS_DIR`) and grant what they ask
for in `mods.toml` next to them:

```toml
[hello-lua]
grant = ["players:chat"]
```

Mods get join/quit/chat/command/tick/world events, gameplay events (pre-login, block place/break/change, object
placement, damage, death, movement; placing, breaking and damage can be cancelled), their own `/commands` (for every
player), a scheduler, per-world storage (`saves/<world>/mods/storage`, separate from the world database) and
services, custom events and channels to work with each other (the SDK's `economy` and `landclaim` samples do). They run with a time budget and
are disabled after repeated faults. Mods that declare settings get a form on their page in the host app and the
admin commands `/mods settings <mod>` and `/mods set <mod> <key> <value>` (values live under `[<mod>.settings]` in
`mods.toml` and apply while the server runs; `/mod` stays the vanilla modlist command); admins with a modded client
also get them in the loader's settings screen. A mod with the `voice` permission (the official voice chat mod) turns
on voice chat for modded players: encrypted voice frames relayed over a dedicated UDP voice port (the mod's
`udp_port` setting, else `HostConfig.voice.udp_port`, else the game port + 1, so 15152 by default; open it next to
the game port). If that port cannot be opened the server logs it and voice chat stays off. The server never decodes
audio. `client-required =
false` under a mod lets players without that client mod join. Without a mods folder nothing changes: same bytes, saves and output as
vanilla. Vanilla game clients can join servers with server-side mods.

Mods with a client half are offered to modded clients (PC loader, Android loader installed with Patcher) over the
[0xBC mod protocol](https://github.com/Blockheads-Club/mod-sdk/blob/dev/docs/protocol-0xbc.md): the client lists the
server's mods, downloads the packages from the server (resumable, SHA-256 checked) and joins; server and client mods
talk over channels and, since API 0.3.1, service calls: a client mod can call a service a server mod provides (and the
other way round), asynchronously over `SERVICE_CALL` / `SERVICE_RESULT`. Only services the provider marks `remote = true` in its
`mod.toml` are reachable, every player has a rate limit (bursts of 20 calls, 10 per second; `BLOCKHEADS_MODS_REMOTE_BURST` and
`BLOCKHEADS_MODS_REMOTE_RATE` change it) and the provider runs under its own time budget. Vanilla clients never receive a protocol byte; when a client mod is required they are not
let in: they get the world summary with a welcome text ("This server uses Blockheads Club mods. Install the mod loader
with Patcher ...", shown by the game as its welcome popup) and are dropped 20 seconds later (or with their next packet)
by a plain disconnect (no rejection text, so the popup stays the only message). A client mod that needs `native` is listed (with its SHA-256, so a server can require it) but its bytes are never sent: the player installs it from the public index and confirms it.

**JVM mods** (Java, Kotlin, ...) are unsandboxed code, so they are off twice over: build with `--features jvm` (needs a
JDK for the Java SDK that is compiled in; the release CLI archives and desktop app bundles already are, the Android APK and
the docker image are not: Android has no JRE to load, and a JRE would make the distroless image several times larger) and start the server with `BLOCKHEADS_JVM=1`, which loads the machine's JRE
(8 or newer; `BLOCKHEADS_JAVA_HOME` picks one, `BLOCKHEADS_JVM_HEAP_MB` sets the heap, default 256). The mod then needs
`native` in its `mods.toml` grant. The desktop/Android host app has the same switch in **Settings > Java mods** (off by default, with a red warning;
it sets `BLOCKHEADS_JVM` / `BLOCKHEADS_JAVA_HOME` for the server it starts) and only works in an app built with
`--features jvm` (`app/src-tauri`); other builds say so under the switch. How to write one: [mod-sdk `docs/jvm.md`](https://github.com/Blockheads-Club/mod-sdk/blob/dev/docs/jvm.md).

