# Scenario fixtures

Reference observations for the gameplay scenarios in `src/gameplay/scenarios.rs`. The docker-free tests in
`tests/scenarios_ours.rs` use them: our server runs in-process and must reproduce every stable value.

## Files

- `<scenario>.txt` is the noise mask learned from several runs of the original 1.7.1 server, all from the same
  seeded world. Lines starting with `#` are comments.
  - `S<TAB>key<TAB>value`: the value was identical in every reference run. It is a hard check. Keys starting
    with `~` are compared but only reported, because they count wall-clock cadence packets.
  - `V<TAB>key`: the value differed between reference runs, or the key was missing from some of them. It is never
    checked.
  - Any other key that our server reports is a difference, because no reference run produced it.
- `<scenario>.xfail` lists known differences that are not fixed yet, as `key  # reason`. The list is strict:
  - an entry that still differs is tolerated;
  - an entry that now matches makes the test fail until you remove the line.

## Regenerating

The fixtures hold observed values only: packet counts, inventories, flags and texts. They never contain material
from the original binary.

Regenerate them after changing a scenario script. This needs docker, the `blockheads-ref:1.7.1` image and the
reference environment:

```sh
cargo build --release -p blockheads_server
BH_PARITY=1 BH_REFENV=<refenv dir> BH_SERVER_BIN=target/release/blockheads_server \
BH_SCENARIO_BLESS=1 BH_DIFF_REF_RUNS=3 BH_DIFF_OUT=/some/dir \
  cargo test -p bh_testclient --test diff_gameplay -- --nocapture
```

- `BH_SCENARIO_BLESS=1` rewrites `<scenario>.txt` from the reference runs, whatever our server does.
- Use at least 3 reference runs: with fewer, a timing-dependent value that happens to repeat becomes a hard check.
- Review the diff, then run `cargo test -p bh_testclient --test scenarios_ours` and update the `.xfail` files.
- To bless a single scenario, append its test name, for example `diff_commands`.

## Nightly differential workflow

`.github/workflows/parity.yml` runs the differential suite nightly and on manual dispatch, but only when the
repository variable `REFENV_REPO` and the secret `REFENV_TOKEN` point at a private copy of the reference
environment. Neither is configured by default, so the `differential` job is skipped and the run succeeds with a
notice. Normal CI does not depend on it.
