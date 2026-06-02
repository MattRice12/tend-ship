use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
#[allow(dead_code)]
pub enum WorktreeError {
    NotFound {
        name: String,
        candidates: Vec<String>,
    },
    Ambiguous {
        name: String,
        matches: Vec<PathBuf>,
    },
    NotARepo,
    GitFailed(String),
}

impl std::fmt::Display for WorktreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::NotFound { name, candidates } => {
                write!(f, "no worktree matches '{name}'")?;
                if !candidates.is_empty() {
                    write!(f, "; available: {}", candidates.join(", "))?;
                }
                Ok(())
            }
            Self::Ambiguous { name, matches } => {
                write!(
                    f,
                    "'{name}' is ambiguous; matches {} worktrees",
                    matches.len()
                )
            }
            Self::NotARepo => write!(f, "not in a git repository — can't enumerate worktrees"),
            Self::GitFailed(s) => write!(f, "git worktree list failed: {s}"),
        }
    }
}

/// Resolve a worktree identifier to an absolute path.
///
/// Rules:
/// - If `input` looks like a path (contains `/`, starts with `~`, `.`, or
///   `/`), it is returned as-is (after `~/` expansion).
/// - Otherwise, `git worktree list --porcelain` is run from `search_from`,
///   and worktrees whose basename matches `input` are considered. If
///   exactly one matches, it wins; if multiple, returns `Ambiguous`; if
///   none, returns `NotFound`.
pub fn resolve(input: &str, search_from: &Path) -> Result<PathBuf, WorktreeError> {
    if looks_like_path(input) {
        return Ok(expand_tilde(input));
    }

    let worktrees = list_worktrees(search_from)?;
    let matches: Vec<PathBuf> = worktrees
        .iter()
        .filter(|p| basename(p).as_deref() == Some(input))
        .cloned()
        .collect();

    match matches.len() {
        0 => {
            let candidates = worktrees
                .iter()
                .filter_map(|p| basename(p))
                .collect::<Vec<_>>();
            Err(WorktreeError::NotFound {
                name: input.to_string(),
                candidates,
            })
        }
        1 => Ok(matches.into_iter().next().unwrap()),
        _ => Err(WorktreeError::Ambiguous {
            name: input.to_string(),
            matches,
        }),
    }
}

fn looks_like_path(s: &str) -> bool {
    s.contains('/') || s.starts_with('~') || s.starts_with('.')
}

fn expand_tilde(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(s)
}

fn basename(path: &Path) -> Option<String> {
    path.file_name().and_then(|n| n.to_str()).map(String::from)
}

fn list_worktrees(cwd: &Path) -> Result<Vec<PathBuf>, WorktreeError> {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .map_err(|e| WorktreeError::GitFailed(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not a git repository") {
            return Err(WorktreeError::NotARepo);
        }
        return Err(WorktreeError::GitFailed(stderr.into_owned()));
    }

    Ok(parse_worktree_list(&String::from_utf8_lossy(&output.stdout)))
}

fn parse_worktree_list(stdout: &str) -> Vec<PathBuf> {
    stdout
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_path_detects_path_shapes() {
        assert!(looks_like_path("/abs/path"));
        assert!(looks_like_path("./rel"));
        assert!(looks_like_path("../up"));
        assert!(looks_like_path("~/home"));
        assert!(looks_like_path("a/b"));
        assert!(!looks_like_path("CO-5528-foo"));
        assert!(!looks_like_path("plain-name"));
    }

    #[test]
    fn parses_porcelain_worktree_list() {
        let stdout = "\
worktree /Users/me/repo
HEAD 1234abcd
branch refs/heads/main

worktree /Users/me/repo/.worktrees/CO-1/foo
HEAD 5678efgh
branch refs/heads/CO-1/foo

worktree /Users/me/repo/.worktrees/CO-2-bar
HEAD 9999aaaa
branch refs/heads/CO-2/bar
";
        assert_eq!(
            parse_worktree_list(stdout),
            vec![
                PathBuf::from("/Users/me/repo"),
                PathBuf::from("/Users/me/repo/.worktrees/CO-1/foo"),
                PathBuf::from("/Users/me/repo/.worktrees/CO-2-bar"),
            ],
        );
    }

    #[test]
    fn basename_extracts_last_segment() {
        assert_eq!(
            basename(Path::new("/a/b/CO-5528-foo")).as_deref(),
            Some("CO-5528-foo"),
        );
        assert_eq!(basename(Path::new("/")).as_deref(), None);
    }

    #[test]
    fn expand_tilde_replaces_with_home() {
        // SAFETY: setting an env var in a test process is fine for our needs
        unsafe {
            std::env::set_var("HOME", "/Users/test");
        }
        assert_eq!(expand_tilde("~/work"), PathBuf::from("/Users/test/work"));
        assert_eq!(expand_tilde("/abs"), PathBuf::from("/abs"));
        assert_eq!(expand_tilde("./rel"), PathBuf::from("./rel"));
    }
}
