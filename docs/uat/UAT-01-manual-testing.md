# UAT-01: Manual Testing Guide for `lmc` v1

**Status:** Draft  
**Date:** 2026-03-25

## Purpose

This document explains how to manually test the current `lmc` v1 implementation end to end.

It covers:

- local checks that do not require an API key
- live checks for `expand` and `explain`
- shell integration checks for zsh
- failure-path checks

It does not cover:

- bash or fish integration
- Windows
- packaging beyond local Cargo builds

## Scope Under Test

The current v1 surface is:

- `lmc "..."` as shorthand for `lmc expand "..."`
- `lmc expand`
- `lmc explain`
- `lmc audit`
- `lmc init zsh`
- `lmc stats`

## Prerequisites

Before starting, make sure you have:

- Rust and Cargo installed
- either a valid `OPENROUTER_API_KEY` or a local Ollama server with a pulled model
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

For hosted provider tests, export an API key:

```sh
export OPENROUTER_API_KEY="your-key-here"
```

For local provider tests, create a config file:

```toml
[provider]
name = "ollama"
model = "qwen2.5-coder"
base_url = "http://127.0.0.1:11434/api/chat"
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

Use the repo root path below when invoking Cargo from outside the project tree:

```sh
repo="/Users/matteo/Developer/lmcomplete"
```

## Manual Test Cases

### UAT-01.01: Help Output

Run:

```sh
"$repo/target/debug/lmc" --help
```

Pass if:

- help text prints successfully
- commands listed include `expand`, `explain`, `audit`, `init`, and `stats`

### UAT-01.02: `audit` Works Without API Access

Run:

```sh
"$repo/target/debug/lmc" audit "commit all changes with message fix login" --shell zsh --history 0
```

Pass if:

- command exits successfully without needing an API key
- output includes `System prompt:`
- output includes `User prompt:`
- output includes `Shell: zsh`
- output includes `User: commit all changes with message fix login`

### UAT-01.03: `audit` Shows Git Context

Run this from inside the disposable git repo:

```sh
"$repo/target/debug/lmc" audit "commit all changes with message fix login" --shell zsh --history 0
```

Pass if the user prompt includes all of the following:

- `Project: git repo`
- `Git branch: feat/manual-uat`
- a `Git status:` section
- a modified file entry for `app.txt`

### UAT-01.04: Secret Redaction In `audit`

Run:

```sh
"$repo/target/debug/lmc" audit 'curl -H "Authorization: Bearer sk-secret-value" https://example.com' --shell zsh --history 0
```

Pass if:

- output includes a warning section
- the user prompt contains `[REDACTED]`
- the raw token `sk-secret-value` does not appear anywhere in the output

### UAT-01.05: `init zsh` Prints A Widget Script

Run:

```sh
"$repo/target/debug/lmc" init zsh
```

Pass if:

- the command exits successfully
- output contains `lmc-expand-buffer`
- output contains `bindkey`

### UAT-01.06: `stats` Works Before Any Live Requests

Run:

```sh
"$repo/target/debug/lmc" stats
```

Pass if:

- the command exits successfully
- output contains numeric fields for requests and tokens

Note:

- values may already be non-zero because `stats` is cumulative and persisted on disk

### UAT-01.07: `expand` Works With A Live Provider

Prerequisite:

- `OPENROUTER_API_KEY` must be set

Run from inside the disposable git repo:

```sh
"$repo/target/debug/lmc" expand "show git status" --shell zsh --history 0
```

Pass if:

- the command exits successfully
- output is a shell command, not markdown or prose
- output is not auto-executed
- output is plausibly equivalent to `git status` or `git status --short`

### UAT-01.08: Default CLI Shorthand Works

Run from inside the disposable git repo:

```sh
"$repo/target/debug/lmc" "show git status"
```

Pass if:

- the command exits successfully
- behavior is equivalent to `lmc expand "show git status"`

### UAT-01.09: `explain` Works With A Live Provider

Run:

```sh
"$repo/target/debug/lmc" explain "tar xzf archive.tar.gz" --shell zsh --history 0
```

Pass if:

- the command exits successfully
- output is plain text explanation
- output is concise
- output explains `tar`, extraction, gzip, and/or the archive file

### UAT-01.10: `stats` Changes After Live Requests

Run `stats`, then run one successful `expand` or `explain`, then run `stats` again:

```sh
"$repo/target/debug/lmc" stats
"$repo/target/debug/lmc" explain "tar xzf archive.tar.gz" --shell zsh --history 0
"$repo/target/debug/lmc" stats
```

Pass if:

- the request count increases
- token totals increase
- last request timestamp is present or changes

### UAT-01.11: Missing API Key Fails Cleanly

Open a clean shell with no provider key:

```sh
env -u OPENROUTER_API_KEY "$repo/target/debug/lmc" expand "show git status"
```

Pass if:

- the command exits non-zero
- stderr reports a missing provider API key
- no shell command is printed as a successful result

### UAT-01.12: Config Permission Enforcement Works

Create a config file with the wrong mode:

```sh
badcfg="$(mktemp)"
cat > "$badcfg" <<'EOF'
[provider]
name = "openrouter"
api_key = "fake-key"
model = "meta-llama/llama-4-scout"
EOF
chmod 644 "$badcfg"
"$repo/target/debug/lmc" --config "$badcfg" explain "tar xzf archive.tar.gz"
```

Pass if:

- the command exits non-zero
- stderr reports that the config file must have mode `0600`

### UAT-01.13: zsh Prefix Trigger Works

Start a clean zsh without your usual shell config:

```sh
zsh -f
```

Inside that shell run:

```sh
eval "$("$repo/target/debug/lmc" init zsh)"
```

Then type this into the prompt without pressing Enter:

```sh
?? show git status
```

Press `Tab`.

Pass if:

- the buffer content is replaced with the expanded command
- the command is not auto-executed
- you can inspect or edit the command before pressing Enter

### UAT-01.14: zsh Shift+Tab Widget Works

In the same clean zsh session, type:

```sh
show git status
```

Press `Shift+Tab`.

Pass if:

- the buffer content is replaced with the expanded command
- the command is not auto-executed

## Acceptance Summary

The manual test passes if:

- all local-only checks pass
- live `expand` and `explain` pass with a valid provider key
- zsh integration replaces the buffer and never auto-executes
- secret values are redacted in `audit`
- failure cases exit clearly and safely

## Known Current Limitations

These are not failures for v1 UAT:

- only OpenRouter is supported
- only zsh widget installation is supported
- `stats` is cumulative, not per shell session
- Kubernetes detection is best-effort only
- command wording from the model may vary, so live-command checks should be judged by intent-equivalence, not exact string match
