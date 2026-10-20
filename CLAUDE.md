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
- Commits use Conventional Commits. Releases: push a signed `vX.Y.Z` tag; `release.yml` builds a draft
  GitHub release (prerelease for 0.x), published by hand once CI is green.
- Tooling: bun (never npm) for JS; SvelteKit 2 + Svelte 5 + TS + adapter-static; Tauri 2;
  English UI by default with i18n (en, tr); rustup stable.
- Tests: `cargo test --workspace` (tests run single-threaded via `.cargo/config.toml`).
  Coverage with `cargo llvm-cov`; report honestly, never fake numbers.
