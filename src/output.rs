//! Streaming safety scaffolding for a future production TTY path.
//!
//! Currently `#[cfg(test)]` gated in `lib.rs` — production uses
//! `safety::preview_expand_output` instead. Destructive patterns are consumed
//! from `safety::DESTRUCTIVE_PATTERNS`.

use std::io::Write;

use anyhow::{Result, bail};

use crate::safety;

const WARNING_LINE: &str = "# WARNING: destructive command";

#[derive(Debug, Default)]
pub struct TrimmedOutput {
    rendered: String,
    started: bool,
    pending_whitespace: String,
}

impl TrimmedOutput {
    pub fn push(&mut self, chunk: &str, sink: &mut impl Write) -> Result<()> {
        for ch in chunk.chars() {
            if ch.is_whitespace() {
                if self.started {
                    self.pending_whitespace.push(ch);
                }
                continue;
            }

            self.started = true;
            self.flush_pending_whitespace(sink)?;
            self.emit_char(ch, sink)?;
        }

        Ok(())
    }

    pub fn finish(&mut self, sink: &mut impl Write) -> Result<&str> {
        if !self.rendered.is_empty() {
            sink.write_all(b"\n")?;
            sink.flush()?;
        }
        Ok(&self.rendered)
    }

    fn flush_pending_whitespace(&mut self, sink: &mut impl Write) -> Result<()> {
        if !self.pending_whitespace.is_empty() {
            let whitespace = std::mem::take(&mut self.pending_whitespace);
            self.emit(&whitespace, sink)?;
        }
        Ok(())
    }

    fn emit_char(&mut self, ch: char, sink: &mut impl Write) -> Result<()> {
        let mut buffer = [0_u8; 4];
        self.emit(ch.encode_utf8(&mut buffer), sink)
    }

    fn emit(&mut self, text: &str, sink: &mut impl Write) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        sink.write_all(text.as_bytes())?;
        sink.flush()?;
        self.rendered.push_str(text);
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct ExpandOutput {
    phase: ExpandPhase,
    pending_warning: bool,
    warning_emitted: bool,
    rendered: String,
    body: CommandBodyRenderer,
    pending_body_output: String,
    safety: SafetyState,
}

impl ExpandOutput {
    pub fn push(&mut self, chunk: &str, sink: &mut impl Write) -> Result<()> {
        for ch in chunk.chars() {
            match &mut self.phase {
                ExpandPhase::InspectingFirstLine(buffer) => {
                    if ch == '\n' {
                        let line = buffer.trim();
                        if line == WARNING_LINE {
                            self.pending_warning = true;
                            self.phase = ExpandPhase::Body;
                        } else {
                            let buffered = std::mem::take(buffer);
                            self.phase = ExpandPhase::Body;
                            self.push_body(&buffered, sink)?;
                            self.push_body("\n", sink)?;
                        }
                        continue;
                    }

                    buffer.push(ch);
                    if !warning_prefix_possible(buffer) {
                        let buffered = std::mem::take(buffer);
                        self.phase = ExpandPhase::Body;
                        self.push_body(&buffered, sink)?;
                    }
                }
                ExpandPhase::Body => self.push_body_char(ch, sink)?,
            }
        }

        Ok(())
    }

    pub fn finish(&mut self, sink: &mut impl Write) -> Result<&str> {
        if let ExpandPhase::InspectingFirstLine(buffer) = &mut self.phase {
            if !buffer.is_empty() {
                let line = std::mem::take(buffer);
                if line.trim() == WARNING_LINE {
                    self.pending_warning = true;
                } else {
                    self.phase = ExpandPhase::Body;
                    self.push_body(&line, sink)?;
                }
            } else {
                self.phase = ExpandPhase::Body;
            }
        }

        self.body.finish();
        self.flush_pending_body_if_resolved(sink, true)?;

        if !self.rendered.is_empty() {
            sink.write_all(b"\n")?;
            sink.flush()?;
        }

        Ok(&self.rendered)
    }

    fn push_body(&mut self, text: &str, sink: &mut impl Write) -> Result<()> {
        for ch in text.chars() {
            self.push_body_char(ch, sink)?;
        }
        Ok(())
    }

    fn push_body_char(&mut self, ch: char, sink: &mut impl Write) -> Result<()> {
        if self.safety.is_buffering() {
            self.body.push_char(ch, &mut self.pending_body_output)?;
            self.flush_pending_body_if_resolved(sink, false)?;
            return Ok(());
        }

        let mut buffer = String::new();
        self.body.push_char(ch, &mut buffer)?;
        self.emit_body(&buffer, sink)
    }

    fn flush_pending_body_if_resolved(
        &mut self,
        sink: &mut impl Write,
        is_eof: bool,
    ) -> Result<()> {
        if !self.safety.is_buffering() {
            return Ok(());
        }

        let decision = destructive_prefix_status(&self.pending_body_output, is_eof);
        if decision == PrefixDecision::Pending {
            return Ok(());
        }

        self.safety = match decision {
            PrefixDecision::Safe => SafetyState::Streaming,
            PrefixDecision::Destructive => SafetyState::Streaming,
            PrefixDecision::Pending => unreachable!(),
        };

        if decision == PrefixDecision::Destructive {
            self.pending_warning = true;
        }

        let buffered = std::mem::take(&mut self.pending_body_output);
        self.emit_body(&buffered, sink)
    }

    fn emit_body(&mut self, text: &str, sink: &mut impl Write) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        if self.pending_warning && !self.warning_emitted {
            self.emit_text(WARNING_LINE, sink)?;
            self.emit_text("\n", sink)?;
            self.warning_emitted = true;
            self.pending_warning = false;
        }

        self.emit_text(text, sink)
    }

    fn emit_text(&mut self, text: &str, sink: &mut impl Write) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        sink.write_all(text.as_bytes())?;
        sink.flush()?;
        self.rendered.push_str(text);
        Ok(())
    }
}

#[derive(Debug)]
enum ExpandPhase {
    InspectingFirstLine(String),
    Body,
}

impl Default for ExpandPhase {
    fn default() -> Self {
        Self::InspectingFirstLine(String::new())
    }
}

#[derive(Debug, Default)]
struct CommandBodyRenderer {
    emitted_anything: bool,
    current_line_has_content: bool,
    pending_whitespace: String,
    pending_line_join: bool,
}

impl CommandBodyRenderer {
    fn push_char(&mut self, ch: char, output: &mut String) -> Result<()> {
        match ch {
            '\r' => {}
            '\n' => {
                self.pending_whitespace.clear();
                if self.current_line_has_content {
                    self.current_line_has_content = false;
                    self.pending_line_join = true;
                }
            }
            '\t' | ' ' => {
                if self.current_line_has_content {
                    self.pending_whitespace.push(' ');
                }
            }
            _ => {
                if !self.current_line_has_content {
                    if self.pending_line_join && self.emitted_anything {
                        output.push(' ');
                    }
                    self.current_line_has_content = true;
                    self.pending_line_join = false;
                } else if !self.pending_whitespace.is_empty() {
                    output.push_str(&self.pending_whitespace);
                    self.pending_whitespace.clear();
                }

                output.push(ch);
                self.emitted_anything = true;
            }
        }

        Ok(())
    }

    fn finish(&mut self) {
        self.pending_whitespace.clear();
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum SafetyState {
    Buffering,
    Streaming,
}

impl Default for SafetyState {
    fn default() -> Self {
        Self::Buffering
    }
}

impl SafetyState {
    fn is_buffering(self) -> bool {
        matches!(self, Self::Buffering)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum PrefixDecision {
    Pending,
    Safe,
    Destructive,
}

fn warning_prefix_possible(buffer: &str) -> bool {
    let candidate = buffer.trim_start();
    candidate.is_empty() || WARNING_LINE.starts_with(candidate)
}

fn destructive_prefix_status(body: &str, is_eof: bool) -> PrefixDecision {
    let compact = collapse_whitespace(body);
    if compact.is_empty() {
        return eof_or_pending(is_eof);
    }

    let first = compact.split(' ').next().unwrap_or_default();
    if matches_partial_root(first) {
        return eof_or_pending(is_eof);
    }

    for pattern in safety::DESTRUCTIVE_PATTERNS {
        if first != pattern.root {
            continue;
        }

        return match (pattern.phrase, pattern.requires_flag) {
            (Some(phrase), _) => phrase_prefix_status(&compact, phrase, is_eof),
            (_, Some(_)) => git_push_force_status(&compact, is_eof),
            _ => rm_status(&compact, is_eof),
        };
    }

    PrefixDecision::Safe
}

fn matches_partial_root(first: &str) -> bool {
    safety::DESTRUCTIVE_ROOTS
        .iter()
        .any(|root| root.starts_with(first) && *root != first)
}

fn eof_or_pending(is_eof: bool) -> PrefixDecision {
    if is_eof {
        PrefixDecision::Safe
    } else {
        PrefixDecision::Pending
    }
}

fn rm_status(text: &str, is_eof: bool) -> PrefixDecision {
    let Some(rest) = text.strip_prefix("rm") else {
        return PrefixDecision::Safe;
    };

    if rest.is_empty() {
        return if is_eof {
            PrefixDecision::Destructive
        } else {
            PrefixDecision::Pending
        };
    }

    if starts_with_non_word(rest) {
        PrefixDecision::Destructive
    } else {
        PrefixDecision::Safe
    }
}

fn phrase_prefix_status(text: &str, phrase: &str, is_eof: bool) -> PrefixDecision {
    if text == phrase {
        return PrefixDecision::Destructive;
    }

    if phrase.starts_with(text) {
        return if is_eof {
            PrefixDecision::Safe
        } else {
            PrefixDecision::Pending
        };
    }

    if let Some(next) = text.strip_prefix(phrase) {
        return if next.is_empty() || starts_with_non_word(next) {
            PrefixDecision::Destructive
        } else {
            PrefixDecision::Safe
        };
    }

    PrefixDecision::Safe
}

fn git_push_force_status(text: &str, is_eof: bool) -> PrefixDecision {
    let phrase = "git push";
    if phrase.starts_with(text) {
        return if is_eof {
            PrefixDecision::Safe
        } else {
            PrefixDecision::Pending
        };
    }

    let Some(rest) = text.strip_prefix(phrase) else {
        return PrefixDecision::Safe;
    };

    if !rest.is_empty() && !starts_with_non_word(rest) {
        return PrefixDecision::Safe;
    }

    if contains_force_flag(text) {
        return PrefixDecision::Destructive;
    }

    if is_eof {
        PrefixDecision::Safe
    } else {
        PrefixDecision::Pending
    }
}

fn contains_force_flag(text: &str) -> bool {
    for candidate in ["--force", "--force-with-lease"] {
        if let Some(index) = text.find(candidate) {
            let next = text[index + candidate.len()..].chars().next();
            if next.is_none() || next.is_some_and(|value| !value.is_alphanumeric() && value != '_')
            {
                return true;
            }
        }
    }

    false
}

fn starts_with_non_word(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|value| !value.is_alphanumeric() && value != '_')
}

fn collapse_whitespace(text: &str) -> String {
    let mut compact = String::new();
    let mut pending_space = false;

    for ch in text.chars() {
        if ch.is_whitespace() {
            if !compact.is_empty() {
                pending_space = true;
            }
            continue;
        }

        if pending_space {
            compact.push(' ');
            pending_space = false;
        }
        compact.push(ch);
    }

    compact
}

pub fn verify_expand_output(rendered: &str, expected: &str) -> Result<()> {
    if rendered == expected {
        return Ok(());
    }

    bail!("streamed expand output diverged from normalized output")
}

#[cfg(test)]
mod tests {
    use super::{ExpandOutput, TrimmedOutput, verify_expand_output};

    #[test]
    fn trimmed_output_streams_without_edge_whitespace() {
        let mut output = TrimmedOutput::default();
        let mut sink = Vec::new();

        output.push("  hello", &mut sink).unwrap();
        output.push(" world  ", &mut sink).unwrap();
        let rendered = output.finish(&mut sink).unwrap();

        assert_eq!(rendered, "hello world");
        assert_eq!(String::from_utf8(sink).unwrap(), "hello world\n");
    }

    #[test]
    fn expand_output_streams_safe_command() {
        let mut output = ExpandOutput::default();
        let mut sink = Vec::new();

        output.push("  ls ", &mut sink).unwrap();
        output.push("-la  ", &mut sink).unwrap();
        let rendered = output.finish(&mut sink).unwrap();

        assert_eq!(rendered, "ls -la");
        assert_eq!(String::from_utf8(sink).unwrap(), "ls -la\n");
    }

    #[test]
    fn expand_output_preserves_explicit_warning() {
        let mut output = ExpandOutput::default();
        let mut sink = Vec::new();

        output
            .push("# WARNING: destructive command\n\n rm -rf tmp", &mut sink)
            .unwrap();
        let rendered = output.finish(&mut sink).unwrap();

        assert_eq!(rendered, "# WARNING: destructive command\nrm -rf tmp");
        assert_eq!(
            String::from_utf8(sink).unwrap(),
            "# WARNING: destructive command\nrm -rf tmp\n"
        );
    }

    #[test]
    fn expand_output_adds_warning_for_destructive_command() {
        let mut output = ExpandOutput::default();
        let mut sink = Vec::new();

        output.push("rm -", &mut sink).unwrap();
        output.push("rf tmp", &mut sink).unwrap();
        let rendered = output.finish(&mut sink).unwrap();

        assert_eq!(rendered, "# WARNING: destructive command\nrm -rf tmp");
        assert_eq!(
            String::from_utf8(sink).unwrap(),
            "# WARNING: destructive command\nrm -rf tmp\n"
        );
    }

    #[test]
    fn expand_output_adds_warning_for_plain_file_delete_command() {
        let mut output = ExpandOutput::default();
        let mut sink = Vec::new();

        output.push("rm no", &mut sink).unwrap();
        output.push("tes.txt", &mut sink).unwrap();
        let rendered = output.finish(&mut sink).unwrap();

        assert_eq!(rendered, "# WARNING: destructive command\nrm notes.txt");
        assert_eq!(
            String::from_utf8(sink).unwrap(),
            "# WARNING: destructive command\nrm notes.txt\n"
        );
    }

    #[test]
    fn expand_output_releases_safe_git_command_after_prefix_diverges() {
        let mut output = ExpandOutput::default();
        let mut sink = Vec::new();

        output.push("git st", &mut sink).unwrap();
        output.push("atus", &mut sink).unwrap();
        let rendered = output.finish(&mut sink).unwrap();

        assert_eq!(rendered, "git status");
        assert_eq!(String::from_utf8(sink).unwrap(), "git status\n");
    }

    #[test]
    fn verify_expand_output_detects_divergence() {
        let error = verify_expand_output("git status", "git status --short").unwrap_err();
        assert!(error.to_string().contains("diverged"));
    }
}
