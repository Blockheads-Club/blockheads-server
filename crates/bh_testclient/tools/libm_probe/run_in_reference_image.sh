#!/bin/sh
# Build and run libm_probe inside the reference userland (Ubuntu 14.04, glibc 2.19-0ubuntu6.15).
# usage: run_in_reference_image.sh <outdir> eval <args-file> | run_in_reference_image.sh <outdir> sweep [seed count]
set -e
HERE=$(cd "$(dirname "$0")" && pwd)
OUT=${1:?outdir}; MODE=${2:-sweep}; IN=${3:-}
# for sweep mode: $3 = seed (optional), $4 = per-category count (optional)
SEED=${3:-}; COUNT=${4:-}
IMG=bh-libm-probe:14.04
mkdir -p "$OUT"
if ! docker image inspect "$IMG" >/dev/null 2>&1; then
  D=$(mktemp -d)
  cat > "$D/Dockerfile" <<'DOCKER'
FROM blockheads-ref:1.7.1
USER root
RUN apt-get update >/dev/null && apt-get install -y --no-install-recommends gcc libc6-dev >/dev/null && rm -rf /var/lib/apt/lists/*
USER bh
ENTRYPOINT []
DOCKER
  docker build -t "$IMG" "$D"
  rm -rf "$D"
fi
cp "$HERE/libm_probe.c" "$OUT/libm_probe.c"
CMD="gcc -std=gnu99 -O0 -fno-builtin -o libm_probe libm_probe.c -lm && getconf GNU_LIBC_VERSION >&2 && (grep -m1 -o -w -E 'fma|avx2' /proc/cpuinfo || true) >&2"
if [ "$MODE" = eval ]; then
  cp "$IN" "$OUT/args.txt"
  docker run --rm --user root -v "$OUT":/w -w /w "$IMG" sh -c "$CMD && ./libm_probe eval < args.txt > out.bin"
else
  docker run --rm --user root -v "$OUT":/w -w /w "$IMG" sh -c "$CMD && ./libm_probe sweep $SEED $COUNT > libm_sweep.bin"
fi
