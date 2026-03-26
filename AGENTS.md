# AGENTS.md

## Working Style

- Act like a high-performing senior engineer.
- Be concise, direct, and execution-focused.
- Prefer simple, maintainable, production-friendly solutions.
- Write low-complexity code that is easy to read, debug, and modify.
- Do not overengineer or add heavy abstractions, extra layers, or large dependencies for small features.
- Keep APIs small, behavior explicit, and naming clear.
- Avoid cleverness unless it clearly improves the result.

## Repo Context

- This repository is a Rust CLI app.
- The binary is `lmc`.
- Main entry points live under `src/`, with CLI integration coverage in `tests/cli.rs`.
- Command implementations live under `src/commands/`.

## Normal Workflow

- Keep diffs focused and easy to review.
- For behavior changes, add or update tests.
- Prefer extending existing patterns over introducing new architecture.
- If CLI output or UX changes, update tests and any affected docs or help text.

## Config Documentation

- Treat `docs/config.example.toml` as the canonical user-facing config reference.
- When config fields, defaults, or validation rules change, update `docs/config.example.toml` in the same diff.
- Keep `README.md` and docs pointing at the canonical example instead of duplicating full schema blocks.
- Add or update a test that parses the canonical example with the real config types.

## Bug Workflow

- When a bug is reported, do not start with a fix.
- First, write a test that reproduces the bug.
- Then have subagents try to fix it when subagents are available.
- Do not consider the bug fixed until the reproducing test passes.

## Validation

- Run `cargo fmt` after Rust code changes.
- Run `cargo test` for normal validation.
- Use targeted runs like `cargo test --test cli` when iterating on CLI behavior.

## Tooling Constraints

- I use `curlie` for `curl`, so either use `curlie` or unalias `curl` first.
- Always use `uv` for any Python operation. Never use system `python` or `pip`.
- Never use Bitnami Helm Charts.
