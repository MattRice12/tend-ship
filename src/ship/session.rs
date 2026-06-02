use std::path::{Path, PathBuf};

const COMMIT_NEEDLE: &str = r#"git commit -m ""#;

/// Locate the most recently-modified `*.jsonl` file under
/// `<claude_home>/projects/<encoded(cwd)>/`. Returns `None` if the
/// directory doesn't exist or contains no JSONL files.
pub fn newest_session_path(claude_home: &Path, cwd: &Path) -> Option<PathBuf> {
    let encoded = crate::encode::encode_path(cwd)?;
    let project_dir = claude_home.join("projects").join(encoded);

    let entries = std::fs::read_dir(&project_dir).ok()?;
    entries
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|s| s == "jsonl")
        })
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
        .map(|e| e.path())
}

/// All commit messages Claude has crafted in the session, in
/// reverse-chronological order (newest first).
///
/// A "crafted commit message" is the contents of a `git commit -m "<msg>"`
/// substring found inside an assistant text block. This is the literal
/// message Claude proposed in its response — no inference, no rewriting.
///
/// When `ticket` is `Some("CO-1234")`, candidates whose subject begins with
/// `[CO-1234]` are surfaced first (in newest-first order), then everything
/// else (also newest-first). This keeps multi-ticket sessions usable from a
/// single worktree: the auto-pick lands on a message that matches the branch,
/// and `[p]revious` still walks the rest if you want them.
pub fn candidates_reverse(jsonl: &str, ticket: Option<&str>) -> Vec<String> {
    let mut all = parse_assistant_texts(jsonl);
    all.reverse();
    let mut out = Vec::new();
    for text in all {
        out.extend(extract_commit_messages(&text));
    }
    let Some(t) = ticket else { return out };
    let needle = format!("[{t}]");
    let (matching, other): (Vec<_>, Vec<_>) =
        out.into_iter().partition(|m| m.starts_with(&needle));
    matching.into_iter().chain(other).collect()
}

/// Walk a JSONL transcript and return every assistant-message text, in
/// the order they appear in the file (oldest first). Lines that are not
/// valid JSON, not assistant records, or contain no text blocks are
/// silently skipped.
pub fn parse_assistant_texts(jsonl: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in jsonl.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if value.get("type").and_then(|v| v.as_str()) != Some("assistant") {
            continue;
        }
        let Some(content) = value
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
        else {
            continue;
        };
        let texts: Vec<&str> = content
            .iter()
            .filter(|block| block.get("type").and_then(|v| v.as_str()) == Some("text"))
            .filter_map(|block| block.get("text").and_then(|t| t.as_str()))
            .collect();
        if texts.is_empty() {
            continue;
        }
        out.push(texts.join(" "));
    }
    out
}

/// Extract every `git commit -m "<message>"` substring from `text`, in
/// document order. The user's convention is "single-line subject, no
/// body, no multi-`-m`, no HEREDOC", so this is a simple needle scan
/// terminating at the next unescaped `"`.
pub fn extract_commit_messages(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(COMMIT_NEEDLE) {
        let after = &rest[start + COMMIT_NEEDLE.len()..];
        match after.find('"') {
            Some(end) => {
                out.push(after[..end].to_string());
                rest = &after[end + 1..];
            }
            None => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_single_commit_message() {
        let text = r#"Done. Run this:

```bash
cd /repo && git add -A && git commit -m "[CO-1234] Fix the bug" && git push
```
"#;
        assert_eq!(
            extract_commit_messages(text),
            vec!["[CO-1234] Fix the bug"],
        );
    }

    #[test]
    fn extracts_multi_command_form() {
        let text = "cd /path &&\n  git add -A &&\n  git commit -m \"[CO-5528] Consolidate duplicate vendor rows\" &&\n  git push";
        assert_eq!(
            extract_commit_messages(text),
            vec!["[CO-5528] Consolidate duplicate vendor rows"],
        );
    }

    #[test]
    fn extracts_multiple_commit_messages_in_order() {
        let text = r#"First commit:
  git commit -m "[CO-1] first"
Then:
  git commit -m "[CO-2] second"
"#;
        assert_eq!(
            extract_commit_messages(text),
            vec!["[CO-1] first", "[CO-2] second"],
        );
    }

    #[test]
    fn extracts_no_messages_from_prose() {
        let text = "I just finished the work. The tests pass. Ready to ship.";
        assert!(extract_commit_messages(text).is_empty());
    }

    #[test]
    fn extracts_no_messages_from_empty() {
        assert!(extract_commit_messages("").is_empty());
    }

    #[test]
    fn extracts_when_message_contains_brackets_and_punctuation() {
        let text = r#"git commit -m "[CO-7] Add :foo support (closes #99)""#;
        assert_eq!(
            extract_commit_messages(text),
            vec!["[CO-7] Add :foo support (closes #99)"],
        );
    }

    #[test]
    fn parse_handles_empty_input() {
        assert!(parse_assistant_texts("").is_empty());
    }

    #[test]
    fn parse_skips_non_assistant_records() {
        let jsonl = r#"{"type":"user","message":{"content":[{"type":"text","text":"hello"}]}}"#;
        assert!(parse_assistant_texts(jsonl).is_empty());
    }

    #[test]
    fn parse_extracts_assistant_text() {
        let jsonl = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello world"}]}}"#;
        assert_eq!(parse_assistant_texts(jsonl), vec!["Hello world"]);
    }

    #[test]
    fn parse_skips_thinking_and_tool_use_blocks() {
        let jsonl = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"hidden"},{"type":"text","text":"visible"},{"type":"tool_use","name":"Bash","input":{}}]}}"#;
        assert_eq!(parse_assistant_texts(jsonl), vec!["visible"]);
    }

    #[test]
    fn candidates_reverse_newest_first() {
        let jsonl = [
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"git commit -m \"[CO-1] first\""}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"git commit -m \"[CO-2] second\""}]}}"#,
        ]
        .join("\n");
        assert_eq!(
            candidates_reverse(&jsonl, None),
            vec!["[CO-2] second", "[CO-1] first"],
        );
    }

    #[test]
    fn candidates_reverse_skips_assistant_messages_without_commit_blocks() {
        let jsonl = [
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Just thinking out loud"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"git commit -m \"[CO-9] only one\""}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Following up"}]}}"#,
        ]
        .join("\n");
        assert_eq!(
            candidates_reverse(&jsonl, None),
            vec!["[CO-9] only one"],
        );
    }

    #[test]
    fn candidates_reverse_extracts_multiple_from_same_response() {
        let jsonl = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"First:\n  git commit -m \"[CO-1] one\"\nThen:\n  git commit -m \"[CO-2] two\""}]}}"#;
        // Both extracted; the response is the newest (only) one, but
        // within it, the first commit-m comes before the second in the
        // original text — extract_commit_messages preserves document
        // order, and there's only one response so no reversal.
        assert_eq!(
            candidates_reverse(jsonl, None),
            vec!["[CO-1] one", "[CO-2] two"],
        );
    }

    // Mixed-ticket session: CO-1 was finished first, then CO-2 work happened.
    // When shipping from a worktree on CO-1, the CO-1 message should auto-pick
    // — but the CO-2 message is still reachable via [p].
    #[test]
    fn candidates_reverse_prefers_matching_ticket_when_present() {
        let jsonl = [
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"git commit -m \"[CO-1] one\""}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"git commit -m \"[CO-2] two\""}]}}"#,
        ]
        .join("\n");
        assert_eq!(
            candidates_reverse(&jsonl, Some("CO-1")),
            vec!["[CO-1] one", "[CO-2] two"],
        );
    }

    // Within each tier, newest-first ordering is preserved.
    #[test]
    fn candidates_reverse_preserves_newest_first_within_each_tier() {
        let jsonl = [
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"git commit -m \"[CO-1] one-a\""}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"git commit -m \"[CO-2] two-a\""}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"git commit -m \"[CO-1] one-b\""}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"git commit -m \"[CO-2] two-b\""}]}}"#,
        ]
        .join("\n");
        assert_eq!(
            candidates_reverse(&jsonl, Some("CO-1")),
            vec!["[CO-1] one-b", "[CO-1] one-a", "[CO-2] two-b", "[CO-2] two-a"],
        );
    }

    // Substring guard: ticket "CO-1" must not match "[CO-12] …" — the trailing
    // `]` in the needle anchors the match to the full bracketed ticket.
    #[test]
    fn candidates_reverse_ticket_match_requires_closing_bracket() {
        let jsonl = [
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"git commit -m \"[CO-12] twelve\""}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"git commit -m \"[CO-1] one\""}]}}"#,
        ]
        .join("\n");
        // CO-12 must not be considered a CO-1 match; CO-1 takes the lead.
        assert_eq!(
            candidates_reverse(&jsonl, Some("CO-1")),
            vec!["[CO-1] one", "[CO-12] twelve"],
        );
    }

    // No commits match the branch's ticket → fall back to chronological order,
    // exactly as if ticket were None. The user walks with [p] from the newest.
    #[test]
    fn candidates_reverse_no_matches_falls_back_to_chronological() {
        let jsonl = [
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"git commit -m \"[CO-2] two\""}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"git commit -m \"[CO-3] three\""}]}}"#,
        ]
        .join("\n");
        assert_eq!(
            candidates_reverse(&jsonl, Some("CO-999")),
            vec!["[CO-3] three", "[CO-2] two"],
        );
    }
}
