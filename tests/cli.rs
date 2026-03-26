use assert_cmd::Command;
use predicates::prelude::*;
use std::io::{Read, Write};
use std::net::TcpListener;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::mpsc;
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
        stdout.contains("MESSAGE=Set OPENROUTER_API_KEY or configure a provider in "),
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
fn zsh_widget_loading_indicator_places_cursor_before_label() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));

    let script = format!(
        r#"
LAST_MESSAGE=""
zle() {{
  case "$1" in
    -M|-R|-N|redisplay|expand-or-complete)
      ;;
    -F)
      ;;
  esac
}}
bindkey() {{ :; }}
source "{widget_path}"
_lmc_show_loading_state "▍"
print -r -- "POSTDISPLAY=$POSTDISPLAY"
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
}

#[test]
fn zsh_widget_accepts_ollama_config_without_api_key() {
    let widget_path = format!("{}/src/shell/zsh_widget.zsh", env!("CARGO_MANIFEST_DIR"));
    let temp_dir = tempfile::tempdir().unwrap();
    let config_dir = temp_dir.path().join(".config/lmcomplete");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("config.toml"),
        r#"
[provider]
name = "ollama"
"#,
    )
    .unwrap();

    let home = temp_dir.path().display();
    let script = format!(
        r#"
export HOME="{home}"
export XDG_CONFIG_HOME="{home}/.config"
unset OPENROUTER_API_KEY
zle() {{ :; }}
bindkey() {{ :; }}
source "{widget_path}"
if _lmc_has_provider_config; then
  print -r -- "CONFIGURED=1"
else
  print -r -- "CONFIGURED=0"
fi
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
    assert!(stdout.contains("CONFIGURED=1"), "stdout was: {stdout}");
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
    assert!(stdout.contains("MESSAGE=[lmc "), "stdout was: {stdout}");
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

#[test]
fn explain_streams_stdout_before_completion() {
    let (base_url, server) = spawn_streaming_server(vec![
        (
            "data: {\"choices\":[{\"delta\":{\"content\":\"hello \"}}]}\n\n",
            Duration::from_millis(2_000),
        ),
        (
            "data: {\"choices\":[{\"delta\":{\"content\":\"world\"}}]}\n\n",
            Duration::ZERO,
        ),
        (
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":2,\"total_tokens\":3,\"cost\":0.01}}\n\n",
            Duration::ZERO,
        ),
        ("data: [DONE]\n\n", Duration::ZERO),
    ]);
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = write_test_config(temp_dir.path(), &base_url);

    let mut child = ProcessCommand::new(env!("CARGO_BIN_EXE_lmc"))
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "explain",
            "tar xzf archive.tar.gz",
            "--shell",
            "zsh",
            "--history",
            "0",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = thread::spawn(move || read_streamed_stdout(stdout, tx));

    let first_chunk = rx.recv_timeout(Duration::from_millis(1_500)).unwrap();
    assert_eq!(first_chunk, "hello");

    let status = child.wait().unwrap();
    assert!(status.success());

    let full_output = reader.join().unwrap();
    assert_eq!(full_output, "hello world\n");
    server.join().unwrap();
}

#[test]
fn expand_streams_stdout_before_completion() {
    let (base_url, server) = spawn_streaming_server(vec![
        (
            "data: {\"choices\":[{\"delta\":{\"content\":\"ls \"}}]}\n\n",
            Duration::from_millis(2_000),
        ),
        (
            "data: {\"choices\":[{\"delta\":{\"content\":\"-la\"}}]}\n\n",
            Duration::ZERO,
        ),
        (
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":2,\"total_tokens\":3,\"cost\":0.01}}\n\n",
            Duration::ZERO,
        ),
        ("data: [DONE]\n\n", Duration::ZERO),
    ]);
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = write_test_config(temp_dir.path(), &base_url);

    let mut child = ProcessCommand::new(env!("CARGO_BIN_EXE_lmc"))
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "expand",
            "list files",
            "--shell",
            "zsh",
            "--history",
            "0",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = thread::spawn(move || read_streamed_stdout(stdout, tx));

    let first_chunk = rx.recv_timeout(Duration::from_millis(1_500)).unwrap();
    assert_eq!(first_chunk, "ls");

    let status = child.wait().unwrap();
    assert!(status.success());

    let full_output = reader.join().unwrap();
    assert_eq!(full_output, "ls -la\n");
    server.join().unwrap();
}

fn read_streamed_stdout(mut stdout: impl Read, tx: mpsc::Sender<String>) -> String {
    let mut first_chunk = [0_u8; 64];
    let count = stdout.read(&mut first_chunk).unwrap();
    let first = String::from_utf8_lossy(&first_chunk[..count]).into_owned();
    tx.send(first.clone()).unwrap();

    let mut rest = String::new();
    stdout.read_to_string(&mut rest).unwrap();
    format!("{first}{rest}")
}

fn spawn_streaming_server(
    frames: Vec<(&'static str, Duration)>,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request).unwrap();
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        stream.flush().unwrap();

        for (frame, delay) in frames {
            stream.write_all(frame.as_bytes()).unwrap();
            stream.flush().unwrap();
            if !delay.is_zero() {
                thread::sleep(delay);
            }
        }
    });

    (format!("http://{address}/api/v1/chat/completions"), handle)
}

fn write_test_config(dir: &Path, base_url: &str) -> PathBuf {
    let config_path = dir.join("config.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"
[provider]
name = "openrouter"
api_key = "test-key"
model = "test-model"
base_url = "{base_url}"
"#
        ),
    )
    .unwrap();
    #[cfg(unix)]
    std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    config_path
}
