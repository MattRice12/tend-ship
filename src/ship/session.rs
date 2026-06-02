use std::path::{Path, PathBuf};

const MAX_CANDIDATE_CHARS: usize = 600;
const MAX_CANDIDATE_SENTENCES: usize = 2;

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

/// Whether a candidate text passes the "looks like a summary" filter.
pub fn passes_filter(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.ends_with('?') {
        return false;
    }
    if starts_with_ci(trimmed, "Let me") {
        return false;
    }
    if trimmed.chars().count() > MAX_CANDIDATE_CHARS {
        return false;
    }
    if count_sentences(trimmed) > MAX_CANDIDATE_SENTENCES {
        return false;
    }
    true
}

/// All assistant texts that pass the filter, in reverse-chronological
/// order (newest first).
pub fn candidates_reverse(jsonl: &str) -> Vec<String> {
    let mut all = parse_assistant_texts(jsonl);
    all.reverse();
    all.into_iter().filter(|t| passes_filter(t)).collect()
}

fn starts_with_ci(haystack: &str, prefix: &str) -> bool {
    let h = haystack.trim_start();
    h.len() >= prefix.len()
        && h.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

/// Sentence count = number of `.<whitespace>` boundaries in the text + 1
/// (for the final sentence).
fn count_sentences(text: &str) -> usize {
    if text.trim().is_empty() {
        return 0;
    }
    let bytes = text.as_bytes();
    let mut boundaries = 0;
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'.' && bytes[i + 1].is_ascii_whitespace() {
            boundaries += 1;
        }
        i += 1;
    }
    boundaries + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_sentences_empty() {
        assert_eq!(count_sentences(""), 0);
        assert_eq!(count_sentences("   "), 0);
    }

    #[test]
    fn count_sentences_basic() {
        assert_eq!(count_sentences("Hello"), 1);
        assert_eq!(count_sentences("Hello."), 1);
        assert_eq!(count_sentences("Hello. World"), 2);
        assert_eq!(count_sentences("Hello. World."), 2);
        assert_eq!(count_sentences("One. Two. Three."), 3);
    }

    #[test]
    fn count_sentences_ignores_period_without_whitespace() {
        assert_eq!(count_sentences("e.g."), 1);
        assert_eq!(count_sentences("Hello.World"), 1);
    }

    #[test]
    fn filter_rejects_question() {
        assert!(!passes_filter("Should I do that?"));
    }

    #[test]
    fn filter_rejects_let_me_opener() {
        assert!(!passes_filter("Let me check the test output first"));
        assert!(!passes_filter("  Let me check"));
        // Case-insensitive
        assert!(!passes_filter("let me see"));
        assert!(!passes_filter("LET ME LOOK"));
    }

    #[test]
    fn filter_rejects_overlength() {
        let long = "x".repeat(601);
        assert!(!passes_filter(&long));
    }

    #[test]
    fn filter_rejects_three_sentences() {
        assert!(!passes_filter("One. Two. Three."));
    }

    #[test]
    fn filter_accepts_normal_summary() {
        assert!(passes_filter("Updated the model and tightened the cancellability rule."));
        assert!(passes_filter("All 47 specs pass. Pushed the lint fix and ready to ship."));
    }

    #[test]
    fn filter_accepts_empty_question_only_if_no_question_mark() {
        assert!(!passes_filter(""));
        assert!(passes_filter("Done"));
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
    fn parse_concatenates_multiple_text_blocks() {
        let jsonl = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"First"},{"type":"text","text":"Second"}]}}"#;
        assert_eq!(parse_assistant_texts(jsonl), vec!["First Second"]);
    }

    #[test]
    fn parse_skips_malformed_lines() {
        let jsonl = "not json\n{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"good\"}]}}\nalso not json";
        assert_eq!(parse_assistant_texts(jsonl), vec!["good"]);
    }

    #[test]
    fn parse_ignores_blank_lines() {
        let jsonl = "\n\n{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"hi\"}]}}\n\n";
        assert_eq!(parse_assistant_texts(jsonl), vec!["hi"]);
    }

    #[test]
    fn candidates_reverse_newest_first() {
        let jsonl = [
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"First message done"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Second message also done"}]}}"#,
        ]
        .join("\n");
        let candidates = candidates_reverse(&jsonl);
        assert_eq!(
            candidates,
            vec!["Second message also done", "First message done"],
        );
    }

    #[test]
    fn candidates_reverse_filters_out_questions_and_let_me() {
        let jsonl = [
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Done with the work"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Should I continue?"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Let me check the output"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Updated the model and pushed"}]}}"#,
        ]
        .join("\n");
        let candidates = candidates_reverse(&jsonl);
        assert_eq!(
            candidates,
            vec!["Updated the model and pushed", "Done with the work"],
        );
    }
}
