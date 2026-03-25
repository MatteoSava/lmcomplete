use assert_cmd::Command;
use predicates::prelude::*;
use std::process::Command as ProcessCommand;

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
fn zsh_widget_finish_handles_success_without_reserved_status_variable() {
    let widget_path = format!(
        "{}/src/shell/zsh_widget.zsh",
        env!("CARGO_MANIFEST_DIR")
    );

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
fn zsh_widget_shows_loading_message_during_expand() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));
    let temp_dir = tempfile::tempdir().unwrap();
    let bin_dir = temp_dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let lmc_path = bin_dir.join("lmc");
    std::fs::write(
        &lmc_path,
        "#!/bin/zsh\nsleep 0.4\nprint -r -- 'git status'\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&lmc_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let path = format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap());
    let script = format!(
        r#"
export PATH="{path}"
export OPENROUTER_API_KEY="test-key"
LAST_MESSAGE=""
EVENT_HANDLER=""
zle() {{
  case "$1" in
    -M)
      if [[ "$2" == "--" ]]; then
        LAST_MESSAGE="$3"
      else
        LAST_MESSAGE="$2"
      fi
      ;;
    -F)
      if [[ "$2" == "-w" ]]; then
        EVENT_HANDLER="_lmc_handle_expand_event"
      else
        EVENT_HANDLER="$3"
      fi
      ;;
    -R|-N|redisplay|expand-or-complete)
      ;;
  esac
}}
bindkey() {{ :; }}
source "{widget_path}"
BUFFER="show git status"
CURSOR=${{#BUFFER}}
_lmc_expand_buffer
sleep 0.2
$EVENT_HANDLER $_LMC_EVENT_FD
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
    assert!(
        stdout.contains("MESSAGE=[lmc "),
        "stdout was: {stdout}"
    );
}

#[test]
fn zsh_widget_completes_expand_in_noninteractive_harness() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));
    let temp_dir = tempfile::tempdir().unwrap();
    let bin_dir = temp_dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let lmc_path = bin_dir.join("lmc");
    std::fs::write(
        &lmc_path,
        "#!/bin/zsh\nsleep 0.1\nprint -r -- 'git status'\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&lmc_path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

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
