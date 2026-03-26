use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin;
use predicates::prelude::*;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[test]
fn init_zsh_prints_widget() {
    Command::cargo_bin("lmc")
        .unwrap()
        .args(["init", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("lmc-expand-buffer"))
        .stdout(predicate::str::contains("lmc-handle-expand-event"))
        .stdout(predicate::str::contains("zle -F -w"))
        .stdout(predicate::str::contains("_LMC_IN_FLIGHT"))
        .stdout(predicate::str::contains("_lmc_loading_indicator"));
}

#[test]
fn audit_prints_prompt_bundle() {
    Command::cargo_bin("lmc")
        .unwrap()
        .args([
            "audit",
            "commit all changes",
            "--shell",
            "zsh",
            "--history",
            "0",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("System prompt:"))
        .stdout(predicate::str::contains("User: commit all changes"));
}

#[test]
fn expand_stream_format_tty_emits_partial_output_before_exit() {
    let temp_dir = tempfile::tempdir().unwrap();
    let (base_url, server) = spawn_streaming_server(vec![
        (
            r#"data: {"choices":[{"delta":{"content":"git "}}]}

"#
            .to_string(),
            Duration::from_millis(40),
        ),
        (
            r#"data: {"choices":[{"delta":{"content":"status"}}],"usage":{"prompt_tokens":1,"completion_tokens":2,"total_tokens":3,"cost":0.1}}

data: [DONE]

"#
            .to_string(),
            Duration::from_millis(250),
        ),
    ]);
    let config_path = write_test_config(temp_dir.path(), &base_url, true);

    let mut child = ProcessCommand::new(cargo_bin("lmc"))
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "expand",
            "show git status",
            "--shell",
            "zsh",
            "--history",
            "0",
            "--stream-format",
            "tty",
        ])
        .env("HOME", temp_dir.path())
        .env("XDG_CONFIG_HOME", temp_dir.path().join(".config"))
        .env("XDG_STATE_HOME", temp_dir.path().join(".state"))
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let mut stdout = child.stdout.take().unwrap();
    let mut first = [0u8; 64];
    let first_len = stdout.read(&mut first).unwrap();
    assert!(first_len > 0);
    assert!(child.try_wait().unwrap().is_none());

    let mut output = String::from_utf8_lossy(&first[..first_len]).into_owned();
    let mut tail = String::new();
    stdout.read_to_string(&mut tail).unwrap();
    output.push_str(&tail);

    let status = child.wait().unwrap();
    server.join().unwrap();

    assert!(status.success());
    assert!(output.contains("git "), "stdout was: {output}");
    assert!(output.ends_with("git status\n"), "stdout was: {output}");
}

#[test]
fn streaming_expand_targets_groq_provider_when_model_uses_groq_suffix() {
    let temp_dir = tempfile::tempdir().unwrap();
    let captured = Arc::new(Mutex::new(String::new()));
    let (base_url, server) = spawn_streaming_server_with_capture(
        vec![(
            r#"data: {"choices":[{"delta":{"content":"git status"}}],"usage":{"prompt_tokens":1,"completion_tokens":2,"total_tokens":3,"cost":0.1}}

data: [DONE]

"#
            .to_string(),
            Duration::from_millis(10),
        )],
        captured.clone(),
    );
    let config_path =
        write_test_config_with_model(temp_dir.path(), &base_url, true, "openai/gpt-oss-120b:groq");

    Command::cargo_bin("lmc")
        .unwrap()
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "expand",
            "show git status",
            "--shell",
            "zsh",
            "--history",
            "0",
            "--stream-format",
            "tty",
        ])
        .env("HOME", temp_dir.path())
        .env("XDG_CONFIG_HOME", temp_dir.path().join(".config"))
        .env("XDG_STATE_HOME", temp_dir.path().join(".state"))
        .assert()
        .success();

    server.join().unwrap();

    let request = captured.lock().unwrap().clone();
    assert!(
        request.contains(r#""model":"openai/gpt-oss-120b""#),
        "request was: {request}"
    );
    assert!(
        request.contains(r#""stream":true"#),
        "request was: {request}"
    );
    assert!(
        request.contains(r#""provider":{"only":["groq"],"allow_fallbacks":false}"#),
        "request was: {request}"
    );
    assert!(
        request.contains(r#""temperature":0.0"#),
        "request was: {request}"
    );
}

#[test]
fn explain_stream_format_tty_emits_partial_output_before_exit() {
    let temp_dir = tempfile::tempdir().unwrap();
    let (base_url, server) = spawn_streaming_server(vec![
        (
            r#"data: {"choices":[{"delta":{"content":"- lists files"}}]}

"#
            .to_string(),
            Duration::from_millis(40),
        ),
        (
            r#"data: {"choices":[{"delta":{"content":"\n- shows hidden files"}}],"usage":{"prompt_tokens":2,"completion_tokens":4,"total_tokens":6,"cost":0.2}}

data: [DONE]

"#
            .to_string(),
            Duration::from_millis(250),
        ),
    ]);
    let config_path = write_test_config(temp_dir.path(), &base_url, true);

    let mut child = ProcessCommand::new(cargo_bin("lmc"))
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "explain",
            "ls -la",
            "--shell",
            "zsh",
            "--history",
            "0",
            "--stream-format",
            "tty",
        ])
        .env("HOME", temp_dir.path())
        .env("XDG_CONFIG_HOME", temp_dir.path().join(".config"))
        .env("XDG_STATE_HOME", temp_dir.path().join(".state"))
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let mut stdout = child.stdout.take().unwrap();
    let mut first = [0u8; 64];
    let first_len = stdout.read(&mut first).unwrap();
    assert!(first_len > 0);
    assert!(child.try_wait().unwrap().is_none());

    let mut output = String::from_utf8_lossy(&first[..first_len]).into_owned();
    let mut tail = String::new();
    stdout.read_to_string(&mut tail).unwrap();
    output.push_str(&tail);

    let status = child.wait().unwrap();
    server.join().unwrap();

    assert!(status.success());
    assert!(output.contains("- lists files"), "stdout was: {output}");
    assert_eq!(output, "- lists files\n- shows hidden files\n");
}

#[test]
fn stream_format_off_keeps_one_shot_output() {
    let temp_dir = tempfile::tempdir().unwrap();
    let body = r#"{"choices":[{"message":{"content":"git status"}}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2,"cost":0.1}}"#;
    let (base_url, server) = spawn_json_server(body.to_string(), Duration::from_millis(120));
    let config_path = write_test_config(temp_dir.path(), &base_url, true);

    Command::cargo_bin("lmc")
        .unwrap()
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "expand",
            "show git status",
            "--shell",
            "zsh",
            "--history",
            "0",
            "--stream-format",
            "off",
        ])
        .env("HOME", temp_dir.path())
        .env("XDG_CONFIG_HOME", temp_dir.path().join(".config"))
        .env("XDG_STATE_HOME", temp_dir.path().join(".state"))
        .assert()
        .success()
        .stdout(predicate::eq("git status\n"));

    server.join().unwrap();
}

#[test]
fn widget_streaming_emits_only_final_done_event() {
    let temp_dir = tempfile::tempdir().unwrap();
    let (base_url, server) = spawn_streaming_server(vec![
        (
            r#"data: {"choices":[{"delta":{"content":"terraform "}}]}

"#
            .to_string(),
            Duration::from_millis(20),
        ),
        (
            r#"data: {"choices":[{"delta":{"content":"plan -out=tfplan"}}]}

"#
            .to_string(),
            Duration::from_millis(20),
        ),
        (
            r#"data: {"choices":[{"delta":{"content":" && terraform show -json tfplan"}}],"usage":{"prompt_tokens":1,"completion_tokens":2,"total_tokens":3,"cost":0.1}}

data: [DONE]

"#
            .to_string(),
            Duration::from_millis(20),
        ),
    ]);
    let config_path = write_test_config(temp_dir.path(), &base_url, true);

    let output = ProcessCommand::new(cargo_bin("lmc"))
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "expand",
            "terraform plan in json?",
            "--shell",
            "zsh",
            "--history",
            "0",
            "--stream-format",
            "widget",
        ])
        .env("HOME", temp_dir.path())
        .env("XDG_CONFIG_HOME", temp_dir.path().join(".config"))
        .env("XDG_STATE_HOME", temp_dir.path().join(".state"))
        .output()
        .unwrap();

    server.join().unwrap();

    assert!(
        output.status.success(),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("preview\t"), "stdout was: {stdout}");
    assert!(stdout.contains("done\tstatus=ok"), "stdout was: {stdout}");
    assert!(
        stdout.contains("command=terraform plan -out=tfplan && terraform show -json tfplan"),
        "stdout was: {stdout}"
    );
}

#[test]
fn zsh_widget_finish_handles_success_without_reserved_status_variable() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));

    let script = format!(
        r#"
zle() {{ :; }}
bindkey() {{ :; }}
source "{widget_path}"
BUFFER="original"
CURSOR=${{#BUFFER}}
_LMC_OUTPUT_FILE="$(mktemp)"
_LMC_ERROR_FILE="$(mktemp)"
print -r -- "git status" > "$_LMC_OUTPUT_FILE"
_lmc_finish_expand 0
print -r -- "$BUFFER"
"#
    );

    let output = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "git status");
}

#[test]
fn zsh_widget_can_be_sourced_twice_without_readonly_errors() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));

    let script = format!(
        r#"
zle() {{ :; }}
bindkey() {{ :; }}
source "{widget_path}"
source "{widget_path}"
BUFFER="original"
CURSOR=${{#BUFFER}}
_LMC_OUTPUT_FILE="$(mktemp)"
_LMC_ERROR_FILE="$(mktemp)"
print -r -- "git status" > "$_LMC_OUTPUT_FILE"
_lmc_finish_expand 0
print -r -- "$BUFFER"
"#
    );

    let output = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "git status");
}

#[test]
fn zsh_widget_reloads_after_previous_readonly_globals() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));

    let script = format!(
        r#"
zle() {{ :; }}
bindkey() {{ :; }}
typeset -gr _LMC_GREY=$'\033[90m'
typeset -gr _LMC_RESET=$'\033[0m'
source "{widget_path}"
"#
    );

    let output = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn zsh_widget_reports_missing_provider_config_before_starting_expand() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));
    let temp_dir = tempfile::tempdir().unwrap();
    let home = temp_dir.path().display();

    let script = format!(
        r#"
export HOME="{home}"
export XDG_CONFIG_HOME="{home}/.config"
unset OPENROUTER_API_KEY
LAST_MESSAGE=""
zle() {{
  case "$1" in
    -M)
      if [[ "$2" == "--" ]]; then
        LAST_MESSAGE="$3"
      else
        LAST_MESSAGE="$2"
      fi
      ;;
    -F|-R|-N|redisplay|expand-or-complete)
      ;;
  esac
}}
bindkey() {{ :; }}
source "{widget_path}"
BUFFER="list all the pods"
CURSOR=${{#BUFFER}}
_lmc_expand_buffer
print -r -- "BUFFER=$BUFFER"
print -r -- "MESSAGE=$LAST_MESSAGE"
print -r -- "IN_FLIGHT=$_LMC_IN_FLIGHT"
"#
    );

    let output = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("BUFFER=list all the pods"),
        "stdout was: {stdout}"
    );
    assert!(
        stdout.contains("MESSAGE=Set OPENROUTER_API_KEY or configure "),
        "stdout was: {stdout}"
    );
    assert!(stdout.contains("IN_FLIGHT=0"), "stdout was: {stdout}");
}

#[test]
fn zsh_widget_uses_cursor_style_loading_frames() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));

    let script = format!(
        r#"
zle() {{ :; }}
bindkey() {{ :; }}
source "{widget_path}"
print -r -- "${{(j:,:)_LMC_LOADING_FRAMES}}"
"#
    );

    let output = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("▏"), "stdout was: {stdout}");
    assert!(stdout.contains("█"), "stdout was: {stdout}");
    assert!(!stdout.contains(".  "), "stdout was: {stdout}");
}

#[test]
fn zsh_widget_emits_cursor_ticks_while_expand_is_running() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));

    let script = format!(
        r#"
zle() {{ :; }}
bindkey() {{ :; }}
source "{widget_path}"
_LMC_STATUS_FILE="$(mktemp)"
command rm -f -- "$_LMC_STATUS_FILE"
fifo="$(mktemp -u)"
mkfifo "$fifo"
exec {{event_fd}}<>"$fifo"
sleep 0.15 &
command_pid=$!
_lmc_emit_loading_ticks "$fifo" "$command_pid" &
ticker_pid=$!
IFS=$'\t' read -r event frame <&"$event_fd"
wait "$command_pid"
wait "$ticker_pid"
exec {{event_fd}}<&-
command rm -f -- "$fifo" "$_LMC_STATUS_FILE"
print -r -- "EVENT=$event"
print -r -- "FRAME=$frame"
"#
    );

    let output = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("EVENT=tick"), "stdout was: {stdout}");
    assert!(
        stdout.contains("FRAME=▎")
            || stdout.contains("FRAME=▍")
            || stdout.contains("FRAME=▌")
            || stdout.contains("FRAME=▋")
            || stdout.contains("FRAME=▊")
            || stdout.contains("FRAME=▉")
            || stdout.contains("FRAME=█"),
        "stdout was: {stdout}"
    );
}

#[test]
fn zsh_widget_shows_loading_message_during_expand() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));
    let temp_dir = tempfile::tempdir().unwrap();
    let bin_dir = temp_dir.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_lmc(
        &bin_dir,
        "#!/bin/zsh\nsleep 0.4\nprint -r -- $'done\tstatus=ok\twarning=none\tcommand=git status'\n",
    );

    let path = format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap());
    let script = format!(
        r#"
export PATH="{path}"
export OPENROUTER_API_KEY="test-key"
LAST_MESSAGE=""
zle() {{
  case "$1" in
    -M)
      if [[ "$2" == "--" ]]; then
        LAST_MESSAGE="$3"
      else
        LAST_MESSAGE="$2"
      fi
      ;;
    -F|-R|-N|redisplay|expand-or-complete)
      ;;
  esac
}}
bindkey() {{ :; }}
source "{widget_path}"
BUFFER="show git status"
CURSOR=${{#BUFFER}}
_lmc_expand_buffer
print -r -- "MESSAGE=$LAST_MESSAGE"
"#
    );

    let output = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("MESSAGE=[lmc "), "stdout was: {stdout}");
}

#[test]
fn zsh_widget_tick_keeps_loading_message_without_prompt_reset() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));

    let script = format!(
        r#"
LAST_MESSAGE=""
RESET_COUNT=0
REDRAW_COUNT=0
zle() {{
  case "$1" in
    -M)
      if [[ "$2" == "--" ]]; then
        LAST_MESSAGE="$3"
      else
        LAST_MESSAGE="$2"
      fi
      ;;
    reset-prompt)
      LAST_MESSAGE=""
      (( RESET_COUNT++ ))
      ;;
    -R)
      (( REDRAW_COUNT++ ))
      ;;
    -N|-F|redisplay|expand-or-complete)
      ;;
  esac
}}
bindkey() {{ :; }}
source "{widget_path}"
fifo="$(mktemp -u)"
mkfifo "$fifo"
exec {{event_fd}}<>"$fifo"
print -r -- $'tick\t.. ' > "$fifo"
_lmc_handle_expand_event $event_fd
print -r -- "MESSAGE=${{LAST_MESSAGE:-<empty>}}"
print -r -- "RESET=$RESET_COUNT"
print -r -- "REDRAW=$REDRAW_COUNT"
command rm -f -- "$fifo"
"#
    );

    let output = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("MESSAGE=[lmc .. ]"), "stdout was: {stdout}");
    assert!(stdout.contains("RESET=0"), "stdout was: {stdout}");
    assert!(stdout.contains("REDRAW=1"), "stdout was: {stdout}");
}

#[test]
fn zsh_widget_loading_indicator_places_cursor_before_label() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));

    let script = format!(
        r#"
LAST_MESSAGE=""
zle() {{
  case "$1" in
    -M)
      if [[ "$2" == "--" ]]; then
        LAST_MESSAGE="$3"
      else
        LAST_MESSAGE="$2"
      fi
      ;;
    -R|-N|-F|redisplay|expand-or-complete)
      ;;
  esac
}}
bindkey() {{ :; }}
source "{widget_path}"
_lmc_show_loading_state "▍"
print -r -- "POSTDISPLAY=$POSTDISPLAY"
print -r -- "MESSAGE=$LAST_MESSAGE"
"#
    );

    let output = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let postdisplay = stdout
        .lines()
        .find_map(|line| line.strip_prefix("POSTDISPLAY="))
        .expect("expected postdisplay output");

    let cursor_idx = postdisplay.find("▍").expect("missing cursor frame");
    let label_idx = postdisplay.find("lmc").expect("missing lmc label");
    assert!(cursor_idx < label_idx, "postdisplay was: {postdisplay}");
    assert!(
        !postdisplay.contains("[lmc"),
        "postdisplay was: {postdisplay}"
    );
    assert!(stdout.contains("MESSAGE=[lmc ▍]"), "stdout was: {stdout}");
}

#[test]
fn zsh_widget_stream_error_message_is_red_and_clears_on_next_redraw() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));

    let script = format!(
        r#"
LAST_MESSAGE=""
zle() {{
  case "$1" in
    -M)
      if [[ "$2" == "--" ]]; then
        LAST_MESSAGE="$3"
      else
        LAST_MESSAGE="$2"
      fi
      ;;
    -R|-N|-F|redisplay|expand-or-complete)
      ;;
  esac
}}
bindkey() {{ :; }}
source "{widget_path}"
BUFFER="terraform plan in json"
CURSOR=${{#BUFFER}}
_LMC_ORIGINAL_BUFFER="$BUFFER"
_lmc_finish_stream_error "OpenRouter returned an empty completion"
print -r -- "ERROR=$LAST_MESSAGE"
LASTWIDGET="self-insert"
_lmc_line_pre_redraw
print -r -- "CLEARED=${{LAST_MESSAGE:-<empty>}}"
"#
    );

    let output = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ERROR=\u{1b}[31mOpenRouter returned an empty completion\u{1b}[0m"),
        "stdout was: {stdout}"
    );
    assert!(stdout.contains("CLEARED=<empty>"), "stdout was: {stdout}");
}

#[test]
fn zsh_widget_stream_success_handler_invalidates_display_before_redraw() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));

    let script = format!(
        r#"
INVALIDATE_COUNT=0
REDRAW_COUNT=0
zle() {{
  case "$1" in
    -I)
      (( INVALIDATE_COUNT++ ))
      ;;
    -R)
      (( REDRAW_COUNT++ ))
      ;;
    -M|-N|-F|redisplay|expand-or-complete|reset-prompt)
      ;;
  esac
}}
bindkey() {{ :; }}
source "{widget_path}"
BUFFER="kub"
CURSOR=${{#BUFFER}}
_LMC_ORIGINAL_BUFFER="$BUFFER"
WIDGET="lmc-handle-expand-event"
_lmc_finish_stream_success "none" "kubectl describe pods -A"
print -r -- "BUFFER=$BUFFER"
print -r -- "INVALIDATE=$INVALIDATE_COUNT"
print -r -- "REDRAW=$REDRAW_COUNT"
"#
    );

    let output = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("BUFFER=kubectl describe pods -A"),
        "stdout was: {stdout}"
    );
    assert!(stdout.contains("INVALIDATE=1"), "stdout was: {stdout}");
    assert!(stdout.contains("REDRAW=1"), "stdout was: {stdout}");
}

#[test]
fn zsh_widget_stream_success_handler_redraw_preserves_previous_shell_output() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));

    let script = format!(
        r#"
PREVIOUS_OUTPUT="terraform plan summary"
REDRAW_COUNT=0
zle() {{
  case "$1" in
    -R)
      (( REDRAW_COUNT++ ))
      ;;
    -M|-N|-F|redisplay|expand-or-complete)
      ;;
  esac
}}
bindkey() {{ :; }}
source "{widget_path}"
BUFFER="tf"
CURSOR=${{#BUFFER}}
_LMC_ORIGINAL_BUFFER="$BUFFER"
WIDGET="lmc-handle-expand-event"
_lmc_finish_stream_success "none" "terraform plan -out=tfplan"
print -r -- "BUFFER=$BUFFER"
print -r -- "PREVIOUS_OUTPUT=${{PREVIOUS_OUTPUT:-<empty>}}"
print -r -- "REDRAW=$REDRAW_COUNT"
"#
    );

    let output = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("BUFFER=terraform plan -out=tfplan"),
        "stdout was: {stdout}"
    );
    assert!(
        stdout.contains("PREVIOUS_OUTPUT=terraform plan summary"),
        "stdout was: {stdout}"
    );
    assert!(stdout.contains("REDRAW=1"), "stdout was: {stdout}");
}

#[test]
fn zsh_widget_real_terminal_does_not_duplicate_injected_result_line() {
    if !Path::new("/usr/bin/script").exists() {
        return;
    }

    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));
    let zshrc = format!(
        r#"
PROMPT='> '
RPROMPT=''
source "{widget_path}"
_test_inject() {{
  _LMC_ORIGINAL_BUFFER="$BUFFER"
  _lmc_finish_stream_success none 'echo INJECTED-COMMAND'
}}
zle -N test-inject _test_inject
bindkey '^T' test-inject
"#
    );

    let transcript = run_scripted_zsh_session(
        &zshrc,
        vec![
            (
                Duration::from_millis(200),
                b"git clone --depth 1 https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git"
                    .to_vec(),
            ),
            (Duration::from_millis(200), b"\x14".to_vec()),
            (Duration::from_millis(400), b"\nexit\n".to_vec()),
        ],
    );

    let injected_count = transcript.matches("echo INJECTED-COMMAND").count();
    assert_eq!(injected_count, 1, "transcript was:\n{transcript}");
}

#[test]
fn zsh_widget_preview_event_does_not_mutate_buffer() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));
    let temp_dir = tempfile::tempdir().unwrap();
    let bin_dir = temp_dir.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_lmc(
        &bin_dir,
        "#!/bin/zsh\nprint -r -- $'preview\tcommand=git'\nsleep 0.2\nprint -r -- $'done\tstatus=ok\twarning=none\tcommand=git status'\n",
    );

    let path = format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap());
    let script = format!(
        r#"
export PATH="{path}"
export OPENROUTER_API_KEY="test-key"
EVENT_HANDLER=""
zle() {{
  case "$1" in
    -F)
      if [[ "$2" == "-w" ]]; then
        EVENT_HANDLER="_lmc_handle_expand_event"
      else
        EVENT_HANDLER="$3"
      fi
      ;;
    -M|-R|-N|redisplay|expand-or-complete)
      ;;
  esac
}}
bindkey() {{ :; }}
source "{widget_path}"
BUFFER="show git status"
CURSOR=${{#BUFFER}}
_lmc_expand_buffer
sleep 0.05
$EVENT_HANDLER $_LMC_EVENT_FD
print -r -- "PARTIAL=$BUFFER"
print -r -- "PARTIAL_IN_FLIGHT=$_LMC_IN_FLIGHT"
integer guard=0
while (( _LMC_IN_FLIGHT )) && (( guard < 40 )); do
  sleep 0.05
  $EVENT_HANDLER $_LMC_EVENT_FD
  (( guard++ ))
done
print -r -- "FINAL=$BUFFER"
"#
    );

    let output = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("PARTIAL=show git status"),
        "stdout was: {stdout}"
    );
    assert!(
        stdout.contains("PARTIAL_IN_FLIGHT=1"),
        "stdout was: {stdout}"
    );
    assert!(stdout.contains("FINAL=git status"), "stdout was: {stdout}");
}

#[test]
fn zsh_widget_keeps_original_buffer_until_done_event() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));
    let temp_dir = tempfile::tempdir().unwrap();
    let bin_dir = temp_dir.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_lmc(
        &bin_dir,
        "#!/bin/zsh\nprint -r -- $'preview\tcommand=git'\nsleep 0.2\nprint -r -- $'done\tstatus=ok\twarning=none\tcommand=git status'\n",
    );

    let path = format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap());
    let script = format!(
        r#"
export PATH="{path}"
export OPENROUTER_API_KEY="test-key"
EVENT_HANDLER=""
zle() {{
  case "$1" in
    -F)
      if [[ "$2" == "-w" ]]; then
        EVENT_HANDLER="_lmc_handle_expand_event"
      else
        EVENT_HANDLER="$3"
      fi
      ;;
    -M|-R|-N|redisplay|expand-or-complete)
      ;;
  esac
}}
bindkey() {{ :; }}
source "{widget_path}"
BUFFER="show git status"
CURSOR=${{#BUFFER}}
_lmc_expand_buffer
sleep 0.05
$EVENT_HANDLER $_LMC_EVENT_FD
print -r -- "PARTIAL=$BUFFER"
integer guard=0
while (( _LMC_IN_FLIGHT )) && (( guard < 40 )); do
  sleep 0.05
  $EVENT_HANDLER $_LMC_EVENT_FD
  (( guard++ ))
done
print -r -- "FINAL=$BUFFER"
"#
    );

    let output = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("PARTIAL=show git status"),
        "stdout was: {stdout}"
    );
    assert!(stdout.contains("FINAL=git status"), "stdout was: {stdout}");
}

#[test]
fn zsh_widget_cancels_stream_on_user_input_and_keeps_original_buffer() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));
    let temp_dir = tempfile::tempdir().unwrap();
    let bin_dir = temp_dir.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_lmc(
        &bin_dir,
        "#!/bin/zsh\nprint -r -- $'preview\tcommand=git'\nsleep 1\nprint -r -- $'done\tstatus=ok\twarning=none\tcommand=git status'\n",
    );

    let path = format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap());
    let script = format!(
        r#"
export PATH="{path}"
export OPENROUTER_API_KEY="test-key"
EVENT_HANDLER=""
zle() {{
  case "$1" in
    -F)
      if [[ "$2" == "-w" ]]; then
        EVENT_HANDLER="_lmc_handle_expand_event"
      else
        EVENT_HANDLER="$3"
      fi
      ;;
    -M|-R|-N|redisplay|expand-or-complete)
      ;;
  esac
}}
bindkey() {{ :; }}
source "{widget_path}"
BUFFER="show git status"
CURSOR=${{#BUFFER}}
_lmc_expand_buffer
sleep 0.05
$EVENT_HANDLER $_LMC_EVENT_FD
LASTWIDGET="self-insert"
_lmc_line_pre_redraw
print -r -- "BUFFER=$BUFFER"
print -r -- "IN_FLIGHT=$_LMC_IN_FLIGHT"
"#
    );

    let output = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("BUFFER=show git status"),
        "stdout was: {stdout}"
    );
    assert!(stdout.contains("IN_FLIGHT=0"), "stdout was: {stdout}");
}

#[test]
fn zsh_widget_cancel_stream_does_not_call_missing_redraw_helper() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));

    let script = format!(
        r#"
zle() {{
  case "$1" in
    -M|-R|-I|-N|-F|redisplay|expand-or-complete)
      ;;
  esac
}}
bindkey() {{ :; }}
source "{widget_path}"
BUFFER="git clone --depth 1 https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git"
CURSOR=${{#BUFFER}}
_LMC_ORIGINAL_BUFFER="$BUFFER"
_LMC_IN_FLIGHT=1
_LMC_COMMAND_PID=-1
_lmc_cancel_stream
print -r -- "BUFFER=$BUFFER"
print -r -- "IN_FLIGHT=$_LMC_IN_FLIGHT"
"#
    );

    let output = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("BUFFER=git clone --depth 1 https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git"),
        "stdout was: {stdout}"
    );
    assert!(stdout.contains("IN_FLIGHT=0"), "stdout was: {stdout}");
    assert!(
        !stderr.contains("command not found"),
        "stderr was: {stderr}"
    );
}

#[test]
fn zsh_widget_completes_expand_in_noninteractive_harness() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));
    let temp_dir = tempfile::tempdir().unwrap();
    let bin_dir = temp_dir.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    write_fake_lmc(
        &bin_dir,
        "#!/bin/zsh\nsleep 0.1\nprint -r -- $'done\tstatus=ok\twarning=none\tcommand=git status'\n",
    );

    let path = format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap());
    let script = format!(
        r#"
export PATH="{path}"
export OPENROUTER_API_KEY="test-key"
EVENT_HANDLER=""
zle() {{
  case "$1" in
    -F)
      if [[ "$2" == "-w" ]]; then
        EVENT_HANDLER="_lmc_handle_expand_event"
      else
        EVENT_HANDLER="$3"
      fi
      ;;
    -M|-R|-N|redisplay|expand-or-complete)
      ;;
  esac
}}
bindkey() {{ :; }}
source "{widget_path}"
BUFFER="show git status"
CURSOR=${{#BUFFER}}
_lmc_expand_buffer
integer guard=0
while (( _LMC_IN_FLIGHT )) && (( guard < 60 )); do
  sleep 0.05
  $EVENT_HANDLER $_LMC_EVENT_FD
  (( guard++ ))
done
print -r -- "BUFFER=$BUFFER"
print -r -- "IN_FLIGHT=$_LMC_IN_FLIGHT"
print -r -- "GUARD=$guard"
"#
    );

    let output = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("BUFFER=git status"), "stdout was: {stdout}");
    assert!(stdout.contains("IN_FLIGHT=0"), "stdout was: {stdout}");
}

#[test]
fn zsh_widget_drains_done_event_when_tick_and_done_arrive_together() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));

    let script = format!(
        r#"
LAST_MESSAGE=""
REDRAWS=0
zle() {{
  case "$1" in
    -M|-R|-N)
      ;;
    -F)
      ;;
    redisplay|expand-or-complete)
      ;;
  esac
}}
bindkey() {{ :; }}
source "{widget_path}"
BUFFER="show git status"
CURSOR=${{#BUFFER}}
_LMC_ORIGINAL_BUFFER="$BUFFER"
_LMC_IN_FLIGHT=1
_LMC_OUTPUT_FILE="$(mktemp)"
_LMC_ERROR_FILE="$(mktemp)"
print -r -- "git status" > "$_LMC_OUTPUT_FILE"
exec {{fd}}< <(printf 'tick\t..\ndone\t0\n')
_LMC_EVENT_FD=$fd
_lmc_handle_expand_event "$fd"
print -r -- "BUFFER=$BUFFER"
print -r -- "IN_FLIGHT=$_LMC_IN_FLIGHT"
"#
    );

    let output = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("BUFFER=git status"), "stdout was: {stdout}");
    assert!(stdout.contains("IN_FLIGHT=0"), "stdout was: {stdout}");
}

#[test]
fn zsh_widget_finishes_on_event_stream_eof_after_command_exit() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));

    let script = format!(
        r#"
zle() {{
  case "$1" in
    -M|-R|-N)
      ;;
    -F)
      ;;
    redisplay|expand-or-complete)
      ;;
  esac
}}
bindkey() {{ :; }}
source "{widget_path}"
BUFFER="show git status"
CURSOR=${{#BUFFER}}
_LMC_ORIGINAL_BUFFER="$BUFFER"
_LMC_IN_FLIGHT=1
_LMC_OUTPUT_FILE="$(mktemp)"
_LMC_ERROR_FILE="$(mktemp)"
print -r -- "git status" > "$_LMC_OUTPUT_FILE"
sleep 0.01 &
_LMC_COMMAND_PID=$!
sleep 0.05
exec {{fd}}< <(:)
_LMC_EVENT_FD=$fd
_lmc_handle_expand_event "$fd"
print -r -- "BUFFER=$BUFFER"
print -r -- "IN_FLIGHT=$_LMC_IN_FLIGHT"
"#
    );

    let output = ProcessCommand::new("zsh")
        .args(["-fc", &script])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("BUFFER=git status"), "stdout was: {stdout}");
    assert!(stdout.contains("IN_FLIGHT=0"), "stdout was: {stdout}");
}

fn write_fake_lmc(bin_dir: &Path, script: &str) {
    let lmc_path = bin_dir.join("lmc");
    fs::write(&lmc_path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&lmc_path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

fn write_test_config(dir: &Path, base_url: &str, streaming_enabled: bool) -> PathBuf {
    write_test_config_with_model(dir, base_url, streaming_enabled, "test-model")
}

fn write_test_config_with_model(
    dir: &Path,
    base_url: &str,
    streaming_enabled: bool,
    model: &str,
) -> PathBuf {
    let config_path = dir.join("config.toml");
    fs::write(
        &config_path,
        format!(
            r#"
[provider]
name = "openrouter"
api_key = "test-key"
model = "{model}"
base_url = "{base_url}"

[streaming]
enabled = {streaming_enabled}
"#
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    config_path
}

fn spawn_streaming_server(chunks: Vec<(String, Duration)>) -> (String, thread::JoinHandle<()>) {
    spawn_streaming_server_with_capture(chunks, Arc::new(Mutex::new(String::new())))
}

fn spawn_streaming_server_with_capture(
    chunks: Vec<(String, Duration)>,
    captured_request: Arc<Mutex<String>>,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request_headers(&mut stream);
        *captured_request.lock().unwrap() = request;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        stream.flush().unwrap();

        for (chunk, delay) in chunks {
            thread::sleep(delay);
            write_chunk(&mut stream, chunk.as_bytes());
        }

        write!(stream, "0\r\n\r\n").unwrap();
        stream.flush().unwrap();
    });
    (address, handle)
}

fn spawn_json_server(body: String, delay: Duration) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        read_request_headers(&mut stream);
        thread::sleep(delay);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
        stream.flush().unwrap();
    });
    (address, handle)
}

fn read_request_headers(stream: &mut std::net::TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0u8; 1024];
    let mut header_end = None;

    while header_end.is_none() {
        let read = stream.read(&mut buffer).unwrap();
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
        header_end = request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|index| index + 4);
    }

    let Some(header_end) = header_end else {
        return String::new();
    };

    let headers = String::from_utf8_lossy(&request[..header_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            let lower = line.to_ascii_lowercase();
            lower
                .strip_prefix("content-length:")
                .and_then(|value| value.trim().parse::<usize>().ok())
        })
        .unwrap_or_default();

    while request.len().saturating_sub(header_end) < content_length {
        let read = stream.read(&mut buffer).unwrap();
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
    }

    String::from_utf8_lossy(&request).into_owned()
}

fn write_chunk(stream: &mut std::net::TcpStream, body: &[u8]) {
    write!(stream, "{:X}\r\n", body.len()).unwrap();
    stream.write_all(body).unwrap();
    write!(stream, "\r\n").unwrap();
    stream.flush().unwrap();
}

fn run_scripted_zsh_session(zshrc: &str, input_steps: Vec<(Duration, Vec<u8>)>) -> String {
    let temp_dir = tempfile::tempdir().unwrap();
    let zshrc_path = temp_dir.path().join(".zshrc");
    let log_path = temp_dir.path().join("typescript");
    fs::write(&zshrc_path, zshrc).unwrap();

    let mut child = ProcessCommand::new("/usr/bin/script")
        .args(["-q", log_path.to_str().unwrap(), "zsh", "-i"])
        .env("HOME", temp_dir.path())
        .env("ZDOTDIR", temp_dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let mut stdin = child.stdin.take().unwrap();
    let writer = thread::spawn(move || {
        for (delay, bytes) in input_steps {
            thread::sleep(delay);
            stdin.write_all(&bytes).unwrap();
            stdin.flush().unwrap();
        }
    });

    writer.join().unwrap();
    let status = child.wait().unwrap();
    assert!(status.success(), "script exited with {status}");

    let transcript = fs::read(&log_path).unwrap();
    normalize_terminal_transcript(&transcript)
}

fn normalize_terminal_transcript(transcript: &[u8]) -> String {
    let mut output = String::new();
    let mut index = 0;

    while index < transcript.len() {
        match transcript[index] {
            0x1b => {
                index += 1;
                if index < transcript.len() && transcript[index] == b'[' {
                    index += 1;
                    while index < transcript.len() {
                        let byte = transcript[index];
                        index += 1;
                        if (0x40..=0x7e).contains(&byte) {
                            break;
                        }
                    }
                }
            }
            b'\r' => {
                output.push('\n');
                index += 1;
            }
            0x08 => {
                output.pop();
                index += 1;
            }
            0x07 => {
                index += 1;
            }
            byte if byte < 0x20 && byte != b'\n' && byte != b'\t' => {
                index += 1;
            }
            byte => {
                output.push(byte as char);
                index += 1;
            }
        }
    }

    output
}
