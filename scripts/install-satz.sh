#!/usr/bin/env bash
# install-satz.sh — installs the satz release the tests drive.
#
#   scripts/install-satz.sh            # the NEWEST release, held to MIN_SATZ or newer
#   scripts/install-satz.sh vX.Y.Z     # that release exactly
#
# satz keeps only its five newest releases (its prune workflow deletes older
# releases and their tags), so an installer asset pinned by tag is gone within
# days. The app's contract is MIN_SATZ OR NEWER, so the runner installs the newest
# release and this script holds it to the floor.
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

# The runner's own satz configuration: no update check. satz prints "Update
# available" with println! — on STDOUT — whenever a newer release than the pinned
# one exists, and then every `--format json` command starts with that line and
# no test can parse it. Written only when no config exists, which is the case on
# a CI runner; a person's config is never touched.
satz_cfg="${XDG_CONFIG_HOME:-$HOME/.config}/satz/satz.toml"
if [[ ! -f "$satz_cfg" ]]; then
  mkdir -p "$(dirname "$satz_cfg")"
  printf 'self_update_frequency = "never"\n' > "$satz_cfg"
  echo "install-satz: wrote $satz_cfg (self_update_frequency = never)"
fi
binary_rs="$repo_root/crates/satz-studio-core/src/satz/binary.rs"
min_pat='^pub const MIN_SATZ: &str = "[0-9]+\.[0-9]+\.[0-9]+";'

case $# in
  0)
    n=$(grep -c -E "$min_pat" "$binary_rs" || true)
    [[ "$n" -eq 1 ]] || die "expected exactly one \`pub const MIN_SATZ: &str = \"X.Y.Z\";\` line in $binary_rs, found $n"
    floor="$(grep -E "$min_pat" "$binary_rs" | sed -E 's/.*"([0-9.]+)".*/\1/')"
    tag="latest" ;;
  1)
    case "$1" in
      -h|--help) usage; exit 0 ;;
      v[0-9]*.[0-9]*.[0-9]*) tag="$1"; floor="" ;;
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

if [[ "$tag" == "latest" ]]; then
  base="https://github.com/tjirsch/satz/releases/latest/download"
else
  base="https://github.com/tjirsch/satz/releases/download/$tag"
fi
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
got=$(printf '%s\n' "$version" | sed -n -E 's/^satz ([0-9]+\.[0-9]+\.[0-9]+)$/\1/p' | tail -1)
[[ -n "$got" ]] || die "$bin printed no 'satz X.Y.Z' line: $version"
if [[ "$tag" == "latest" ]]; then
  # the floor: the newest release must be MIN_SATZ or newer (sort -V orders versions)
  lowest=$(printf '%s\n%s\n' "$floor" "$got" | sort -V | head -1)
  [[ "$lowest" == "$floor" ]] || die "the newest satz release is $got, below MIN_SATZ $floor — move the pin down or wait for satz"
  echo "install-satz: satz $got (MIN_SATZ $floor)"
else
  [[ "$got" == "${tag#v}" ]] || die "$bin reports $got, not ${tag#v}"
fi
