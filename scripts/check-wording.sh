#!/usr/bin/env bash
# check-wording.sh — one word for one thing. satz's glossary (vendor/satz/docs/glossary.md) says which word names what;
# scripts/check-wording.txt lists the wordings that were used for a second thing, each
# with what to say. This reports every line of the tracked text that uses one.
#
#   scripts/check-wording.sh            # whole tree (CI)
#   scripts/check-wording.sh --staged   # staged files (pre-commit hook)
#   scripts/check-wording.sh FILE...    # specific files
#
# Read: Markdown, Satz, Rust, shell and Python sources. Not read: the decision records
# before 0093 and the changelogs, which keep the words of their day; the glossary and the
# word list, which have to name the words; generated and third-party trees.
#
# bash 3.2 compatible (macOS default).
set -uo pipefail
cd "$(git rev-parse --show-toplevel)"
list=scripts/check-wording.txt
case "${1:-}" in
  --staged) shift; files=$(git diff --cached --name-only --diff-filter=ACMR) ;;
  "") files=$(git ls-files) ;;
  *) files=$(printf '%s\n' "$@") ;;
esac
files=$(printf '%s\n' "$files" | grep -E '\.(md|satz|rs|sh|py|toml)$' \
  | grep -Ev '^(docs/adr/00([01][0-9]|2[0-8])-|vendor/|target/|.*CHANGELOG\.md$|docs/glossary\.md$|scripts/check-wording\.(sh|txt)$|THIRD-PARTY)' || true)
[ -n "$files" ] || { echo "check-wording: OK (0 files)"; exit 0; }
bad=0
while IFS=$'\t' read -r pattern say; do
  case "$pattern" in ''|'#'*) continue ;; esac
  hits=$(printf '%s\n' "$files" | tr '\n' '\0' | xargs -0 grep -nEiH -- "$pattern" 2>/dev/null || true)
  [ -n "$hits" ] || continue
  bad=1
  echo "✗ say: $say"
  printf '%s\n' "$hits" | cut -c1-200 | sed 's/^/    /'
done < "$list"
n=$(printf '%s\n' "$files" | wc -l | tr -d ' ')
if [ "$bad" -eq 0 ]; then echo "check-wording: OK ($n files)"; else echo "check-wording: a wording that names a second thing — vendor/satz/docs/glossary.md says which word to use"; exit 1; fi
