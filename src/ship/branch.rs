use std::process::Command;

const TICKET_PREFIX: &str = "CO-";

#[derive(Debug)]
pub enum BranchError {
    DetachedHead,
    NotAGitRepo,
    GitCommandFailed(String),
    InvalidUtf8,
}

/// Returns the current branch name, or an error describing why we couldn't.
pub fn current_branch() -> Result<String, BranchError> {
    let output = Command::new("git")
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

/// Extract the `CO-XXXX` ticket from a branch name of the form
/// `CO-XXXX/short-description`. Returns `None` if the branch doesn't
/// match the convention.
pub fn extract_ticket(branch: &str) -> Option<String> {
    let rest = branch.strip_prefix(TICKET_PREFIX)?;
    let digit_count = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_count == 0 {
        return None;
    }
    let (digits, after) = rest.split_at(digit_count);
    if !after.starts_with('/') {
        return None;
    }
    Some(format!("{TICKET_PREFIX}{digits}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_standard_ticket() {
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

    #[test]
    fn no_ticket_when_no_slash() {
        assert_eq!(extract_ticket("CO-1234"), None);
    }

    #[test]
    fn no_ticket_when_no_slash_after_digits() {
        assert_eq!(extract_ticket("CO-1234-foo"), None);
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

    #[test]
    fn no_ticket_when_digits_followed_by_dash_not_slash() {
        assert_eq!(extract_ticket("CO-12-34/foo"), None);
    }

    #[test]
    fn empty_branch_name() {
        assert_eq!(extract_ticket(""), None);
    }
}
