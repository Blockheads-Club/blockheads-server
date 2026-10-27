// Sets the release version in every file that carries it: `bun scripts/set-version.ts 0.5.0`.
// semantic-release runs it in its prepare step (.releaserc.json); the files it touches are the `assets` of
// @semantic-release/git there. Every pattern must match, so a moved version string fails the release instead of
// shipping a half-bumped tree.
import { readFileSync, writeFileSync } from 'node:fs';

const next = process.argv[2] ?? '';
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(next)) {
  console.error('usage: bun scripts/set-version.ts X.Y.Z[-pre.N]');
  process.exit(1);
}
const current: string = JSON.parse(readFileSync('app/package.json', 'utf8')).version;
const v = current.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const nl = '\\r?\\n';

// Each pattern captures the text before and after the current version. `g` patterns replace every match.
const edits: [string, RegExp][] = [
  ['README.md', new RegExp(`(\\(v)${v}( pre-release\\))`)],
  ['README.md', new RegExp(`(ghcr\\.io/blockheads-club/blockheads-server:)${v}( )`, 'g')],
  ['Cargo.toml', new RegExp(`(\\[workspace\\.package\\]${nl}version = ")${v}(")`)],
  ['app/package.json', new RegExp(`("version": ")${v}(")`)],
  ['app/src-tauri/Cargo.toml', new RegExp(`(${nl}version = ")${v}(")`)],
  ['app/src-tauri/tauri.conf.json', new RegExp(`("version": ")${v}(")`)],
  // Local crates are the ones without a `source` line (registry and git crates always have one).
  ['Cargo.lock', new RegExp(`(name = "[^"]+"${nl}version = ")${v}("${nl}(?!source))`, 'g')],
  ['app/src-tauri/Cargo.lock', new RegExp(`(name = "[^"]+"${nl}version = ")${v}("${nl}(?!source))`, 'g')],
];

for (const [file, re] of edits) {
  const text = readFileSync(file, 'utf8');
  if (!re.test(text)) {
    console.error(`${file}: ${current} not found (${re})`);
    process.exit(1);
  }
  writeFileSync(file, text.replace(re, (_, before: string, after: string) => before + next + after));
}
console.log(`version ${current} -> ${next}`);
