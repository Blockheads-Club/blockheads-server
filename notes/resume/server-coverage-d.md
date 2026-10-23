# server coverage, group D (world, dynworld, worldgen, trees, building, plants, lighting)

Branch `test/coverage-d` (worktree `server-wt/coverage-d`), based on origin/dev 7f06e8d.

## Method
- Shared target dir with an own llvm-cov dir: `CARGO_LLVM_COV_TARGET_DIR=~/.cache/bhcov/target/llvm-cov-target-d`
  (other agents delete binaries in the shared llvm-cov-target; never run `cargo llvm-cov clean --workspace`).
- Scripts in `~/.cache/bhcov/` (not in the repo): `envd.sh`, `others-d.sh` (lib tests of everything except my
  modules -> `others-d.lcov`, rerun after production-line edits in a file), `iterd.sh <filter>` (my lib tests ->
  `lcov-d.info`), `int-d.sh` (integration tests, 15+ min -> `int-d.lcov`, valid per file only if the file was not
  edited since `int-d.start`), `rep-d.py` (missed lines per file, merged), `showi.py <file suffix>` (missed lines
  with source).
- Per-file loop: read the code of the misses, write scenario tests (new `mod scenario_tests` at the end of the file),
  run `./others-d.sh` when production lines moved, then `./iterd.sh <module>:: <module>/`.
- Line coverage quirks: a `}` that closes an `if let` whose else branch cannot happen shows as missed; such
  branches were turned into `expect`/single-line `let ... else { unreachable!() }` (behaviour unchanged).
  Hooks traits (`DwHooks`, `DwOpsHooks`) default methods are covered by calling `NullHooks`/`NullOpsHooks`.
- Test doubles shared by dynworld tests: `dynworld::state::testutil` (Sc, Srv, CountingOps, Hk, TObj/tobj, ModSpy,
  dw_served, plist_dict). Terrain env: `trees::tree::testenv::env(name)` (2 macro wide, dirt to y=9, air above).

## Status
- dynworld: all files 100 % lines (lib + other modules' lib tests; integration not counted).
- Remaining (lib-only numbers, missed lines): see the final report of the run / rerun `python3 ~/.cache/bhcov/rep-d.py`.
