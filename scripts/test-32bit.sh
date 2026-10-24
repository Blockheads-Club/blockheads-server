#!/usr/bin/env bash
# The server crate's tests on 32-bit x86 (CI: test-32bit job; needs the i686-unknown-linux-gnu target and gcc-multilib).
#
# The lib unit tests do not fit in one 32-bit process: they build thousands of worlds, and a world (its macro tile
# array, its leaked physical blocks, its LMDB maps) is never given back, so the 4 GiB address space is gone about
# halfway through the suite ("memory allocation of 65536 bytes failed"). 64-bit has the room. So the lib tests run in
# several processes, CHUNK tests each; everything else (integration tests, doc tests) runs as usual.
set -euo pipefail
cd "$(dirname "$0")/.."

TARGET=${TARGET:-i686-unknown-linux-gnu}
CHUNK=${CHUNK:-250}
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

cargo test -p blockheads_server --lib --target "$TARGET" --no-run
cargo test -p blockheads_server --lib --target "$TARGET" -- --list --format terse 2>/dev/null |
  sed -n 's/: test$//p' | sort >"$TMP/all"
[ -s "$TMP/all" ] || { echo "no lib tests listed"; exit 1; }
split -l "$CHUNK" "$TMP/all" "$TMP/chunk."

status=0
for c in "$TMP"/chunk.*; do
  echo "== lib tests, $(wc -l <"$c") of $(wc -l <"$TMP/all")"
  xargs -a "$c" cargo test -p blockheads_server --lib --target "$TARGET" -- --exact || status=1
done
for t in crates/blockheads_server/tests/*.rs; do
  cargo test -p blockheads_server --target "$TARGET" --test "$(basename "$t" .rs)" || status=1
done
cargo test -p blockheads_server --target "$TARGET" --doc || status=1
exit "$status"
