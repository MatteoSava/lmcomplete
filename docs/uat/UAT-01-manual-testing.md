# UAT-01: Manual Testing Guide for `lmcomplete` v0.1.0

**Status:** Draft  
**Date:** 2026-03-26

## Purpose

This document explains how to manually test the current `lmcomplete` v0.1.0 implementation end to end.

It covers:

- local checks that do not require an API key
- install and launch smoke checks
- live checks for `expand` and `explain`
- shell integration checks for zsh
- failure-path checks

It does not cover:

- bash or fish integration
- Windows
- packaging beyond local Cargo builds

## Scope Under Test

The current launch surface is:

- `lmc "..."` as shorthand for `lmc expand "..."`
- `lmc expand`
- `lmc explain`
- `lmc audit`
- `lmc init zsh`
- `lmc stats`

## Prerequisites

Before starting, make sure you have:

- Rust and Cargo installed
- a valid `OPENROUTER_API_KEY` for live `expand` and `explain` tests
- zsh installed for shell widget testing
- network access for live `expand` and `explain`

## Test Environment Setup

From the repository root:

```sh
cargo build
cargo test
```

Expected result:

- the project builds successfully
- the automated test suite passes

For live provider tests, export an API key:

```sh
export OPENROUTER_API_KEY="your-key-here"
```

Optional: install the local package and use the installed `lmc` binary:

```sh
cargo install --path .
lmc --help
```

Optional: build the binary once and use it directly:

```sh
cargo build
./target/debug/lmc --help
```

## Test Workspace Setup

Use a disposable git repository for the cwd-aware tests:

```sh
tmpdir="$(mktemp -d)"
cd "$tmpdir"
git init
git checkout -b feat/manual-uat
printf 'hello\n' > app.txt
git add app.txt
git commit -m "init"
printf 'change\n' >> app.txt
```

If Git refuses to commit because user name or email is unset, configure them locally in the temp repo:

```sh
git config user.name "UAT"
git config user.email "uat@example.com"
git add app.txt
git commit -m "init"
printf 'change\n' >> app.txt
```

Use the repo root path below when invoking the binary from outside the project tree:

```sh
repo="/Users/matteo/Developer/lmcomplete"
bin="$repo/target/debug/lmc"
```

## Manual Test Cases

### UAT-01.01: Help Output

Run:

```sh
"$bin" --help
```

Pass if:

- help text prints successfully
- commands listed include `expand`, `explain`, `audit`, `init`, and `stats`

### UAT-01.02: Install Smoke

Run:

```sh
lmc --help
```

Pass if:

- the command exits successfully
- help text prints from the installed binary

### UAT-01.03: `audit` Works Without API Access

Run:

```sh
"$bin" audit "commit all changes with message fix login" --shell zsh --history 0
```

Pass if:

- command exits successfully without needing an API key
- output includes `System prompt:`
- output includes `User prompt:`
- output includes `Shell: zsh`
- output includes `User: commit all changes with message fix login`

### UAT-01.04: `audit` Shows Git Context

Run this from inside the disposable git repo:

```sh
"$bin" audit "commit all changes with message fix login" --shell zsh --history 0
```

Pass if the user prompt includes all of the following:

- `Project: git repo`
- `Git branch: feat/manual-uat`
- a `Git status:` section
- a modified file entry for `app.txt`

### UAT-01.05: Secret Redaction In `audit`

Run:

```sh
"$bin" audit 'curl -H "Authorization: Bearer sk-secret-value" https://example.com' --shell zsh --history 0
```

Pass if:

- output includes a warning section
- the user prompt contains `[REDACTED]`
- the raw token `sk-secret-value` does not appear anywhere in the output

### UAT-01.06: `init zsh` Prints A Widget Script

Run:

```sh
"$bin" init zsh
```

Pass if:

- the command exits successfully
- output contains `lmc-expand-buffer`
- output contains `bindkey`

### UAT-01.07: `stats` Works Before Any Live Requests

Run:

```sh
"$bin" stats
```

Pass if:

- the command exits successfully
- output contains numeric fields for requests and tokens

Note:

- values may already be non-zero because `stats` is cumulative and persisted on disk

### UAT-01.08: `expand` Works With A Live Provider

Prerequisite:

- `OPENROUTER_API_KEY` must be set

Run from inside the disposable git repo:

```sh
"$bin" expand "show git status" --shell zsh --history 0
```

Pass if:

- the command exits successfully
- output is a shell command, not markdown or prose
- output starts appearing before the full command completes when run in an interactive terminal
- output is not auto-executed
- output is plausibly equivalent to `git status` or `git status --short`

### UAT-01.09: Default CLI Shorthand Works

Run from inside the disposable git repo:

```sh
"$bin" "show git status"
```

Pass if:

- the command exits successfully
- behavior is equivalent to `lmc expand "show git status"`

### UAT-01.10: `explain` Works With A Live Provider

Run:

```sh
"$bin" explain "tar xzf archive.tar.gz" --shell zsh --history 0
```

Pass if:

- the command exits successfully
- output is plain text explanation
- output starts appearing before the full explanation completes when run in an interactive terminal
- output is concise
- output explains `tar`, extraction, gzip, and/or the archive file
