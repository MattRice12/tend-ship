use std::path::Path;
use std::process::Command;

#[derive(Debug)]
#[allow(dead_code)]
pub enum BranchError {
    DetachedHead,
    NotAGitRepo,
    GitCommandFailed(String),
    InvalidUtf8,
}

/// Returns the current branch name of the repo at `cwd`, or an error
/// describing why we couldn't.
pub fn current_branch(cwd: &Path) -> Result<String, BranchError> {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(["symbolic-ref", "--short", "HEAD"])
        .output()
        .map_err(|e| BranchError::GitCommandFailed(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not a git repository") {
            return Err(BranchError::NotAGitRepo);
        }
        return Err(BranchError::DetachedHead);
    }

    let s = String::from_utf8(output.stdout).map_err(|_| BranchError::InvalidUtf8)?;
    Ok(s.trim().to_string())
}

/// Extract a ticket-style prefix (`<PROJ>-<digits>`) from the start of a
/// branch name. Returns the matched substring (e.g. `"CO-5281"`, `"PROJ-42"`,
/// `"ABC123-7"`) or `None` if the branch doesn't start with one.
///
/// Shape: one or more leading ASCII uppercase letters, optionally followed by
/// uppercase letters or digits, then a literal `-`, then one or more digits.
/// Anything after the digits (a `/`, `-`, `_`, end-of-string, …) is fine — the
/// extractor doesn't require a particular separator, so both
/// `CO-5281/desc` and `PROJ-42-desc` extract their ticket.
pub fn extract_ticket(branch: &str) -> Option<String> {
    let bytes = branch.as_bytes();
    let mut i = 0;
    // [A-Z][A-Z0-9]*  — must start with at least one uppercase letter.
    if i >= bytes.len() || !bytes[i].is_ascii_uppercase() {
        return None;
    }
    i += 1;
    while i < bytes.len() && (bytes[i].is_ascii_uppercase() || bytes[i].is_ascii_digit()) {
        i += 1;
    }
    // Required `-`
    if i >= bytes.len() || bytes[i] != b'-' {
        return None;
    }
    i += 1;
    // \d+  — at least one digit.
    let digit_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == digit_start {
        return None;
    }
    Some(branch[..i].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_slash_separated_ticket() {
        assert_eq!(
            extract_ticket("CO-5281/controller-parser"),
            Some("CO-5281".to_string()),
        );
    }

    #[test]
    fn extracts_with_trailing_slash_only() {
        assert_eq!(
            extract_ticket("CO-1234/"),
            Some("CO-1234".to_string()),
        );
    }

    // Many teams use dash instead of slash between ticket and description.
    #[test]
    fn extracts_dash_separated_ticket() {
        assert_eq!(
            extract_ticket("CO-1234-foo"),
            Some("CO-1234".to_string()),
        );
    }

    // A bare-ticket branch name still extracts the ticket.
    #[test]
    fn extracts_bare_ticket() {
        assert_eq!(extract_ticket("CO-1234"), Some("CO-1234".to_string()));
    }

    // The project identifier isn't hardcoded — any UPPER-LETTERS-then-DIGITS works.
    #[test]
    fn extracts_other_ticket_systems() {
        assert_eq!(extract_ticket("PROJ-42/spec"), Some("PROJ-42".to_string()));
        assert_eq!(extract_ticket("ABC123-7/x"), Some("ABC123-7".to_string()));
        assert_eq!(extract_ticket("BUG-99-fix"), Some("BUG-99".to_string()));
    }

    #[test]
    fn no_ticket_when_not_at_start() {
        assert_eq!(extract_ticket("feat/CO-1234/foo"), None);
    }

    #[test]
    fn no_ticket_when_zero_digits() {
        assert_eq!(extract_ticket("CO-/foo"), None);
    }

    #[test]
    fn no_ticket_for_main() {
        assert_eq!(extract_ticket("main"), None);
    }

    // No leading uppercase letter at all → no ticket.
    #[test]
    fn no_ticket_for_lowercase_branch() {
        assert_eq!(extract_ticket("co-1234/foo"), None);
        assert_eq!(extract_ticket("feature-branch"), None);
    }

    #[test]
    fn empty_branch_name() {
        assert_eq!(extract_ticket(""), None);
    }
}
