# Server coverage resume notes (agent A scope: types, app, server, plist, foundation, fmath, env, net, protocol, storage, rng, hooks, host, mods, bh_testclient)

## Done (branch test/coverage)
- types/*: 100 % lines (enum_ext sweep over every generated enum wrapper, structs/tile helpers).
- plist/xml_read.rs, binary.rs, json.rs: 100 % lines. Fixture replays moved to `*_golden.rs` (xml_read_golden.rs, binary_golden.rs,
  json_golden.rs), pulled in with `#[cfg(test)] #[path = "..."] mod golden;`.
- Still to do in plist: value.rs, xml_write.rs, keyed_archiver.rs, helpers.rs (a few lines).

## Convention proposed for the gate (coordinator owns scripts/coverage-gate.sh)
Add `|_golden\.rs` to IGNORE: golden replays need the private fixtures and skip in CI, so their code can never count.
Rule for new tests: fixture-dependent tests go to `<module>_golden.rs`; everything else must run without fixtures.
Closing-brace artefact: `if let ... { } ` without else, and `match` arms that panic in test helpers, show as uncovered `}` lines. Use
`let ... else { panic!() }` on one line in helpers and exercise the else branch of production `if let`s.

## Tooling (in ~/.cache/bhcov, shared target ~/.cache/bhcov/target)
- cov.sh "<cargo llvm-cov args>" "<test filter>" <file substrings>: runs the tests, merges only this worktree's profraw, prints
  per-file missed lines (lib-only runs overstate misses: integration tests cover more).
- base.lcov: full workspace baseline from before the plist work (per-file miss lists for untouched files).
- Baseline misses of the remaining scope (lines): app/integration 690, bh_testclient save_diff 472, normalize 286, scenario 255,
  server/bhserver 284, dispatch 282, plist/keyed_archiver 209, app/delegate 200, getopt 160, game_controller 154, protocol/messages 143,
  net/bhmatch 140, refenv 139, server/commands 138, decode 129, plist/value 116, session 107, app/cli 100, env/snow_surface_block 93,
  harness/mod 82, fmath/libm 76, storage/lmdb 57, env/surface_block 52, plist/xml_write 50, ...
## Open question
- scripts/mods-coverage.sh on test/limbo-no-match (cb1f50c) still reports server_host.rs at 99.83 % (1 line); the drop_limbo None path
  is covered by the new test. The remaining missed line could not be located with llvm-cov text/json output.
