use std::path::Path;

/// Encode an absolute path into the form Claude Code uses for
/// `~/.claude/projects/<encoded>/` directory names.
///
/// Rule: every character that is not `[A-Za-z0-9-]` becomes `-`.
/// Existing `-` characters are preserved as-is; adjacent transformed
/// characters produce adjacent `-`s (no collapsing).
///
/// Returns `None` if the path is not valid UTF-8.
pub fn encode_path(path: &Path) -> Option<String> {
    let s = path.to_str()?;
    Some(
        s.chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn encodes_verified_worktree_path() {
        let p = PathBuf::from(
            "/Users/mattrice/programming/work/claims-dir/.worktrees/CO-5281-controller-parser",
        );
        assert_eq!(
            encode_path(&p).unwrap(),
            "-Users-mattrice-programming-work-claims-dir--worktrees-CO-5281-controller-parser",
        );
    }

    #[test]
    fn encodes_simple_path() {
        assert_eq!(
            encode_path(&PathBuf::from("/Users/foo/bar")).unwrap(),
            "-Users-foo-bar",
        );
    }

    #[test]
    fn encodes_slash_then_dot_to_double_dash() {
        assert_eq!(
            encode_path(&PathBuf::from("/a/.b/c")).unwrap(),
            "-a--b-c",
        );
    }

    #[test]
    fn preserves_existing_hyphens() {
        assert_eq!(
            encode_path(&PathBuf::from("/CO-123-foo")).unwrap(),
            "-CO-123-foo",
        );
    }

    #[test]
    fn converts_underscores_to_dashes() {
        assert_eq!(
            encode_path(&PathBuf::from("/a_b/c_d")).unwrap(),
            "-a-b-c-d",
        );
    }

    #[test]
    fn empty_path_produces_empty_string() {
        assert_eq!(encode_path(&PathBuf::from("")).unwrap(), "");
    }

    #[test]
    fn handles_multiple_consecutive_separators() {
        assert_eq!(
            encode_path(&PathBuf::from("/a//b")).unwrap(),
            "-a--b",
        );
    }
}
