# server coverage group C resume
Status: every file of npc/, path/, trains/, physics_objects/, objects/ reports 100.00% LINES in the lib-only
`cargo llvm-cov report` summary of `cargo llvm-cov --lib -p blockheads_server` (checked 2026-10-24).
Method notes:
- The summary scores each function by its best single instantiation, so a per-lcov check is not enough: trait
  default methods need ONE implementor that takes every branch (see BareNpc.ridable / BareCar.engine cells).
- Find leftovers with `cargo llvm-cov report --json` (functions with count 0 across all instantiations) and
  `report --text --show-instantiations`.
- Helper scripts: ~/.cache/bhcov/c-iter.sh, c-txt.sh, c-commit.sh; full pass: /tmp/.../fullc.sh equivalent
  (clean profraw, `cargo llvm-cov --no-report --lib -p blockheads_server`, `report`).
- Seeded simulation tests: clown_fish `sim_tests`, drop_bear `seeded_forests...`, scorpion `seeded_dunes...`,
  cave_troll `sim_tests::seeded_caverns...`; hostile test host (cave_troll `test_support`) has paused/regen/trade/
  nosim/cactus/no_world/client switches.
Dead code removed (each with proof in its commit message): clown_fish choose_next_tile y!=pos.y branch, drop_bear
night-check tile0 None, scorpion wander is_net wrapper, cave_troll opt_same / FALL speed clamp, steam_train and
train_car replica link guard.
Left: nothing for group C lines. Still to do by the owner: confirm the bh_testclient scenario suites
(`cargo test -p bh_testclient -- --test-threads=1`) after the freight_car chest-factory fix.
