#!/usr/bin/env bash
# install-satz.sh — installs the satz release the tests drive.
#
#   scripts/install-satz.sh            # the tag is MIN_SATZ, prefixed with v
#   scripts/install-satz.sh vX.Y.Z     # that release
#
# MIN_SATZ is the ONE minimum satz version: `pub const MIN_SATZ` in
# crates/satz-studio-core/src/satz/binary.rs. The script downloads the cargo-dist
# installer of the tagged GitHub release together with its SHA-256 sidecar
# (`<hex>  satz-installer.sh`, or a bare hex), verifies the installer against the
# sidecar — a missing sidecar is a failure, never a skipped check — runs it with
# --no-modify-path (satz's install-path is ~/.local/bin) and prints the version of
# the binary it installed, which must be the tag's.
#
# Windows: satz has no Windows release; exit 2. CI builds it from the submodule
# vendor/satz there (.github/workflows/ci.yml).
#
# bash 3.2 compatible (macOS default).
set -euo pipefail

usage() { echo "usage: scripts/install-satz.sh [vX.Y.Z]" >&2; }
die() { echo "install-satz: $*" >&2; exit 1; }

case "$(uname -s 2>/dev/null || true):${OS:-}" in
  MINGW*|MSYS*|CYGWIN*|*:Windows_NT)
    echo "install-satz: satz has no Windows release; CI builds it from vendor/satz instead" >&2
    exit 2 ;;
esac

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
binary_rs="$repo_root/crates/satz-studio-core/src/satz/binary.rs"
min_pat='^pub const MIN_SATZ: &str = "[0-9]+\.[0-9]+\.[0-9]+";'

case $# in
  0)
    n=$(grep -c -E "$min_pat" "$binary_rs" || true)
    [[ "$n" -eq 1 ]] || die "expected exactly one \`pub const MIN_SATZ: &str = \"X.Y.Z\";\` line in $binary_rs, found $n"
    tag="v$(grep -E "$min_pat" "$binary_rs" | sed -E 's/.*"([0-9.]+)".*/\1/')" ;;
  1)
    case "$1" in
      -h|--help) usage; exit 0 ;;
      v[0-9]*.[0-9]*.[0-9]*) tag="$1" ;;
      *) usage; die "a tag looks like vX.Y.Z, not '$1'" ;;
    esac ;;
  *) usage; exit 1 ;;
esac

if command -v sha256sum >/dev/null 2>&1; then
  sha256() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
  sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
  die "neither sha256sum nor shasum is on PATH"
fi

base="https://github.com/tjirsch/satz/releases/download/$tag"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
echo "install-satz: $tag from $base"
curl -fsSL -o "$tmp/satz-installer.sh" "$base/satz-installer.sh" \
  || die "download failed: $base/satz-installer.sh (no such release, or no installer asset)"
curl -fsSL -o "$tmp/satz-installer.sh.sha256" "$base/satz-installer.sh.sha256" \
  || die "no SHA-256 sidecar at $base/satz-installer.sh.sha256 — an installer without one is not run"

expected=$(awk 'NR == 1 { print tolower($1) }' "$tmp/satz-installer.sh.sha256")
case "$expected" in
  ''|*[!0-9a-f]*) die "the sidecar is not a SHA-256 (\`<hex>  satz-installer.sh\` or a bare hex): $(head -c 200 "$tmp/satz-installer.sh.sha256")" ;;
esac
[[ ${#expected} -eq 64 ]] || die "the sidecar hash has ${#expected} hex characters, not 64"
actual=$(sha256 "$tmp/satz-installer.sh")
[[ "$actual" == "$expected" ]] || die "SHA-256 mismatch for satz-installer.sh: sidecar $expected, downloaded $actual"
echo "install-satz: satz-installer.sh verified ($actual)"

sh "$tmp/satz-installer.sh" --no-modify-path

bin="$HOME/.local/bin/satz"
[[ -x "$bin" ]] || die "the installer ran but $bin is not there"
version=$("$bin" --version)
printf '%s\n' "$version"
want=$(printf '%s' "${tag#v}" | sed 's/\./\\./g')
printf '%s\n' "$version" | grep -q -E "(^|[ v])${want}"'( |$)' || die "$bin reports a version other than ${tag#v}"
