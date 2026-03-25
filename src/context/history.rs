use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use anyhow::Result;
use directories::BaseDirs;

use crate::context::shell::Shell;

pub fn recent_commands(shell: Shell, limit: usize) -> Result<Vec<String>> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let Some(path) = history_path(shell) else {
        return Ok(Vec::new());
    };

    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    recent_commands_from_reader(shell, limit, reader)
}

fn history_path(shell: Shell) -> Option<PathBuf> {
    let home = BaseDirs::new()?.home_dir().to_path_buf();
    match shell {
        Shell::Zsh => Some(home.join(".zsh_history")),
        Shell::Bash | Shell::Sh => Some(home.join(".bash_history")),
        Shell::Fish => Some(home.join(".local/share/fish/fish_history")),
    }
}

fn parse_history_line(shell: Shell, line: &str) -> Option<String> {
    match shell {
        Shell::Zsh => parse_zsh_line(line),
        Shell::Fish => parse_fish_line(line),
        Shell::Bash | Shell::Sh => Some(line.to_string()),
    }
}

fn parse_zsh_line(line: &str) -> Option<String> {
    if let Some(rest) = line.strip_prefix(": ") {
        return rest.split_once(';').map(|(_, command)| command.to_string());
    }
    Some(line.to_string())
}

fn parse_fish_line(line: &str) -> Option<String> {
    line.trim_start()
        .strip_prefix("- cmd: ")
        .map(|value| value.to_string())
}

fn recent_commands_from_reader<R: BufRead>(
    shell: Shell,
    limit: usize,
    reader: R,
) -> Result<Vec<String>> {
    let mut commands = VecDeque::with_capacity(limit);
    let mut reader = reader;
    let mut buffer = Vec::new();

    loop {
        buffer.clear();
        if reader.read_until(b'\n', &mut buffer)? == 0 {
            break;
        }

        let line = String::from_utf8_lossy(&buffer);
        let line = line.trim_end_matches(['\n', '\r']);
        if line.trim().trim_matches('\u{fffd}').is_empty() {
            continue;
        }
        let Some(command) = parse_history_line(shell, line) else {
            continue;
        };
        if command.trim().is_empty() {
            continue;
        }

        if commands.len() == limit {
            commands.pop_front();
        }
        commands.push_back(command);
    }

    Ok(commands.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::context::shell::Shell;

    use super::{parse_fish_line, parse_zsh_line, recent_commands_from_reader};

    #[test]
    fn parses_zsh_history_lines() {
        let parsed = parse_zsh_line(": 1710000000:0;git status").unwrap();
        assert_eq!(parsed, "git status");
    }

    #[test]
    fn parses_fish_history_lines() {
        let parsed = parse_fish_line("- cmd: cargo test").unwrap();
        assert_eq!(parsed, "cargo test");
    }

    #[test]
    fn tolerates_non_utf8_history_bytes() {
        let history = b": 1710000000:0;git status\n\xff\n: 1710000001:0;git diff\n";
        let reader = Cursor::new(history);

        let commands = recent_commands_from_reader(Shell::Zsh, 10, reader).unwrap();

        assert_eq!(commands, vec!["git status", "git diff"]);
    }
}
