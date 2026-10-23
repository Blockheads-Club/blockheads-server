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

## Status (update 2)
- dynworld: all files 100 % lines (lib + other modules' lib tests; integration not counted).
- plants: vine 3, kelp 5, normal_plant 5, tulip 14, plant.rs 33, simple_plants 3 missed (down from 290/212/113/110/123/11).
  Remaining ones are mostly `}` of unreachable `if let` fall-throughs and unloaded-block edge cases.
- Baseline (before this run, lib + integration, missed lines): load 344, tile_edit 303, vine 291, factories 266,
  lookups 256, lifecycle 255, block_io 233, features 228, kelp 212, tree 190, artificial_light 178, tile_loader 164,
  misc 159, update 157, plant 123, terrain 121, normal_plant 113, world/update 112, tulip 110, net_sync 108,
  persistence 98, torch 90, crafting 84, gem 79, state 76, connections 73, ... total 5691 of 37141.
- NOT started: trees/*, building/*, lighting/*, worldgen/*, world/* (the biggest: tile_edit, lifecycle, misc, update,
  crafting, connections, state, simulation, db_convertor, loot, custom_rules, blocks).
- Terrain helpers: `trees::tree::testenv::env(name)`; tile edits via `tile_at_world_position_loaded(x,y,&w).update(..)`;
  unloaded blocks via `*w.macro_tiles().get(i).unwrap().physical_block.borrow_mut() = None`.
