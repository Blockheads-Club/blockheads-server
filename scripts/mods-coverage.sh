#!/usr/bin/env bash
# 100 % line coverage gate for the mod host seam (crates/blockheads_server/src/mods/). The hook call sites in the
# game code are exercised by the same tests. Needs cargo-llvm-cov and the mod-sdk checkout next to this repository.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo llvm-cov clean --workspace
cargo llvm-cov --no-report -p blockheads_server --lib -- mods
cargo llvm-cov --no-report -p bh_testclient --test mods_server
cargo llvm-cov --no-report -p bh_testclient --test mods_gameplay
cargo llvm-cov report --summary-only | tee /dev/stderr | awk '
  /src\/mods\// { n++; if ($10 != "100.00%") { print "not fully covered: " $1 " " $10; bad = 1 } }
  END { if (n == 0) { print "no mods files in the report"; exit 1 } exit bad }'
