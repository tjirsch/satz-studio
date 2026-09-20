# A release is `cargo release` on `main`, not a version-bump pull request

- **Status:** accepted
- **Date:** 2026-09-20
- **Deciders:** the maintainer

## Context

Every change reaches `main` through a squash-merged pull request, and nothing else is
committed to `main`. A release needs one more commit on top of that: the workspace
version in `Cargo.toml`, which `.github/workflows/release.yml` reads for every bundle's
file name and which the tag `vX.Y.Z` repeats.

Putting that commit through a pull request runs the full matrix again — Linux, macOS
and Windows, each with formatting, clippy, the test suite, the verification harness and
a build of the app, the Windows job alone about twenty-five minutes — over a tree that
has just passed all of it. The only difference between the two trees is the version
line. v0.5.1's bump pull request was closed for that reason, which left the changes it
was meant to release sitting on `main` unreleased.

satz releases the other way: `cargo release patch|minor --execute --no-confirm` on
`main` bumps, commits, tags and pushes in one step, and its `CLAUDE.md` states that as
the exception to its own branch rule.

## Decision

A release is `cargo release patch|minor --execute --no-confirm`, run on `main` once
`main` is green. `release.toml` holds the configuration: the commit message
`version bump`, the tag `v{{version}}`, `push = true`, `publish = false`, and one
shared version for the two crates, which both inherit `workspace.package.version`.

The version-bump commit is the one commit that lands on `main` without a pull request.
`CLAUDE.md` and `CONTRIBUTING.md` say so where they state the branch rule, so the
exception is written down rather than discovered in the reflog.

`release.yml` refuses a tag whose name is not `v` plus the workspace version, so a tag
pushed by hand cannot produce bundles whose file names disagree with their tag.

## Consequences

- A release is one command and one CI run — the tag's — instead of two.
- `main` carries commits nobody reviewed. They are version lines, written by a tool,
  and the tag is pushed with them, so the window in which `main` disagrees with the
  release is a second rather than a review cycle.
- A bad release is answered the way satz answers one: the next patch release. There is
  no pull request to close instead.
- Anyone with push rights to `main` can cut a release. That is already true of the tag.

## Options

### `cargo release` on `main` *(chosen)*

- Good: one step, one CI run, no second review of a version line.
- Good: the same flow as satz, so one habit serves both repositories.
- Bad: a commit on `main` that no pull request gated.
- Bad: the rule "nothing is committed to `main` directly" now has an exception, and an
  exception has to be written down to stay one.

### A version-bump pull request

- Good: every commit on `main` came through review, with no exception to explain.
- Bad: a full matrix run — about twenty-five minutes of it on Windows — over a tree
  that has just passed the same matrix.
- Bad: the release is two steps with a wait between them, which is how v0.5.1's bump
  came to be abandoned and its changes left unreleased.

### A workflow that bumps and tags from a dispatch

- Good: no local tooling, and the exception lives in a workflow rather than in a habit.
- Bad: the workflow needs write access to `main` — the same exception, granted to a
  token instead of to a person, and harder to see.
- Bad: a release is then only as reproducible as the runner, and cannot be run from a
  checkout that is already green.
