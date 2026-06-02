//! Optional commit-message synthesis via [apfel](https://github.com/Arthur-Ficial/apfel),
//! a CLI wrapper around macOS's on-device LLM. When apfel is on PATH, tend-ship
//! feeds it the working-tree diff and asks for a one-line subject. The result
//! is offered as the primary candidate; Claude-authored `git commit -m`
//! messages from the transcript follow it (still reachable via `[p]`).
//!
//! This is a best-effort enhancement. Any failure — apfel not installed,
//! spawn error, non-zero exit, empty diff, garbage output — returns `None`
//! and tend-ship falls through to its transcript-based candidates exactly as
//! it did before apfel existed.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

const PROMPT: &str = "Write a single-line git commit subject for the diff that follows. \
Use imperative mood. Keep it under 70 characters. No quotes, no preamble, no \
explanation, no leading bracket prefix. Reply with only the subject text.";

/// Synthesize a commit subject from `target`'s working-tree diff, or `None`.
///
/// Set `TEND_SHIP_NO_APFEL=1` to force-disable apfel even when it's installed —
/// useful for tests, CI, or anyone who'd rather always use Claude's transcript
/// messages.
pub fn synthesize(target: &Path) -> Option<String> {
    if std::env::var_os("TEND_SHIP_NO_APFEL").is_some() {
        return None;
    }
    let diff = git_diff(target)?;
    if diff.trim().is_empty() {
        return None;
    }
    run_apfel(&diff)
}

/// `git diff HEAD --no-color` against the target's working tree, capturing
/// every uncommitted change. Returns `None` if git fails for any reason.
fn git_diff(target: &Path) -> Option<String> {
    let out = Command::new("git")
        .current_dir(target)
        .args(["diff", "HEAD", "--no-color"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

/// Pipe `<PROMPT>\n\n<diff>` to apfel on stdin, take the first non-empty
/// line of stdout. Returns `None` on any error (including apfel-not-on-PATH).
fn run_apfel(diff: &str) -> Option<String> {
    let mut child = Command::new("apfel")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let input = format!("{PROMPT}\n\n{diff}");
    child.stdin.as_mut()?.write_all(input.as_bytes()).ok()?;
    drop(child.stdin.take());
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_subject(&String::from_utf8_lossy(&output.stdout))
}

/// First non-empty line of `raw`, with surrounding quote-like wrappers
/// stripped. Defensive against models that wrap their output in `"..."`.
fn parse_subject(raw: &str) -> Option<String> {
    let line = raw.lines().map(str::trim).find(|l| !l.is_empty())?;
    let cleaned = line.trim_matches(|c: char| c == '"' || c == '\'' || c == '`');
    (!cleaned.is_empty()).then(|| cleaned.to_string())
}

#[cfg(test)]
mod tests {
    use super::parse_subject;

    #[test]
    fn parses_bare_subject_line() {
        assert_eq!(
            parse_subject("Rename meta comment in README\n"),
            Some("Rename meta comment in README".to_string()),
        );
    }

    #[test]
    fn skips_leading_blank_lines() {
        assert_eq!(
            parse_subject("\n\nFix off-by-one in foo\n"),
            Some("Fix off-by-one in foo".to_string()),
        );
    }

    #[test]
    fn strips_surrounding_quotes() {
        assert_eq!(
            parse_subject("\"Add apfel synthesis\"\n"),
            Some("Add apfel synthesis".to_string()),
        );
        assert_eq!(
            parse_subject("'tighten loop'"),
            Some("tighten loop".to_string()),
        );
        assert_eq!(
            parse_subject("`docs tweak`"),
            Some("docs tweak".to_string()),
        );
    }

    #[test]
    fn returns_none_for_empty_output() {
        assert!(parse_subject("").is_none());
        assert!(parse_subject("\n\n\n").is_none());
    }

    #[test]
    fn returns_none_when_only_wrapping_chars() {
        assert!(parse_subject("\"\"").is_none());
        assert!(parse_subject("''").is_none());
    }
}
