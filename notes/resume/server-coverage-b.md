# server coverage group B resume (branch test/coverage-b)

Status (lib-only run, `~/.cache/bhcov/b/iter.sh "" blockhead/ interaction/ items/ crafting/`): every file of
blockhead/, interaction/, items/ and crafting/ is at 100% lines (0 missed). 1959 lib tests pass.

Helpers: ~/.cache/bhcov/b/{iter.sh,qt.sh,fmt.sh,show.sh,addtests.py,ctx.py}. iter.sh prints the missed ranges per file.
`ctx.py <file> <ctx> <lines...>` shows llvm-cov execution counts around a line (needs `cargo llvm-cov report --text >
~/.cache/bhcov/b/report.txt` first, see the notes on a closing brace line: it is the count of the implicit else).

All tests are server-crate unit tests (single binary). New test files (counted by the gate only if outside `tests`):
- blockhead/update/cov_host.rs   scripted recording `CovHost` (BlockheadHost) with knobs / objects; used by
  update, state_changes tests. Keep every method used: unused methods show up as uncovered lines.
- blockhead/update/coverage_tests.rs, blockhead/interaction_rules/coverage_tests.rs (deterministic fuzz of the rule
  cascade + targeted scenes), blockhead/state_changes/cov_tests.rs, blockhead/ai/cov_tests.rs,
  blockhead/action_queue/cov_tests.rs, blockhead/lifecycle/cov_tests.rs, blockhead/action/cov_tests.rs.
- `AHost` (interaction_rules.rs testhost) gained `misc`, `wb_type`, `can_ride`, `has_ai` knobs.

Production edits (behaviour-neutral, proofs in the commit messages): update.rs (`_unused` removed, dig bonus-particle
count via checked_div), interaction_rules.rs (tree sign guards, head_ok None arm, tree plant ternary, always-true water
test in P3.5), state_changes.rs (`_unused` removed, can_collapse ride-animation test), lifecycle.rs (actions array
built directly), ai.rs (mining `else if let`).

Open / ideas: none for group B lines. Not run: bh_testclient integration suites (the production edits are
behaviour-neutral); run them with `-- --test-threads=1` if you want a belt-and-braces check.
