use std::path::Path;
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
pub fn is_dirty(cwd: &Path) -> Result<bool, GitError> {
    let output = run(cwd, &["status", "--porcelain"])?;
    Ok(!output.stdout.iter().all(|b| b.is_ascii_whitespace()))
}

/// One-line summary like "4 files changed, 87 insertions(+), 12 deletions(-)".
/// Returns an empty string if the diff is empty.
pub fn diff_stat_summary(cwd: &Path) -> Result<String, GitError> {
    let output = run(cwd, &["diff", "--stat", "HEAD"])?;
    Ok(last_diff_stat_line(&output.stdout).to_string())
}

/// `git add -A`
pub fn add_all(cwd: &Path) -> Result<(), GitError> {
    run(cwd, &["add", "-A"])?;
    Ok(())
}

/// `git commit -m "<message>"`. Returns the new short SHA on success.
/// Maps hook-failure exit codes to `HookFailed` so the caller can show
/// the hook output verbatim.
pub fn commit(cwd: &Path, message: &str) -> Result<String, GitError> {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(["commit", "-m", message])
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        return Err(GitError::HookFailed {
            stderr: combine_outputs(&stdout, &stderr),
        });
    }
    short_sha(cwd)
}

/// True if HEAD has upstream tracking configured.
pub fn has_upstream(cwd: &Path) -> bool {
    Command::new("git")
        .current_dir(cwd)
        .args(["rev-parse", "--abbrev-ref", "@{u}"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// `git push [--force-with-lease]`
pub fn push(cwd: &Path, force_with_lease: bool) -> Result<(), GitError> {
    let mut args = vec!["push"];
    if force_with_lease {
        args.push("--force-with-lease");
    }
    let output = Command::new("git").current_dir(cwd).args(&args).output()?;
    if !output.status.success() {
        return Err(GitError::GitFailed {
            command: format!("git {}", args.join(" ")),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(())
}

/// `git push -u origin HEAD [--force-with-lease]`
pub fn push_set_upstream(cwd: &Path, force_with_lease: bool) -> Result<(), GitError> {
    let mut args = vec!["push", "-u", "origin", "HEAD"];
    if force_with_lease {
        args.push("--force-with-lease");
    }
    let output = Command::new("git").current_dir(cwd).args(&args).output()?;
    if !output.status.success() {
        return Err(GitError::GitFailed {
            command: format!("git {}", args.join(" ")),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(())
}

/// Path to the main repo's `.git` dir. From a worktree, this resolves
/// back to the original repository's `.git`. Used to find the "common"
/// repo path when looking up sessions from a worktree.
pub fn common_dir(cwd: &Path) -> Result<std::path::PathBuf, GitError> {
    let output = run(cwd, &["rev-parse", "--git-common-dir"])?;
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let raw = std::path::PathBuf::from(&s);
    let resolved = if raw.is_absolute() {
        raw
    } else {
        cwd.join(raw)
    };
    Ok(resolved)
}

fn short_sha(cwd: &Path) -> Result<String, GitError> {
    let output = run(cwd, &["rev-parse", "--short", "HEAD"])?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run(cwd: &Path, args: &[&str]) -> Result<Output, GitError> {
    let output = Command::new("git").current_dir(cwd).args(args).output()?;
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
