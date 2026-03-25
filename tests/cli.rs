use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn init_zsh_prints_widget() {
    Command::cargo_bin("lmc")
        .unwrap()
        .args(["init", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("lmc-expand-buffer"));
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
