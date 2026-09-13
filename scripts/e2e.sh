#!/usr/bin/env bash
# e2e.sh — the verification harness, end to end: the installed satz is at least
# MIN_SATZ, the e2e tests of the core crate pass against it, and, outside CI, the app
# builds. One verdict line per step; the first failure ends the run.
#
#   bash scripts/e2e.sh          # the satz gate, the e2e tests, a build of the app
#   bash scripts/e2e.sh --ci     # the satz gate and the e2e tests (ci.yml builds the app)
#
# MIN_SATZ is read from crates/satz-studio-core/src/satz/binary.rs, the one place it
# is defined; satz is taken from PATH, else from ~/.local/bin/satz, as the app takes
# it. The tests are `cargo test -p satz-studio-core --locked --test 'e2e_*'`:
# tests/e2e_interview.rs, tests/e2e_map.rs and tests/e2e_edit.rs, each creating its
# own temporary estate over vendor/satz. docs/verification.md says what they prove.
#
# bash 3.2 compatible (macOS default).
set -euo pipefail

ci=0
case "${1:-}" in
  --ci) ci=1 ;;
  '') ;;
  -h|--help) echo "usage: scripts/e2e.sh [--ci]" >&2; exit 0 ;;
  *) echo "usage: scripts/e2e.sh [--ci]" >&2; exit 1 ;;
esac

cd "$(dirname "$0")/.."

pass() { printf 'e2e: ok    %s\n' "$1"; }
fail() { printf 'e2e: FAIL  %s\n' "$1" >&2; exit 1; }

# 1. the satz binary, at least MIN_SATZ
binary_rs=crates/satz-studio-core/src/satz/binary.rs
min_pat='^pub const MIN_SATZ: &str = "[0-9]+\.[0-9]+\.[0-9]+";'
n=$(grep -c -E "$min_pat" "$binary_rs" || true)
[[ "$n" -eq 1 ]] || fail "expected exactly one \`pub const MIN_SATZ: &str = \"X.Y.Z\";\` line in $binary_rs, found $n"
min=$(grep -E "$min_pat" "$binary_rs" | sed -E 's/.*"([0-9.]+)".*/\1/')
satz_bin=$(command -v satz 2>/dev/null || true)
[[ -n "$satz_bin" ]] || satz_bin="$HOME/.local/bin/satz"
[[ -x "$satz_bin" ]] || fail "satz is neither on PATH nor at $HOME/.local/bin/satz"
# the version line is on stdout (`satz X.Y.Z`); the banner on stderr is not read
found=$("$satz_bin" --version 2>/dev/null | sed -n -E 's/^satz v?([0-9]+\.[0-9]+\.[0-9]+).*/\1/p' | head -n 1)
[[ -n "$found" ]] || fail "$satz_bin --version printed no version"
version_ge() { # $1 >= $2, numerically field by field
  local IFS=.
  local a=($1) b=($2) i
  for i in 0 1 2; do
    if (( ${a[$i]} > ${b[$i]} )); then return 0; fi
    if (( ${a[$i]} < ${b[$i]} )); then return 1; fi
  done
  return 0
}
version_ge "$found" "$min" || fail "satz $found at $satz_bin is older than MIN_SATZ $min — run \`satz self-update\`"
pass "satz $found at $satz_bin (MIN_SATZ $min)"

# 2. the e2e tests, against that satz and the pinned vendor/satz
cargo test -p satz-studio-core --locked --test 'e2e_*' || fail "the e2e tests"
pass "cargo test -p satz-studio-core --locked --test 'e2e_*'"

# 3. the app builds (ci.yml has its own build step)
if [[ "$ci" -eq 0 ]]; then
  cargo build -p satz-studio --locked || fail "cargo build -p satz-studio"
  pass "cargo build -p satz-studio --locked"
fi

echo "e2e: every step passed"
