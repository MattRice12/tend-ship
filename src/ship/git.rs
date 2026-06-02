use std::process::{Command, Output};

#[derive(Debug)]
#[allow(dead_code)]
pub enum GitError {
    NotARepo,
    HookFailed { stderr: String },
    GitFailed { command: String, stderr: String },
    Io(std::io::Error),
}

impl From<std::io::Error> for GitError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// True if `git status --porcelain` produces any output (tracked changes
/// or untracked files).
pub fn is_dirty() -> Result<bool, GitError> {
    let output = run(&["status", "--porcelain"])?;
    Ok(!output.stdout.iter().all(|b| b.is_ascii_whitespace()))
}

/// One-line summary like "4 files changed, 87 insertions(+), 12 deletions(-)".
/// Returns an empty string if the diff is empty.
pub fn diff_stat_summary() -> Result<String, GitError> {
    let output = run(&["diff", "--stat", "HEAD"])?;
    Ok(last_diff_stat_line(&output.stdout).to_string())
}

/// `git add -A`
pub fn add_all() -> Result<(), GitError> {
    run(&["add", "-A"])?;
    Ok(())
}

/// `git commit -m "<message>"`. Returns the new short SHA on success.
/// Maps hook-failure exit codes to `HookFailed` so the caller can show
/// the hook output verbatim.
pub fn commit(message: &str) -> Result<String, GitError> {
    let output = Command::new("git")
        .args(["commit", "-m", message])
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        return Err(GitError::HookFailed {
            stderr: combine_outputs(&stdout, &stderr),
        });
    }
    short_sha()
}

/// True if HEAD has upstream tracking configured.
pub fn has_upstream() -> bool {
    Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "@{u}"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// `git push`
pub fn push() -> Result<(), GitError> {
    let output = Command::new("git").arg("push").output()?;
    if !output.status.success() {
        return Err(GitError::GitFailed {
            command: "git push".to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(())
}

/// `git push -u origin HEAD`
pub fn push_set_upstream() -> Result<(), GitError> {
    let output = Command::new("git")
        .args(["push", "-u", "origin", "HEAD"])
        .output()?;
    if !output.status.success() {
        return Err(GitError::GitFailed {
            command: "git push -u origin HEAD".to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(())
}

fn short_sha() -> Result<String, GitError> {
    let output = run(&["rev-parse", "--short", "HEAD"])?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run(args: &[&str]) -> Result<Output, GitError> {
    let output = Command::new("git").args(args).output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not a git repository") {
            return Err(GitError::NotARepo);
        }
        return Err(GitError::GitFailed {
            command: format!("git {}", args.join(" ")),
            stderr: stderr.into_owned(),
        });
    }
    Ok(output)
}

fn last_diff_stat_line(stdout: &[u8]) -> &str {
    let s = std::str::from_utf8(stdout).unwrap_or("");
    s.lines().filter(|l| !l.trim().is_empty()).next_back().unwrap_or("")
}

fn combine_outputs(stdout: &str, stderr: &str) -> String {
    match (stdout.trim().is_empty(), stderr.trim().is_empty()) {
        (true, true) => String::new(),
        (false, true) => stdout.to_string(),
        (true, false) => stderr.to_string(),
        (false, false) => format!("{stdout}\n{stderr}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_diff_stat_line_picks_summary() {
        let output = b" file.rs | 12 ++++++++++++\n 1 file changed, 12 insertions(+)\n";
        assert_eq!(
            last_diff_stat_line(output),
            " 1 file changed, 12 insertions(+)",
        );
    }

    #[test]
    fn last_diff_stat_line_handles_empty() {
        assert_eq!(last_diff_stat_line(b""), "");
        assert_eq!(last_diff_stat_line(b"\n\n"), "");
    }

    #[test]
    fn last_diff_stat_line_single_line() {
        assert_eq!(
            last_diff_stat_line(b"1 file changed, 1 insertion(+)\n"),
            "1 file changed, 1 insertion(+)",
        );
    }

    #[test]
    fn combine_outputs_both_present() {
        assert_eq!(combine_outputs("out", "err"), "out\nerr");
    }

    #[test]
    fn combine_outputs_only_stdout() {
        assert_eq!(combine_outputs("out", ""), "out");
    }

    #[test]
    fn combine_outputs_only_stderr() {
        assert_eq!(combine_outputs("", "err"), "err");
    }

    #[test]
    fn combine_outputs_neither() {
        assert_eq!(combine_outputs("", ""), "");
    }
}
