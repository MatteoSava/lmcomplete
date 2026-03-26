# `lmc` Launch Demo Plan

## Purpose

Create one short terminal demo that proves the public launch story:

- `lmc` is fast to install
- `lmc init zsh` is the shell onboarding path
- `lmc` expands commands with context
- `lmc explain` is useful for review and trust
- `lmc audit` shows what leaves the machine
- `lmc stats` confirms local usage tracking

Keep the demo tight. The target runtime is 45-60 seconds.

## Intended Assets

Store all launch media under `docs/launch/assets/`.

- `docs/launch/assets/lmc-launch-demo.mov`: raw capture from the terminal recording session
- `docs/launch/assets/lmc-launch-demo.mp4`: edited release-ready video
- `docs/launch/assets/lmc-launch-demo.gif`: short looping preview for README or social posts
- `docs/launch/assets/lmc-launch-demo-poster.png`: thumbnail frame for embeds and release notes

## Scene List

### Scene 1: Install and identity

- Duration: 6-8 seconds
- Terminal: a clean shell in the repository root or a fresh user shell
- Commands:
  - `lmc --help`
  - `lmc --version`
- On-screen point:
  - the binary is `lmc`
  - the launch identity is `lmcomplete`

### Scene 2: Shell onboarding

- Duration: 8-10 seconds
- Commands:
  - `lmc init zsh`
  - `eval "$(lmc init zsh)"`
- On-screen point:
  - widget install is one command
  - the user stays in zsh

### Scene 3: Context-aware expansion

- Duration: 10-12 seconds
- Location: a small disposable git repo with one modified file
- Commands:
  - `lmc "show git status"`
  - `lmc expand "commit all changes with message fix login" --shell zsh --history 0`
- On-screen point:
  - output is a shell command, not prose
  - git context makes the result relevant

### Scene 4: Explanation

- Duration: 8-10 seconds
- Commands:
  - `lmc explain "tar xzf archive.tar.gz" --shell zsh --history 0`
- On-screen point:
  - plain-text explanation
  - concise enough to read in one glance

### Scene 5: Trust and redaction

- Duration: 8-10 seconds
- Commands:
  - `lmc audit 'curl -H "Authorization: Bearer sk-secret-value" https://example.com' --shell zsh --history 0`
- On-screen point:
  - secret-looking values are redacted
  - the user can inspect the prompt bundle before sending anything

### Scene 6: Local stats

- Duration: 4-6 seconds
- Commands:
  - `lmc stats`
- On-screen point:
  - usage is tracked locally
  - stats are simple and non-intrusive

## Capture Notes

- Use a terminal theme with high contrast and no distracting background animation.
- Prefer a real shell session over a scripted screen mock; the point is to show the native interaction.
- Keep the prompt text short and readable at video scale.
- Record in a clean repo so the git-context scene is obvious.
- Do not show the command executing automatically.
- If a destructive command warning appears, leave it visible long enough to read.

## Editing Notes

- Trim dead time between commands.
- Keep the first 5 seconds focused on the product name and binary name.
- Include one or two quick zooms only if the terminal font becomes too small in the final cut.
- Export a looping GIF from the first three scenes only if the file size stays reasonable.
