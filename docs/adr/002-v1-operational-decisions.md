# ADR-002: V1 Operational Decisions and Scope Tightening

**Status:** Accepted
**Date:** 2026-03-25
**Amends:** ADR-001

## Context

ADR-001 established the high-level architecture for `lmc`, but left several runtime decisions open:

- What a valid model response looks like
- How fast the shell path must be, and what happens on failure
- How much context is allowed into a prompt
- What `audit`, `explain`, and `stats` mean in practice
- What config schema we support in v1
- How we evaluate prompt quality before release
- How v1 is packaged and what remains out of scope

Without these decisions, the implementation will drift and the shell UX will be inconsistent.

## Decision

### 1. Tightened v1 scope

V1 ships a one-shot CLI for:

- `expand`: natural language to shell command
- `explain`: shell command to concise explanation
- `audit`: show the fully assembled outbound payload after redaction
- `init zsh`: install-time shell integration for zsh only
- `stats`: local usage totals

V1 does not ship:

- Command execution
- Interactive repair loops
- Multi-step workflow planning
- Ollama or any local/offline provider
- Bash/Fish widget installers
- Windows support
- Snippet libraries
- Arbitrary file-content ingestion

The provider trait remains in place, but the only supported provider shipped in v1 is OpenRouter.

### 2. Output contract

#### `expand`

The `expand` response is valid only if all of the following are true:

- It resolves to exactly one shell command string
- It contains no markdown fences, headings, or explanatory prose
- It is non-empty after trimming
- It does not contain multiple alternatives phrased as prose such as "or", "alternatively", or enumerated options
- It may contain one leading safety line exactly equal to `# WARNING: destructive command`, followed by the command on the next line

The client must normalize the model output in this order:

1. Trim outer whitespace
2. Strip a single wrapping code fence if present
3. Re-trim
4. Validate against the contract above

If validation fails, the request is treated the same as a provider failure and may use the fallback model once.

#### `explain`

The `explain` response must be:

- Plain text only
- Concise
- Either one short paragraph or up to 5 bullet points
- Focused on what the command does, notable flags, and obvious safety implications

`explain` is informational only. It does not need shell-ready formatting.

### 3. Latency and failure policy

Shell integration must remain fast enough to feel native.

V1 request budgets:

- Primary model timeout: 1500 ms
- Fallback model timeout: 1500 ms
- Total wall-clock budget for `expand`: 3000 ms
- No retries beyond a single fallback attempt

Fallback is triggered when:

- The primary request times out
- The provider returns `429` or any `5xx`
- The response cannot be parsed
- The response fails output validation

On failure:

- `lmc` exits non-zero
- The CLI prints a concise error to stderr
- The zsh widget leaves the shell buffer unchanged

No part of v1 may auto-execute the resulting command.

### 4. Prompt and context budgeting

V1 uses simple character budgets rather than tokenizer-specific budgets. This keeps the implementation cheap and deterministic.

Outbound prompt rules:

- Total dynamic prompt payload target: 4000 characters
- User input is always included
- Shell and OS are always included
- Project labels are always included when detected
- Absolute paths must be converted to relative paths when surfaced

Section budgets:

- Git branch: 120 chars
- Git remotes: up to 5 lines, 400 chars total
- Git status: up to 20 lines, 1200 chars total
- History: up to `history.max_entries`, but never more than 10 lines or 1000 chars total
- `package.json` scripts: up to 20 names
- Compose services: up to 20 names
- Make targets: up to 20 names

When the prompt exceeds budget, sections are dropped in this order:

1. History
2. Git remotes
3. Git status lines beyond the first 10
4. Derived project metadata such as scripts, services, and targets

Shell, OS, user input, project labels, and git branch are never dropped.

### 5. Context collection rules

V1 context collection must stay cheap and local.

Allowed:

- File existence checks
- Small metadata reads such as `package.json` script keys or `go.mod` module name
- Git commands used in ADR-001

Not allowed:

- Reading source file contents into the prompt
- Recursive repository scans for semantic understanding
- Sending env var values
- Sending raw absolute paths

Kubernetes detection in v1 is best-effort only:

- A `k8s/` directory counts as Kubernetes context
- Top-level `*.yaml` or `*.yml` files containing `apiVersion:` count as Kubernetes context

Recursive manifest discovery is deferred.

### 6. Secret filtering and `audit`

All outbound user input and gathered context must pass through the same redaction pipeline before leaving the machine.

`audit` is the trust tool for this system. It must show:

- The final system prompt
- The final user prompt
- Redaction warnings, if any
- Which categories of sensitive patterns matched

`audit` must never print the unredacted secret value back to the terminal.

### 7. Config schema

V1 configuration is TOML at `~/.config/lmcomplete/config.toml`.

The supported schema is:

```toml
[provider]
name = "openrouter"
model = "openai/gpt-oss-120b:groq"
api_key = "sk-..."
base_url = "https://openrouter.ai/api/v1/chat/completions"

[provider.fallback]
name = "openrouter"
model = "anthropic/claude-3.5-sonnet"

[history]
max_entries = 10
max_line_chars = 120
output_entries = 2
max_output_chars = 500

[request]
timeout_ms = 1500
fallback_timeout_ms = 1500

[context]
max_prompt_chars = 4000

[agent]
command = "claude -p"

[fix]
auto_suggest = true

[streaming]
enabled = true
```

Config rules:

- `config.toml` must be mode `0600` on Unix systems
- `OPENROUTER_API_KEY` is supported
- Environment variables take precedence over file values
- Missing config falls back to built-in defaults except for secrets

Fields beyond this schema are ignored in v1.

### 8. `stats` semantics

ADR-001 used the phrase "per session." In v1, `stats` is defined as cumulative local totals persisted on disk.

V1 `stats` includes:

- Request count
- Prompt tokens
- Completion tokens
- Total tokens
- Total cost
- Last request timestamp

V1 does not include:

- Per-shell-session segmentation
- Per-model breakdown
- Reset subcommands

If a reset is needed in v1, deleting the stats file is sufficient.

### 9. Evaluation and release gate

V1 requires both code tests and prompt evaluations.

Required checks before release:

- Unit and integration tests must pass
- Eval runner must process the JSONL dataset without crashing
- Zero secret leaks in redaction/audit fixtures
- Zero uncaught panics across the eval dataset
- T1 exact-match score: at least 85%
- T2 exact-match score: at least 75%
- T3 exact-match score: at least 70%

Destructive-command cases require manual review even when the exact-match gate passes.

Exact-match scoring must normalize:

- Outer whitespace
- Repeated spaces
- Optional leading destructive warning line

### 10. Packaging and release

V1 distribution is intentionally narrow:

- Prebuilt binaries via GitHub Releases for macOS and Linux
- Checksums published with each release
- `cargo install --path .` remains the local development path

Deferred until after v1 stabilizes:

- Homebrew distribution
- Self-update
- OS package managers beyond GitHub Releases

### 11. Non-goals clarified

The following are explicitly out of scope for v1:

- Executing commands on behalf of the user
- Building a shell AST or command parser
- Guaranteeing perfectly portable quoting across all shells
- Generating long shell scripts as the default behavior
- Reading file contents for semantic project understanding
- Remote telemetry or product analytics

### 12. XDG path resolution

**Crate:** `etcetera` — respects `XDG_CONFIG_HOME` on all platforms including macOS (unlike the `dirs` crate which ignores XDG overrides on non-Linux).

| Purpose | Path | Default |
|---------|------|---------|
| Config | `$XDG_CONFIG_HOME/lmcomplete/config.toml` | `~/.config/lmcomplete/config.toml` |
| Cache | `$XDG_CACHE_HOME/lmcomplete/` | `~/.cache/lmcomplete/` |
| Stats | `$XDG_CACHE_HOME/lmcomplete/stats.json` | `~/.cache/lmcomplete/stats.json` |
| Last output | `$XDG_CACHE_HOME/lmcomplete/last_output` | `~/.cache/lmcomplete/last_output` |

### 13. OpenRouter prompt caching

The system prompt must be a **stable prefix** that never changes between calls. All dynamic context (history, git status, cwd) goes in the user message, not the system message. This maximizes cache hit rate across providers.

Per-provider behavior through OpenRouter:

| Provider | Caching | Config needed |
|----------|---------|---------------|
| Groq | Automatic | None |
| OpenAI | Automatic (min 1024 tokens) | None |
| Anthropic | Manual | Add `cache_control` breakpoint after system message |
| Gemini | Manual (5min TTL) | Add `cache_control` breakpoint |

The system prompt should be structured to stay above 1024 tokens so it qualifies for OpenAI automatic caching. The response `usage.prompt_tokens_details.cached_tokens` field is logged to stats when available.

`lmc` adds `cache_control: { type: "ephemeral" }` after the system message content for Anthropic models. Model detection uses the model ID prefix (`anthropic/`, `google/`).

### 14. Previous command output capture

Extends section 4 (context budgeting) with richer history context.

**History entries (last 10 commands):**
- Each entry: first 120 characters, truncated with `…` if longer
- Source: ZSH `precmd` hook reads `fc -ln -1`

**Output capture (last 2 commands):**
- Capture stdout/stderr of the last 2 commands, truncated to 500 characters each
- Source: ZSH `preexec` hook starts capture, `precmd` hook reads and stores result
- Stored at `$XDG_CACHE_HOME/lmcomplete/last_output` as JSON:

```json
{
  "commands": [
    {"cmd": "cargo test", "exit_code": 1, "output": "error[E0382]: use of moved value..."},
    {"cmd": "git status", "exit_code": 0, "output": "On branch main\nM src/main.rs"}
  ]
}
```

- Previous command exit code is always included in context regardless of budget
- The `preexec`/`precmd` pair uses a temp file for output capture. If capture fails, only the command string and exit code are stored.

Updated section budget additions:
- History commands: up to 10 lines, 1200 chars total (was 1000)
- Last 2 command outputs: up to 1000 chars total
- Total dynamic prompt target raised to 5000 chars to accommodate output capture

### 15. ZLE widget keybindings

The shell integration installed by `lmc init zsh` provides these keybindings:

| Keybinding | Action | Implementation |
|-----------|--------|---------------|
| `Shift+Tab` | Expand: natural language → shell command | Save `UNDO_CHANGE_NO`, replace BUFFER with `lmc expand` result |
| `Shift+Tab` again | Undo: restore original text | Detect `_lmc_expanded=1`, call `zle undo` to saved change number |
| `Ctrl+E` | Explain current BUFFER | Call `lmc explain "$BUFFER"`, show result via `zle -M` (above prompt) |
| `??` prefix + `Tab` | Prefix trigger expand | Strip `??` prefix, then same as Shift+Tab |
| `Ctrl+F` | Accept fix suggestion | After auto-suggest on failure, place the fix into BUFFER |

**State tracking variables:**

- `_lmc_expanded` (0 or 1) — toggles Shift+Tab between expand and undo
- `_lmc_undo_point` — stores `UNDO_CHANGE_NO` captured before expansion
- `_lmc_fix_suggestion` — stores the last fix suggestion from auto-suggest

State is reset (`_lmc_expanded=0`) on any keypress other than the lmc keybindings, detected via `zle-line-pre-redraw`.

### 16. Agent delegation

`lmc` does not build an agent. It delegates to external agent CLIs configured by the user.

```toml
[agent]
command = "claude -p"
# alternatives: "codex exec", "aider --message"
```

Usage:

```sh
lmc agent "refactor the auth module"
```

This shells out to the configured command with the query appended as an argument. No output parsing, no chaining, no retry logic. `lmc` exits with the same exit code as the delegated command.

### 17. `lmc fix`

Suggests a corrected command after a failure. Two triggers:

**Auto-suggest (via `precmd` hook):**
1. `precmd` detects `$? != 0`
2. Runs `lmc fix --suggest` in the background
3. When result arrives, shows the suggestion above the prompt via `zle -M`
4. User presses `Ctrl+F` to accept the fix into BUFFER, or ignores it

**Explicit:**
- User types `lmc fix` after a failure
- Can also be bound to a keybinding

**Context sent to LLM:**
- The failed command (from history)
- Exit code
- stderr output (truncated to 500 chars, from last_output cache)
- cwd + project type context

**System prompt for fix:**
```
The previous command failed. Suggest a corrected command.
Return ONLY the fixed command. No explanation, no markdown.
```

Returns corrected command into BUFFER — never auto-executes.

Configurable:
```toml
[fix]
auto_suggest = true
```

Set `auto_suggest = false` to disable the `precmd` hook trigger. Explicit `lmc fix` always works.

### 18. Streaming response in CLI and widget

Use the OpenRouter streaming API (`"stream": true`) for interactive `expand` and `explain` invocations.

- Direct CLI invocations stream to the terminal when stdout is a TTY
- The zsh widget streams preview updates into BUFFER progressively via `zle -R`
- `expand` previews are normalized as they arrive, then finalized through the safety pipeline
- First token typically appears in ~200ms vs ~800ms for full response
- If user presses any key during streaming, streaming is cancelled and the partial content remains in BUFFER
- Non-streaming fallback for providers that don't support SSE

Configurable:
```toml
[streaming]
enabled = true
```

When `enabled = false`, the widget waits for the full response before updating BUFFER (original behavior).

### Shell aliases awareness

**Deferred** — revisit after v1 stabilizes.

## Consequences

- The implementation has a smaller, testable v1 boundary
- Some useful future behaviors are intentionally deferred to keep latency and trust high
- Character-based prompt budgeting is less precise than token budgeting, but simpler and stable enough for v1
- `stats` is less sophisticated than originally implied, but operationally clear
- Packaging remains lightweight and reversible until the CLI interface settles
- Prompt caching reduces cost and latency for repeat users, but requires the system prompt to remain stable
- Output capture via `preexec`/`precmd` hooks adds shell integration complexity but significantly improves context quality
- Streaming adds perceived speed but introduces partial-state handling in the widget
- `lmc fix` auto-suggest adds a background process per failed command — configurable off if unwanted
- Agent delegation is intentionally thin — avoids building agent infrastructure that already exists elsewhere
