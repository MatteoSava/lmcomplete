# ADR-001: Initial Architecture for lmcomplete

**Status:** Accepted
**Date:** 2026-03-25

> Follow-up operational decisions and v1 scope tightening are captured in ADR-002.

## Context

Typing precise shell commands (git, curl, docker, kubectl, etc.) requires memorizing syntax. lmcomplete bridges the gap: type natural language, get the real command.

## Decision

### Language: Rust

- Single static binary, no runtime dependency
- Fast startup (critical for shell integration — must feel instant)
- Strong ecosystem for CLI (clap) and async HTTP (reqwest/tokio)

### Binary name: `lmc`

### Core features (v1)

1. **Expand** — `lmc "commit this file"` → `git add <file> && git commit`
2. **Explain** — `lmc explain "tar xzf archive.tar.gz"` → human-readable breakdown
3. **Command history context** — feed last N commands from shell history to the LLM for contextual understanding ("undo that", "do the same for the other file")
4. **Cwd-aware context** — detect git repo, package.json, Cargo.toml, k8s manifests, etc. and include relevant context in the prompt
5. ~~**Snippet library**~~ — deferred to v2

### Shell integration (v1: zsh, later: bash, fish)

- **Shift+Tab widget** — ZLE widget that captures current buffer, sends to lmc, replaces buffer with result. User presses Enter to execute.
- **Prefix trigger** — type `?? commit this file` then Tab. The `??` prefix signals lmc to expand. Less intrusive, no keybinding conflicts.
- Both modes show the expanded command for review before execution (dry-run by default, never auto-execute).

### LLM backend

- Trait-based provider abstraction (`Provider` trait with `complete()` method)
- Initial provider: **OpenRouter** (targeting Groq models for speed)
- Config in `~/.config/lmcomplete/config.toml`:
  ```toml
  [provider]
  name = "openrouter"
  api_key = "sk-..."  # or env var OPENROUTER_API_KEY
  model = "meta-llama/llama-4-scout"

  [provider.fallback]
  name = "openrouter"
  model = "anthropic/claude-3.5-sonnet"
  ```
- Adding a new provider = implement the trait, register in factory
- Future: ollama for fully local/offline use

### Architecture (high-level)

```
lmc CLI (clap)
  ├── commands/
  │   ├── expand.rs      — natural language → shell command
  │   └── explain.rs     — shell command → explanation
  ├── context/
  │   ├── history.rs     — read shell history file
  │   ├── cwd.rs         — detect project type, read relevant files
  │   └── shell.rs       — detect current shell, format output accordingly
  ├── provider/
  │   ├── mod.rs         — Provider trait
  │   ├── openrouter.rs  — OpenRouter/Groq implementation
  │   └── ollama.rs      — (future) local model support
  ├── prompt/
  │   └── builder.rs     — assemble system prompt + context + user input
  ├── shell/
  │   └── zsh_widget.zsh — ZLE widget script (installed by `lmc init zsh`)
  └── config.rs          — config loading from TOML
```

### Shell widget installation

```sh
# User adds to .zshrc:
eval "$(lmc init zsh)"
```

This outputs the ZLE widget definition and keybindings. The widget calls `lmc expand --shell zsh "$(BUFFER)"` and replaces BUFFER with the result.

### Safety

- Never auto-execute. Always show the command and wait for user confirmation (Enter).
- Warn on destructive patterns (rm -rf, DROP TABLE, force push).
- Token/cost tracking per session with `lmc stats`.

## LLM Context Payload

The prompt sent to the LLM is assembled from multiple context sources. Getting this right is critical for accuracy.

### System prompt (static)

```
You are a shell command generator. Given a natural language description,
return ONLY the shell command. No explanation, no markdown, no backticks.
If the command is destructive (rm -rf, DROP, --force), prefix with: # WARNING: destructive command
Shell: {shell_type} | OS: {os_type}
```

### Context payload (dynamic, assembled per-request)

| Field | Source | Example | Sent? |
|-------|--------|---------|-------|
| `query` | User input | "commit all changes" | Always |
| `shell` | `$SHELL` env var | "zsh" | Always |
| `os` | `std::env::consts::OS` | "macos" | Always |
| `cwd_project` | File detection in cwd | See below | When detected |
| `history` | Shell history file | Last N commands, redacted | When enabled |
| `git_status` | `git status --short` | "M src/main.rs" | When in git repo |
| `git_branch` | `git branch --show-current` | "feat/login" | When in git repo |
| `git_remotes` | `git remote -v` | "origin github.com/..." | When in git repo |

### Cwd project detection (check for file existence, cheap)

| File/Dir | Project type | Extra context added |
|----------|-------------|-------------------|
| `.git/` | Git repo | branch, status, remotes |
| `Cargo.toml` | Rust project | — |
| `package.json` | Node project | scripts keys from package.json |
| `go.mod` | Go project | module name |
| `Dockerfile` | Docker project | — |
| `docker-compose.yml` / `compose.yml` | Compose project | service names |
| `k8s/` or `*.yaml` with `apiVersion` | Kubernetes | — |
| `Makefile` | Make project | target names |
| `pyproject.toml` / `setup.py` | Python project | — |
| `Terraform/` or `*.tf` | Terraform | — |

### What is NOT sent (hard rules)

- File contents (only filenames/metadata)
- Environment variable values
- Anything matching secret patterns (see below)
- Full absolute paths (shortened to relative from cwd)

### Example assembled prompt

For `"commit all changes with message fix login"`:

```
Shell: zsh | OS: macos
Project: git repo (branch: feat/login, remote: origin → github.com/user/app)
Status: M src/main.rs, M src/auth.rs
Recent commands:
  cargo test
  cargo clippy

User: commit all changes with message fix login
```

## Secret Filtering

### Library: `secretscan` (Rust, 50+ built-in patterns)

Used to scan shell history lines and any context before it's included in the LLM prompt. Provides regex-based pattern matching for:

- API keys (AWS, GCP, Azure, OpenAI, Anthropic, Stripe, etc.)
- Tokens (GitHub `ghp_*`, GitLab `glpat-*`, npm, PyPI, etc.)
- Passwords in URLs (`https://user:pass@host`)
- Private keys (RSA, SSH, PGP headers)
- Connection strings with credentials
- JWT tokens
- Generic high-entropy strings

### Filtering pipeline

```
Shell history → secretscan patterns → replace matches with [REDACTED] → include in prompt
User query → secretscan patterns → warn user if secret detected (don't block)
```

### Additional hardcoded patterns (on top of secretscan)

```
export \w*KEY\w*=.*      → export API_KEY=[REDACTED]
export \w*SECRET\w*=.*   → export AWS_SECRET=[REDACTED]
export \w*TOKEN\w*=.*    → export GH_TOKEN=[REDACTED]
export \w*PASSWORD\w*=.* → export DB_PASSWORD=[REDACTED]
-H ['"]Authorization:.*  → -H 'Authorization: [REDACTED]'
```

### `lmc audit` command

Dry-runs the full pipeline and shows exactly what would be sent, with redacted fields highlighted. Essential for user trust.

## Security & Privacy

- No telemetry, no data collection by lmc itself
- API calls go directly to the configured provider — no lmc proxy/middleman
- Config file permissions enforced: `config.toml` must be `0600` (contains API key)
- Support for API key via env var (`OPENROUTER_API_KEY`) to avoid storing on disk
- All context passes through secret filtering before leaving the machine
- With ollama provider (v2), zero data leaves the machine

## Competitive Analysis

| Tool | Language | Shell integration | LLM provider | Offline | Key difference |
|------|----------|------------------|--------------|---------|----------------|
| **shell-gpt (sgpt)** | Python | No (separate command) | OpenAI | No | Full Python runtime needed, no shell widget |
| **ai-shell** | TypeScript | No (interactive TUI) | OpenAI | No | Node.js runtime, TUI-based not inline |
| **github copilot CLI** | TypeScript | No (separate command) | GitHub/OpenAI | No | Tied to GitHub ecosystem |
| **navi** | Rust | Yes (widget) | None (cheatsheets) | Yes | No LLM, manual cheatsheets only |
| **lmc (ours)** | Rust | Yes (inline widget) | Multi-provider | Planned | Inline expansion, no runtime, multi-provider, context-aware |

### lmc's differentiation

1. **Inline expansion** — command appears in your prompt, not a separate TUI. Feels native.
2. **No runtime** — single binary vs Python/Node.js dependency chains
3. **Provider-agnostic** — not locked to OpenAI. Start with fastest (Groq via OpenRouter), swap anytime.
4. **Context-aware** — history + cwd detection makes expansions more accurate than stateless tools
5. **Shell-native** — ZLE widget makes it feel like a shell feature, not an external tool

## Eval Dataset

Prompt quality is critical — a bad system prompt means wrong commands. We maintain a structured eval dataset to iterate on prompts.

### Three-tier dataset (~650+ pairs)

| Tier | Source | Size | Purpose |
|------|--------|------|---------|
| **T1: Curated core** | Hand-written | ~100 | Our target domains, high quality |
| **T2: NL2Bash subset** | Filtered from [NL2Bash](https://github.com/TellinaTool/nl2bash) (MIT) | ~500 | Standard Linux commands (find, grep, sed, awk, tar, chmod) |
| **T3: Context-aware** | Hand-written | ~50 | History/cwd dependent ("undo that", "same for prod") |

### Dataset format

JSONL at `eval/dataset.jsonl`:

```json
{"id": "git-001", "tier": "T1", "input": "commit all changes with message fix login bug", "expected": "git add -A && git commit -m 'fix login bug'", "domain": "git", "context": {"cwd_type": "git"}}
{"id": "ctx-001", "tier": "T3", "input": "undo that", "expected": "git reset --soft HEAD~1", "domain": "git", "context": {"cwd_type": "git", "history": ["git commit -m 'wip'"]}}
```

### Sources

- [NL2Bash dataset](https://github.com/TellinaTool/nl2bash) — 9,305 pairs, MIT license
- [NL2CMD (NeurIPS 2020)](https://github.com/magnumresearchgroup/Magnum-NLC2CMD) — winning solution, extended dataset
- [NL2CMD paper](https://arxiv.org/abs/2302.07845) — 6x larger auto-generated dataset
- [IBM NL2Bash-EABench](https://github.com/IBM/nl2bash-eabench) — execution-based evaluation benchmark

## Consequences

- Rust means slower iteration than Python but better UX (startup time, single binary)
- OpenRouter dependency means network required (until ollama support lands)
- ZLE widget approach is proven (similar to zsh-autosuggestions) but requires `eval` in .zshrc
- Snippet library deferred to v2 to keep initial scope tight
