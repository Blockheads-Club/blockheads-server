#!/usr/bin/env bash
# Writes <dist>/latest.json for the Tauri updater plugin of the host app from the signed updater artifacts in <dist>
# (each next to its .sig, as release.yml collects them). All six desktop targets must be present.
# Usage: latest-json.sh <dist> <version> <tag> <owner/repo>
set -euo pipefail
dist="$1" version="$2" tag="$3" repo="$4"
base="https://github.com/$repo/releases/download/$tag"
platforms='{}'
add() { # <platform key> <asset name suffix>
  local f
  f="$(find "$dist" -maxdepth 1 -type f -name "*$2" | head -n1)"
  if [ -z "$f" ] || [ ! -f "$f.sig" ]; then
    echo "::error::no signed updater artifact for $1 (*$2 and its .sig)" >&2
    exit 1
  fi
  local name
  name="$(basename "$f")"
  platforms="$(jq --arg k "$1" --arg sig "$(cat "$f.sig")" --arg url "$base/$name" '. + {($k): {signature: $sig, url: $url}}' <<<"$platforms")"
}
add windows-x86_64 _x64-setup.exe
add windows-aarch64 _arm64-setup.exe
add darwin-aarch64 _aarch64.app.tar.gz
add darwin-x86_64 _x64.app.tar.gz
add linux-x86_64 _amd64.AppImage
add linux-aarch64 _aarch64.AppImage
jq -n --arg v "$version" --arg notes "https://github.com/$repo/releases/tag/$tag" --arg date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --argjson p "$platforms" \
  '{version: $v, notes: $notes, pub_date: $date, platforms: $p}' > "$dist/latest.json"
echo "wrote $dist/latest.json"
