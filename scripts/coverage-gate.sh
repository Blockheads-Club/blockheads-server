#!/usr/bin/env bash
# Line coverage gate for the workspace (CI: coverage job). Runs the whole test suite once with cargo-llvm-cov, writes
# lcov.info and summary.txt, and fails when a gate below is missed. Needs cargo-llvm-cov.
#
# Policy is 100 % line coverage of the whole workspace, gated per crate (blockheads_server, bh_testclient at 100) and,
# as a second net, file by file for the mod host seam and the 0xBC protocol (crates/blockheads_server/src/mods/) and the
# test client's modded mode (crates/bh_testclient/src/modded.rs). The floors never go down.
# Left out of the measurement (not game code, nothing in them can run without a person or the private reference
# environment): the probe tools (crates/bh_tools), the thin command-line mains of the test client
# (crates/bh_testclient/src/bin) and the *_golden.rs tests, which replay oracle fixtures and skip in CI. The harness
# parts that need docker and the reference server (refenv, save/trace diff, gdb) skip without them.
set -euo pipefail
cd "$(dirname "$0")/.."

FLOOR_blockheads_server=100
FLOOR_bh_testclient=100
IGNORE='crates/bh_tools/|crates/bh_testclient/src/bin/|_golden\.rs'

cargo llvm-cov --workspace --lcov --output-path lcov.info --ignore-filename-regex "$IGNORE"
cargo llvm-cov report --summary-only --ignore-filename-regex "$IGNORE" | tee summary.txt

# Columns of the summary: $1 file, $8 lines, $9 missed lines. Files are printed relative to crates/.
awk -v floor_server="$FLOOR_blockheads_server" -v floor_client="$FLOOR_bh_testclient" '
  $1 ~ /^(blockheads_server|bh_testclient)\/src\// {
    crate = $1; sub(/\/.*/, "", crate)
    lines[crate] += $8; missed[crate] += $9
    if ($1 ~ /src\/mods\/|src\/modded\.rs/) {
      strict++
      if ($9 != 0) { printf "not fully covered: %s (%d lines missed)\n", $1, $9; bad = 1 }
    }
  }
  END {
    if (strict == 0) { print "no mod host files in the report"; exit 1 }
    floors["blockheads_server"] = floor_server; floors["bh_testclient"] = floor_client
    for (c in floors) {
      if (lines[c] == 0) { printf "no %s files in the report\n", c; bad = 1; continue }
      pct = 100 * (lines[c] - missed[c]) / lines[c]
      printf "%-18s %6.2f %% of %d lines (floor %s %%)\n", c, pct, lines[c], floors[c]
      if (pct < floors[c]) { printf "below the floor: %s\n", c; bad = 1 }
    }
    exit bad
  }' summary.txt
