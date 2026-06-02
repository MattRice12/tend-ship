mod branch;
mod cli;
mod git;
mod session;
mod worktree;

use clap::Parser;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Push mode chosen by which subcommand was invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushMode {
    /// Plain `git push`.
    Normal,
    /// `git push --force-with-lease`.
    ForceWithLease,
}

pub fn run(args: &[String], push_mode: PushMode) -> i32 {
    let subcommand_name = match push_mode {
        PushMode::Normal => "push",
        PushMode::ForceWithLease => "pfwl",
    };
    let mut argv = vec![subcommand_name.to_string()];
    argv.extend_from_slice(args);
    let parsed = match cli::ShipArgs::try_parse_from(&argv) {
        Ok(p) => p,
        Err(e) => {
            let kind = e.kind();
            let _ = e.print();
            return if matches!(
                kind,
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                0
            } else {
                2
            };
        }
    };

    match execute(parsed, push_mode) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{e}");
            e.exit_code()
        }
    }
}

#[derive(Debug)]
enum ShipError {
    NotARepo,
    DetachedHead,
    NoSession(String),
    NoCandidates(String),
    HookFailed(String),
    GitFailed(String),
    Io(String),
}

impl ShipError {
    fn exit_code(&self) -> i32 {
        match self {
            Self::NotARepo
            | Self::DetachedHead
            | Self::NoSession(_)
            | Self::NoCandidates(_) => 2,
            Self::HookFailed(_) | Self::GitFailed(_) | Self::Io(_) => 3,
        }
    }
}

impl std::fmt::Display for ShipError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::NotARepo => write!(f, "tend-ship: not in a git repository"),
            Self::DetachedHead => {
                write!(f, "tend-ship: HEAD is detached; check out a branch first")
            }
            Self::NoSession(m) | Self::NoCandidates(m) => write!(f, "tend-ship: {m}"),
            Self::HookFailed(out) | Self::GitFailed(out) => write!(f, "{out}"),
            Self::Io(m) => write!(f, "tend-ship: {m}"),
        }
    }
}

fn execute(args: cli::ShipArgs, push_mode: PushMode) -> Result<i32, ShipError> {
    let cwd = std::env::current_dir().map_err(|e| ShipError::Io(e.to_string()))?;

    // Target precedence:
    //   1. Positional WORKTREE arg
    //   2. TEND_PROJECT_DIR env var (set when invoked as a tend extension)
    //   3. Process cwd
    let target = match &args.worktree {
        Some(input) => worktree::resolve(input, &cwd)
            .map_err(|e| ShipError::NoSession(e.to_string()))?,
        None => match std::env::var_os("TEND_PROJECT_DIR") {
            Some(s) => PathBuf::from(s),
            None => cwd.clone(),
        },
    };

    match git::is_dirty(&target) {
        Ok(false) => {
            println!("Nothing to commit.");
            return Ok(0);
        }
        Ok(true) => {}
        Err(git::GitError::NotARepo) => return Err(ShipError::NotARepo),
        Err(e) => return Err(ShipError::Io(format!("{e:?}"))),
    }

    let branch = match branch::current_branch(&target) {
        Ok(b) => b,
        Err(branch::BranchError::DetachedHead) => return Err(ShipError::DetachedHead),
        Err(branch::BranchError::NotAGitRepo) => return Err(ShipError::NotARepo),
        Err(e) => return Err(ShipError::Io(format!("{e:?}"))),
    };
    let ticket = branch::extract_ticket(&branch);
    let diff_stat = git::diff_stat_summary(&target).unwrap_or_default();

    let (candidates, session_path) = if args.message.is_some() {
        (Vec::new(), None)
    } else {
        load_candidates(&target, args.session.as_deref(), ticket.as_deref())?
    };

    if args.message.is_none() && candidates.is_empty() {
        return Err(ShipError::NoCandidates(
            "no usable session candidates found; use -m to override".into(),
        ));
    }

    let mut cursor: usize = 0;
    let mut override_msg: Option<String> = args.message.clone();
    let mut needs_redisplay = true;

    loop {
        // Auto-picked candidates are already complete commit messages
        // (Claude crafted them with the ticket prefix). The `-m` / `[m]`
        // override path runs through compose_message so an unprefixed
        // subject gets the ticket prefix applied.
        let message = match &override_msg {
            Some(t) => compose_message(t, ticket.as_deref()),
            None => candidates[cursor].clone(),
        };

        if needs_redisplay {
            print_preview(
                &branch,
                ticket.as_deref(),
                &diff_stat,
                session_path.as_deref(),
                &message,
            );
            needs_redisplay = false;
        }

        if args.force {
            return commit_and_push(&target, &message, !args.no_push, push_mode);
        }

        match prompt()? {
            PromptResult::Yes => {
                return commit_and_push(&target, &message, !args.no_push, push_mode);
            }
            PromptResult::No => return Ok(1),
            PromptResult::Previous => {
                if override_msg.is_some() {
                    override_msg = None;
                    needs_redisplay = true;
                } else if cursor + 1 < candidates.len() {
                    cursor += 1;
                    needs_redisplay = true;
                } else {
                    println!("No older candidates.");
                }
            }
            PromptResult::Message => {
                print!("Enter subject: ");
                io::stdout()
                    .flush()
                    .map_err(|e| ShipError::Io(e.to_string()))?;
                let input = read_line()?;
                let trimmed = input.trim();
                if !trimmed.is_empty() {
                    override_msg = Some(trimmed.to_string());
                    needs_redisplay = true;
                }
            }
        }
    }
}

fn load_candidates(
    target: &Path,
    session_id: Option<&str>,
    ticket: Option<&str>,
) -> Result<(Vec<String>, Option<PathBuf>), ShipError> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| ShipError::Io("$HOME not set".into()))?;
    let claude_home = home.join(".claude");

    let session_path = match session_id {
        Some(id) => {
            // Explicit `-s` requires exact-target match — no fallback.
            let encoded = crate::encode::encode_path(target)
                .ok_or_else(|| ShipError::Io("target is not valid UTF-8".into()))?;
            let path = claude_home
                .join("projects")
                .join(encoded)
                .join(format!("{id}.jsonl"));
            if !path.is_file() {
                return Err(ShipError::NoSession(format!(
                    "session file not found: {}",
                    path.display()
                )));
            }
            path
        }
        // If invoked as a tend extension, tend passes the chosen transcript
        // path via env var. Use it directly — no discovery, no fallback.
        None if std::env::var_os("TEND_TRANSCRIPT").is_some() => {
            let p = PathBuf::from(std::env::var_os("TEND_TRANSCRIPT").unwrap());
            if !p.is_file() {
                return Err(ShipError::NoSession(format!(
                    "TEND_TRANSCRIPT points at a missing file: {}",
                    p.display()
                )));
            }
            p
        }
        None => find_session_with_fallback(&claude_home, target)?,
    };

    let content = std::fs::read_to_string(&session_path)
        .map_err(|e| ShipError::Io(e.to_string()))?;
    let candidates = session::candidates_reverse(&content, ticket);
    Ok((candidates, Some(session_path)))
}

/// Look for a newest session JSONL in two places, in order:
///   1. The target directory itself
///   2. The target's "main repo" (resolved via `git rev-parse --git-common-dir`)
///      — relevant when target is a worktree and Claude Code was launched
///      from the main repo.
/// First hit wins. If neither has a session, returns `NoSession`.
fn find_session_with_fallback(claude_home: &Path, target: &Path) -> Result<PathBuf, ShipError> {
    if let Some(p) = session::newest_session_path(claude_home, target) {
        return Ok(p);
    }
    if let Ok(common) = git::common_dir(target) {
        if let Some(parent) = common.parent() {
            let main = parent.canonicalize().unwrap_or_else(|_| parent.to_path_buf());
            if main != target {
                if let Some(p) = session::newest_session_path(claude_home, &main) {
                    return Ok(p);
                }
            }
        }
    }
    Err(ShipError::NoSession(
        "no session JSONL found for this directory or its main repo; use -m to override".into(),
    ))
}

fn compose_message(subject: &str, ticket: Option<&str>) -> String {
    if subject.starts_with('[') {
        return subject.to_string();
    }
    match ticket {
        Some(t) => format!("[{t}] {subject}"),
        None => subject.to_string(),
    }
}

fn print_preview(
    branch: &str,
    ticket: Option<&str>,
    diff_stat: &str,
    session_path: Option<&Path>,
    message: &str,
) {
    println!();
    println!("branch:    {branch}");
    println!("ticket:    {}", ticket.unwrap_or("(none)"));
    println!("changes:   {}", diff_stat.trim());
    if let Some(path) = session_path {
        let age = mtime_age(path).unwrap_or_else(|| "unknown".into());
        println!("session:   {}  ({age})", display_path(path));
    } else {
        println!("session:   (skipped — using -m override)");
    }
    println!("message:   {message}");
    println!();
}

fn mtime_age(path: &Path) -> Option<String> {
    let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
    let elapsed = SystemTime::now().duration_since(mtime).ok()?;
    Some(format_age(elapsed.as_secs()))
}

fn format_age(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

fn display_path(path: &Path) -> String {
    if let Some(home) = std::env::var_os("HOME") {
        let home_str = home.to_string_lossy().into_owned();
        let path_str = path.to_string_lossy();
        if let Some(rest) = path_str.strip_prefix(home_str.as_str()) {
            return format!("~{rest}");
        }
    }
    path.display().to_string()
}

#[derive(Debug)]
enum PromptResult {
    Yes,
    No,
    Previous,
    Message,
}

fn prompt() -> Result<PromptResult, ShipError> {
    loop {
        print!("Ship? [Y]es / [n]o / [p]revious / [m]essage override: ");
        io::stdout()
            .flush()
            .map_err(|e| ShipError::Io(e.to_string()))?;
        let input = read_line()?;
        let trimmed = input.trim().to_ascii_lowercase();
        let result = match trimmed.as_str() {
            "" | "y" | "yes" => PromptResult::Yes,
            "n" | "no" => PromptResult::No,
            "p" | "prev" | "previous" => PromptResult::Previous,
            "m" | "message" => PromptResult::Message,
            _ => continue,
        };
        return Ok(result);
    }
}

fn read_line() -> Result<String, ShipError> {
    let mut line = String::new();
    io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| ShipError::Io(e.to_string()))?;
    Ok(line)
}

fn commit_and_push(
    cwd: &Path,
    message: &str,
    push: bool,
    push_mode: PushMode,
) -> Result<i32, ShipError> {
    git::add_all(cwd).map_err(|e| ShipError::Io(format!("git add failed: {e:?}")))?;

    let sha = match git::commit(cwd, message) {
        Ok(sha) => sha,
        Err(git::GitError::HookFailed { stderr }) => return Err(ShipError::HookFailed(stderr)),
        Err(e) => return Err(ShipError::GitFailed(format!("{e:?}"))),
    };

    if push {
        let force_with_lease = matches!(push_mode, PushMode::ForceWithLease);
        let result = if git::has_upstream(cwd) {
            git::push(cwd, force_with_lease)
        } else {
            git::push_set_upstream(cwd, force_with_lease)
        };
        if let Err(git::GitError::GitFailed { command, stderr }) = result {
            return Err(ShipError::GitFailed(format!("{command} failed:\n{stderr}")));
        }
    }

    println!("✓ {sha} {message}");
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_adds_ticket_prefix() {
        assert_eq!(
            compose_message("fix the bug", Some("CO-1234")),
            "[CO-1234] fix the bug",
        );
    }

    #[test]
    fn compose_skips_prefix_when_no_ticket() {
        assert_eq!(compose_message("fix the bug", None), "fix the bug");
    }

    #[test]
    fn compose_skips_prefix_when_subject_already_bracketed() {
        assert_eq!(
            compose_message("[BUG-99] hotfix", Some("CO-1234")),
            "[BUG-99] hotfix",
        );
        assert_eq!(
            compose_message("[anything] goes", None),
            "[anything] goes",
        );
    }

    #[test]
    fn format_age_buckets() {
        assert_eq!(format_age(5), "5s ago");
        assert_eq!(format_age(59), "59s ago");
        assert_eq!(format_age(60), "1m ago");
        assert_eq!(format_age(3599), "59m ago");
        assert_eq!(format_age(3600), "1h ago");
        assert_eq!(format_age(86399), "23h ago");
        assert_eq!(format_age(86400), "1d ago");
    }
}
