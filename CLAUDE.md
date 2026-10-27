# CLAUDE.md

Clean-room Rust reimplementation of The Blockheads 1.7.1 dedicated server (GPL-3.0-or-later).

## Rules
- Do not change game behaviour, protocol bytes, save formats, RNG order, or CLI output
  (help/usage must stay byte-identical to the original on Linux).
- Feature `faithful-crashes` is OFF by default and must stay off by default.
- Private material rule: never copy original binaries, decompilation, reference environments,
  or private spec/arch/client docs into this repo, and never reference them by absolute or
  relative path in committed files. Tests needing them must skip cleanly when absent, driven
  by env vars: `BH_PRIVATE_DOCS`, `BH_REFENV`, `BH_REF_BINARY`, `BH_PARITY=1`.
- Commits use Conventional Commits; releases are automatic. Once CI passes on dev, `auto-release.yml` runs
  semantic-release (`.releaserc.json`): fix/perf -> patch, feat -> minor; it bumps the version
  (`scripts/set-version.ts`), commits `chore(release): vX.Y.Z`, tags it and dispatches `release.yml`, which builds,
  attaches and publishes the release. A hand-pushed `vX.Y.Z` tag still builds into a draft release.
- Tooling: bun (never npm) for JS; SvelteKit 2 + Svelte 5 + TS + adapter-static; Tauri 2;
  English UI by default with i18n (en, tr); rustup stable.
- Tests: `cargo test --workspace` (tests run single-threaded via `.cargo/config.toml`).
  Coverage with `cargo llvm-cov`; report honestly, never fake numbers.
