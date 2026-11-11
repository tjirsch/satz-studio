#!/usr/bin/env bash
# check-newest-satz.sh — the files of the NEWEST satz release through this app's grammar
# and document layer, before the pin moves to it.
#
# The tests read vendor/satz, which is the pinned satz: a form satz adds to the language
# shows there only when the pin moves, and until then the app fails on an estate that
# uses it while every test is green. This checks out the installed satz's release in
# vendor/satz — the work tree only, restored on exit — and runs the three tests that
# hold satz's own files to the grammar and the document layer:
#
#   tests/grammar_pin.rs     the vendored grammar is the commit that satz names
#   tests/cst_roundtrip.rs   every .satz of satz parses to a tree that is the file, and
#                            the tree and satz-core's AST agree on its entries
#   tests/cst_tree.rs        the node kinds
#
#   scripts/check-newest-satz.sh
#
# The daily run of .github/workflows/ci.yml calls it; a failure opens the issue that says
# the app is behind the language. `scripts/move-pin.sh <tag>` is the first step of the
# answer.
set -euo pipefail

die() { echo "check-newest-satz: $*" >&2; exit 1; }
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

satz="$(command -v satz || true)"
[[ -n "$satz" ]] || satz="$HOME/.local/bin/satz"
[[ -x "$satz" ]] || die "no satz on PATH or in ~/.local/bin"
version="$("$satz" --version 2>/dev/null | tail -1 | awk '{print $2}')"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "could not read a version from \`$satz --version\`"
tag="v$version"

pinned="$(git -C vendor/satz rev-parse HEAD)"
git -C vendor/satz fetch --quiet --tags origin
newest="$(git -C vendor/satz rev-parse "$tag^{commit}" 2>/dev/null)" || die "vendor/satz has no tag $tag"
if [[ "$pinned" == "$newest" ]]; then
  echo "check-newest-satz: the pin is $tag, the installed release — nothing newer to read"
  exit 0
fi

[[ -z "$(git -C vendor/satz status --porcelain)" ]] || die "vendor/satz has changes; this checks another release out in it"
trap 'git -C vendor/satz checkout --quiet "$pinned"' EXIT
git -C vendor/satz checkout --quiet "$tag"
echo "check-newest-satz: reading satz $tag (the pin is $(git -C vendor/satz describe --tags "$pinned" 2>/dev/null || echo "$pinned"))"

# not --locked: satz-core is compiled from the checkout, and its dependencies are that
# release's
if cargo test -p satz-studio-core --test grammar_pin --test cst_roundtrip --test cst_tree; then
  echo "check-newest-satz: the app reads the files of satz $tag"
else
  die "satz $tag has a form, a file or a grammar pin this app does not read — \`scripts/move-pin.sh $tag\` moves the pin and the grammar; what then fails says what the document layer has to learn"
fi
