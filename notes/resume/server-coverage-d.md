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
- Round-1 agent, after its first report: trees/* scenario tests (every species through growth, fruit, save/reload, old
  age, dead-tile removal, pine food, unloaded ground; commits e2a6793 + 1830812). 1830812 also swept in round-2's
  then-uncommitted world/* edits by mistake. Its unverified building/column.rs scenario test was discarded; it then stopped.

## Status (update 3, round 2 agent)
- Lib-only measurement: `~/.cache/bhcov/iterl.sh "<filter>" <file suffix>` runs the filtered lib tests with its own
  llvm-cov dir (`llvm-cov-target-dw`) and prints that file's missed lines; `showt.py <suffix> [ctx]` prints them with
  source; `repl.py` reports all group D files from a full lib run (`liball-d.lcov`, 4 minutes). Use filter `world::`
  for world/* (every test of the module contributes).
- world/*: 100 % lines (lib only) except lifecycle.rs 2 lines (`initialize_databases`: the `blocks` database failing to
  open while `main` opened; needs an LMDB failure, not reachable).
- Test doubles added: misc.rs `testkit::Rec` (net_bh, controlled_bh, paintable, bh_by_id, wb/wb_level/wb_type/wb_pos,
  cannot_remove, goals, busy, path_users, meditating, light blocks), lifecycle.rs `testkit::Rec`/`Bh` (path creator,
  tips, asleep/regenerating/... flags), `testkit::APP_ENV` (app database environment of the test delegate),
  helpers.rs `testutil::dw_obj_of_type`.
- Hook trait defaults (`WorldCoreHooksStub`, `WorldOpsDepsStub`) are covered by generated calls through
  `NullCoreHooks`/`NullDeps` (generator: `~/.cache/bhcov/genhooks.py`).
- Dead code removed (proofs in the commit messages): tile_edit.rs fill arms for items that `item_type_is_valid_fill_item`
  excludes, 3|5 back wall fix-ups, station neighbour `if let`; growth.rs duplicate guard; heartbeat.rs paused loop;
  db_convertor.rs three impossible fallbacks; custom_rules.rs None arm; state.rs trim_free_physical_blocks `if let`.
- Careful: tests that write save files must set a private HOME (`set_home_for_test` under `HOME_LOCK`), never the real one.
- Next: building/*, lighting/*, worldgen/*, trees/* rest, plants rest.

## Status (update 4, round 2 agent, end of run)
- Full lib-only run (`~/.cache/bhcov/fulll.sh`, ~3 min incl. build): group D missed lines 3759 -> about 340 of 45 400.
  Left (lib only): block_io 79, tree.rs 54, gem 47, plant.rs 31, pine 24, cactus 20, tulip 14, terrain 12, coconut 10,
  apple 9, maple 7, features 7, light_archive 6, kelp 5, normal_plant 5, tile_loader 4, vine 3, bed 3 (a real
  Blockhead in `dw.blockheads()`), simple_plants 3, lifecycle 2 (LMDB open failure of `blocks`), artificial_light 2,
  sunlight 2 (the `minAge > 1.5 && peers != 0` early return is only reachable if the clock moves 1.5 s between two
  consecutive reads), one line each in the fruit species.
- building/*, lighting/*, world/* done (see commit log); worldgen/* mostly done through whole-world generation over
  many rule sets (`worldgen::tile_loader::sweep_tests`) plus legacy block files and a self-made reference world.
- trees/* and plants/* rest: extend `trees::tree::fuzz_tests` (random disturbances per species) and write per-species
  scenarios for gem (branches, gem drops), pine/cactus (animals), plant.rs/tulip.
- Dead code removed this round (proofs in commit messages): see update 3 plus: base-init failure arms of the
  at-position / save-dict initialisers (building, trees, plants, lighting), `DynamicWorld::is_client()` branches
  (constant false), bed.rs interaction-object init arms, tile_loader.rs `hasRefinedTerrain` write.
- Helper scripts added: `iterl.sh <filter> <file suffix>` (lib-only, own llvm-cov dir `llvm-cov-target-dw`),
  `showt.py <suffix> [ctx]`, `repl.py` / `fulll.sh` (full lib report), `genhooks.py` (generates calls of trait
  defaults).

## Status (update 5, round 3 agent): group D at 0 missed lines (lib only)
- Full lib-only run (`~/.cache/bhcov/fulll.sh`, now with `HOME=~/.cache/bhcov/d-home`, `CARGO_HOME`/`RUSTUP_HOME` pinned to
  the real ones): **0 of 46 481 lines missed** in world, dynworld, worldgen, trees, building, plants, lighting
  (353 before this round). `iterl.sh` / `fulll.sh` also run under the private HOME now.
- Test aids added: `trees::tree::testenv::{scatter, wipe, gauntlet}` (obstacle fields, partly unloaded worlds, odd
  heights per species), `trees::tree::edge_tests` (broken / shaded / unloaded tall trees, odd save dicts, leaf takeover,
  apple rescue), `plants::plant::scenario_tests::{plants_over_odd_places_and_unloaded_blocks,
  normal_plants_beside_unloaded_rows}`, `worldgen::tile_loader::sweep_tests::GenOps` (shared block-loading double),
  `FakeHost::clobber` (throws a block's client light blocks away inside `fully_load_if_needed_around_pos`),
  `lighting::artificial_light::testkit::Rec::peers_script` (peer count scripted per call).
- Seams for the three "unreachable" spots of round 2: lifecycle (a plain record named `blocks` in the unnamed LMDB
  database makes `Database::new(env, "blocks")` fail while `main` opens: `a_blocked_block_database_name_...`), sunlight
  (`peers_script = [0, 1]`: the peer count changes between the two looks instead of a 1.5 s clock stall), bed (a real
  `Blockhead` from `blockhead::blockhead::testutil` pushed into `dw.blockheads`).
- Dead code removed this round (proofs in the commit messages): the `return None` arm of every species'/plant's
  `from_save_dict` (the save-dict initialisers answered `true` on every path; they return `()` now), the
  `if let Some(tile)` around the own tile in the tulip/kelp/vine at-position initialisers (`plant_init_at_position`
  only succeeds on a loaded tile), `simple_plants::never` + its dummy const, block_io `DataSaveException` arm and
  the `light_blob`/`server_block` None arms (`write_xml` cannot raise), terrain `t <= 0.0f` clamp, features
  `island != 0` wrapper, light_archive `cfg!(faithful-crashes)` branch (now a `#[cfg]` statement).
- Careful: tests that call `WorldTileLoader::init...` on a world with a server or with a save id write below
  `paths::appsup()`; run them only with the private HOME.
