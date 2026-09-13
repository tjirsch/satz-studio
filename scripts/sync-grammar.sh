#!/usr/bin/env bash
# sync-grammar.sh — refreshes the vendored tree-sitter grammar, vendor/satz-tree-sitter/,
# from a checkout of the satz-tree-sitter repository (private, hence vendored): src/
# whole (parser.c, grammar.json, node-types.json, tree_sitter/*.h), LICENSE, and
# COMMIT — the checkout's HEAD, so the copy names what it was taken from.
#
#   scripts/sync-grammar.sh <path-to-satz-tree-sitter-checkout>
#
# Refused: a checkout with uncommitted changes (COMMIT would name a state that was
# not what got copied), and one without src/parser.c (run `tree-sitter generate`
# there and commit the result first).
set -euo pipefail

die() { echo "sync-grammar: $*" >&2; exit 1; }
[[ $# -eq 1 ]] || { echo "usage: scripts/sync-grammar.sh <path-to-satz-tree-sitter-checkout>" >&2; exit 1; }

[[ -d "$1" ]] || die "no such directory: $1"
src="$(cd "$1" && pwd -P)"
top=$(git -C "$src" rev-parse --show-toplevel 2>/dev/null) || die "$src is not a git checkout"
[[ "$top" == "$src" ]] || die "$src is inside the checkout $top; pass the checkout itself"
dirty=$(git -C "$src" status --porcelain)
[[ -z "$dirty" ]] || { printf '%s\n' "$dirty" >&2; die "$src has uncommitted changes; commit or stash them first"; }
[[ -f "$src/src/parser.c" ]] || die "$src/src/parser.c is missing — run \`tree-sitter generate\` there and commit"
[[ -f "$src/LICENSE" ]] || die "$src/LICENSE is missing"

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
dest="$repo_root/vendor/satz-tree-sitter"
[[ -d "$dest" ]] || die "$dest is missing — not a satz-studio checkout?"
old=$(cat "$dest/COMMIT" 2>/dev/null || echo "(none)")
new=$(git -C "$src" rev-parse HEAD)

rm -rf "$dest/src"
cp -R "$src/src" "$dest/src"
cp "$src/LICENSE" "$dest/LICENSE"
printf '%s\n' "$new" > "$dest/COMMIT"
echo "sync-grammar: vendor/satz-tree-sitter $old -> $new"
