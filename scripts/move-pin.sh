#!/usr/bin/env bash
# move-pin.sh — moves the satz pin and everything that has to move with it, in one step:
#
#   1. the submodule vendor/satz to the tag;
#   2. the vendored grammar to the commit that satz names in its
#      editors/zed/extension.toml (tests/grammar_pin.rs holds the two together);
#   3. the recorded reports under crates/satz-studio-core/tests/fixtures/ that one satz
#      command writes: the presets report, the two reviews, the interfaces report and the
#      two questions reports.
#
#   scripts/move-pin.sh v0.95.2
#
# It needs the satz binary of that release on PATH, or in ~/.local/bin as the app takes
# it — the reports are that binary's output — and a clean work tree. It commits nothing.
#
# Not recorded here: merge-presets-smoke.json, which is `satz_merge_presets` over a
# pristine library built for the test (crates/satz-studio-core/src/satz/reports.rs says
# how). What is left to a person afterwards is printed at the end.
set -euo pipefail

die() { echo "move-pin: $*" >&2; exit 1; }
[[ $# -eq 1 && "$1" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "usage: scripts/move-pin.sh vX.Y.Z" >&2; exit 1; }
tag="$1"
version="${tag#v}"

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"
[[ -z "$(git status --porcelain)" ]] || die "the work tree has changes; commit or set them aside first"

satz="$(command -v satz || true)"
[[ -n "$satz" ]] || satz="$HOME/.local/bin/satz"
[[ -x "$satz" ]] || die "no satz on PATH or in ~/.local/bin — install $tag first (scripts/install-satz.sh $tag)"
found="$("$satz" --version 2>/dev/null | tail -1 | awk '{print $2}')"
[[ "$found" == "$version" ]] || die "$satz is satz $found, and the reports are recorded from $version — install $tag first (scripts/install-satz.sh $tag)"

# 1. the submodule
git -C vendor/satz fetch --quiet --tags origin
git -C vendor/satz checkout --quiet "$tag" || die "vendor/satz has no tag $tag"
echo "move-pin: vendor/satz -> $tag"

# 2. the grammar satz names
manifest=vendor/satz/editors/zed/extension.toml
field() {
  awk -v key="$1" '
    /^\[grammars\.satz\]/ { s = 1; next }
    /^\[/                 { s = 0 }
    s && $1 == key        { gsub(/"/, "", $3); print $3 }
  ' "$manifest"
}
url="$(field repository)"
rev="$(field rev)"
[[ -n "$url" && -n "$rev" ]] || die "$manifest names no grammar repository and rev"
if [[ "$(cat vendor/satz-tree-sitter/COMMIT)" == "$rev" ]]; then
  echo "move-pin: the grammar is already $rev"
else
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  git clone --quiet "$url" "$tmp/grammar"
  git -C "$tmp/grammar" checkout --quiet "$rev" || die "$url has no commit $rev"
  bash scripts/sync-grammar.sh "$tmp/grammar"
fi

# 3. the recorded reports. Paths inside them are written as the tests read them: the
#    repository root as /e, and the fixture config's own prefix taken off.
fixtures=crates/satz-studio-core/tests/fixtures
config=tests/fixtures/smoke/config.toml
record() { # $1 the file under fixtures/, the rest the satz command; exit 1 is a verdict
  local out="$fixtures/$1"; shift
  # a name with its extension: satz adds `.json` to one that has none
  local dir; dir="$(mktemp -d)"
  local raw="$dir/report.json"
  "$satz" --config "$config" "$@" --format json --out "$raw" > /dev/null 2>&1 || true
  # an empty or missing report is never recorded: a fixture that is nothing makes every
  # test that reads it fail for a reason that is not satz's
  [[ -s "$raw" ]] || die "satz $* wrote no report"
  sed -e "s#$repo_root#/e#g" -e 's#tests/fixtures/smoke/\.\./\.\./\.\./##g' "$raw" > "$out"
  rm -rf "$dir"
  [[ -s "$out" ]] || die "$out came out empty"
  echo "move-pin: recorded $out"
}
record presets-smoke.json presets vendor/satz/tests/smoke/yaml/smoke.satz
record interfaces-showcase.json interfaces showcase.satz
record questions-smoke.json questions vendor/satz/tests/smoke/yaml/smoke.satz
record questions-showcase.json questions showcase.satz
record review/organization-budget.json review-preset vendor/satz/presets/organization-budget.satz
record review/team-access.json review-preset "$fixtures/review/team-access.satz"
# the broken pack is recorded as standing in an estate's packs/, where the test reads it
sed -i.bak "s#/e/$fixtures/review/#/e/packs/#g" "$fixtures/review/team-access.json" && rm -f "$fixtures/review/team-access.json.bak"

cat <<EOF

move-pin: done, nothing committed. Left to you:
  - \`cargo test --workspace --locked\` and \`bash scripts/e2e.sh\`: a form satz added that
    the document layer does not map fails there (crates/satz-studio-core/src/cst/build.rs);
  - merge-presets-smoke.json, when satz changed the merge report;
  - NOTICE, when satz's own changed (tests/notice.rs says so);
  - MIN_SATZ, only when the app now uses something $tag introduced.
EOF
