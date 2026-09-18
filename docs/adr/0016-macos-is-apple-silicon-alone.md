# 0016 — macOS is Apple silicon alone

- **Status:** accepted
- **Date:** 2026-09-18
- **Deciders:** Thomas

## Context

Every push ran the test matrix on `macos-15` (arm64) and `macos-15-intel` (x86_64), and
every release tag built two macOS bundles from the same tree. The two runs differ in the
architecture and in nothing else: the same OS, the same webview (WKWebView), the same
keyring store, and a satz binary that is the platform's rather than the chip's. GitHub
prices `macos-15-intel` as the most expensive runner it hosts, and on 2026-09-17 it
flaked — passing in one run of a pair and failing in the other on the same commit.

An Intel bundle is also a promise: an artifact published is an artifact someone installs
and then expects to keep working, and this repository has never run one. The app's own
verification list says so — no `dx bundle` artifact of any OS has been launched by hand
yet.

## Decision

macOS is Apple silicon, in both workflows. `ci.yml` tests on `macos-15` and
`windows-2022`; `release.yml` builds the `.app` and `.dmg` on `macos-15`, the `.deb` and
`.AppImage` on `ubuntu-24.04`, and the `.msi` on `windows-2022`. **Linux and Windows are
unchanged and stay x86_64** — this is a decision about the Mac, not about the
architecture.

## Consequences

- **Good:** the platform matrix halves, every push waits on two runners rather than
  three, and the costliest runner in the catalogue is out of the loop. One flake source
  goes with it.
- **Good:** what is published is what is built and can be checked. Two macOS artifacts
  where one was ever going to be opened by hand is a promise the project cannot keep.
- **Bad:** an Intel Mac gets nothing. Rosetta does not help — the bundle is a native
  binary and there is no universal build here. Someone on an Intel Mac builds from
  source (`cargo` and `dx bundle` both work there; nothing in the tree is
  architecture-dependent) or runs the app elsewhere.
- **Reversible, cheaply:** one matrix entry in each workflow brings it back, and the
  tree needs no change for it. What would not come back is the time between a request
  and a release built for it.
- The satz side is the same shape and already decided: satz publishes arm64 and x86_64
  for macOS, Linux and Windows, because a CLI on a customer's machine is not this app on
  the maintainer's.

## Alternatives

- **Keep both, test one.** Build the Intel bundle on a tag while testing only on Apple
  silicon. Rejected as the worst of the two: it publishes an artifact no test has run
  against, which is how a broken bundle reaches someone.
- **A universal binary.** `dx` has no universal target here, and `lipo` over two builds
  needs the Intel runner anyway — the cost this removes.
