use assert_cmd::Command as TestCommand;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// End-to-end happy path: a tempdir git repo with one staged change and a
/// fixture JSONL on a fake HOME — `tend-ship ship --force` should commit
/// and push to a bare-repo origin.
#[test]
fn force_commits_and_pushes_with_ticket_prefix() {
    let world = TestWorld::new("CO-9999/e2e-happy-path");
    world.write_fixture_session(
        "session-abc",
        &assistant_text_jsonl(
            r#"git commit -m "[CO-9999] Implement the feature and add tests""#,
        ),
    );
    world.write_change("change.txt", "new content\n");

    TestCommand::cargo_bin("tend-ship")
        .unwrap()
        .args(["push", "--force"])
        .env("HOME", world.home.path())
        .current_dir(&world.canonical_repo)
        .assert()
        .success();

    let subject = remote_branch_subject(&world.remote, &world.branch);
    assert_eq!(subject, "[CO-9999] Implement the feature and add tests");
}

/// Branch without a CO-XXXX prefix → no ticket, no `[…]` in the subject.
#[test]
fn no_ticket_prefix_when_branch_doesnt_match() {
    let world = TestWorld::new("scratch/no-ticket-here");
    world.write_fixture_session(
        "session-noticket",
        &assistant_text_jsonl(r#"git commit -m "Made a quick fix""#),
    );
    world.write_change("noted.txt", "x\n");

    TestCommand::cargo_bin("tend-ship")
        .unwrap()
        .args(["push", "--force"])
        .env("HOME", world.home.path())
        .current_dir(&world.canonical_repo)
        .assert()
        .success();

    assert_eq!(remote_branch_subject(&world.remote, &world.branch), "Made a quick fix");
}

/// `-m <text>` skips session reading entirely; subject prefix still applies.
#[test]
fn message_override_skips_session_reading() {
    let world = TestWorld::new("CO-7777/override");
    // Intentionally no session JSONL — `-m` should not require one.
    world.write_change("a.txt", "1\n");

    TestCommand::cargo_bin("tend-ship")
        .unwrap()
        .args(["push", "-f", "-m", "Skip session and use this directly"])
        .env("HOME", world.home.path())
        .current_dir(&world.canonical_repo)
        .assert()
        .success();

    assert_eq!(
        remote_branch_subject(&world.remote, &world.branch),
        "[CO-7777] Skip session and use this directly",
    );
}

/// `--no-push` commits locally but leaves origin untouched.
#[test]
fn no_push_keeps_remote_untouched() {
    let world = TestWorld::new("CO-1111/no-push");
    world.write_fixture_session(
        "session-np",
        &assistant_text_jsonl(r#"git commit -m "[CO-1111] Committed locally only""#),
    );
    world.write_change("local.txt", "x\n");

    let remote_head_before = remote_branch_sha(&world.remote, &world.branch);

    TestCommand::cargo_bin("tend-ship")
        .unwrap()
        .args(["push", "--force", "--no-push"])
        .env("HOME", world.home.path())
        .current_dir(&world.canonical_repo)
        .assert()
        .success();

    let remote_head_after = remote_branch_sha(&world.remote, &world.branch);
    assert_eq!(
        remote_head_before, remote_head_after,
        "remote should not have moved with --no-push",
    );

    // Verify the commit IS on the local working tree
    let local_subject = local_head_subject(&world.canonical_repo);
    assert_eq!(local_subject, "[CO-1111] Committed locally only");
}

/// Clean tree → exit 0 with "Nothing to commit." — no commit, no push.
#[test]
fn nothing_to_commit_when_tree_is_clean() {
    let world = TestWorld::new("CO-2222/clean");
    let remote_head_before = remote_branch_sha(&world.remote, &world.branch);

    let output = TestCommand::cargo_bin("tend-ship")
        .unwrap()
        .args(["push", "--force"])
        .env("HOME", world.home.path())
        .current_dir(&world.canonical_repo)
        .output()
        .unwrap();
    assert!(output.status.success(), "tend-ship should succeed on clean tree");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("Nothing to commit."),
        "expected 'Nothing to commit.' got: {stdout}",
    );

    assert_eq!(remote_head_before, remote_branch_sha(&world.remote, &world.branch));
}

/// Fallback: run tend-ship from inside a worktree whose own encoded path
/// has no session; tend-ship walks to the main repo via `--git-common-dir`
/// and uses *that* repo's session.
#[test]
fn worktree_falls_back_to_main_repo_session() {
    let world = TestWorld::new("CO-4000/main");
    let worktree = world.add_worktree("CO-4444-side", "CO-4444/side");

    // Session lives ONLY in the main repo's encoded dir, not the worktree's.
    let main_session_dir = world.fixture_dir_for(&world.canonical_repo);
    let jsonl = assistant_text_jsonl(r#"git commit -m "[CO-4444] Made the worktree edit""#);
    fs::write(
        main_session_dir.join("main-session.jsonl"),
        format!("{jsonl}\n"),
    )
    .unwrap();

    fs::write(worktree.join("from-worktree.txt"), "edit\n").unwrap();

    TestCommand::cargo_bin("tend-ship")
        .unwrap()
        .args(["push", "--force"])
        .env("HOME", world.home.path())
        .current_dir(&worktree)
        .assert()
        .success();

    assert_eq!(
        remote_branch_subject(&world.remote, "CO-4444/side"),
        "[CO-4444] Made the worktree edit",
    );
}

/// Option 3: from the main repo, name a worktree by its basename and have
/// tend-ship resolve it via `git worktree list` then operate on that
/// worktree.
#[test]
fn worktree_resolved_by_name_from_main_repo() {
    let world = TestWorld::new("CO-5000/main");
    let worktree = world.add_worktree("CO-5555-byname", "CO-5555/byname");

    // Session lives in the main repo's encoded dir (where Claude was launched).
    let main_session_dir = world.fixture_dir_for(&world.canonical_repo);
    let jsonl = assistant_text_jsonl(
        r#"git commit -m "[CO-5555] Made the change in the side worktree""#,
    );
    fs::write(
        main_session_dir.join("main-session.jsonl"),
        format!("{jsonl}\n"),
    )
    .unwrap();

    fs::write(worktree.join("by-name.txt"), "edit\n").unwrap();

    // Run from the MAIN repo, naming the worktree positionally.
    TestCommand::cargo_bin("tend-ship")
        .unwrap()
        .args(["CO-5555-byname", "--force"])
        .env("HOME", world.home.path())
        .current_dir(&world.canonical_repo)
        .assert()
        .success();

    assert_eq!(
        remote_branch_subject(&world.remote, "CO-5555/byname"),
        "[CO-5555] Made the change in the side worktree",
    );
}

/// Simulated tend invocation: `TEND_SESSION_JSONL` + `TEND_SESSION_CWD`
/// set, no positional args. tend-ship should use the provided JSONL
/// directly and run git against TEND_SESSION_CWD.
#[test]
fn tend_extension_envvars_drive_session_and_target() {
    let world = TestWorld::new("CO-7000/from-tend");

    // Stash the JSONL anywhere; tend would pass an absolute path so we
    // don't even need it under the cwd-encoded directory.
    let some_dir = world.home.path().join("anywhere");
    fs::create_dir_all(&some_dir).unwrap();
    let jsonl_path = some_dir.join("explicit.jsonl");
    fs::write(
        &jsonl_path,
        assistant_text_jsonl(r#"git commit -m "[CO-7000] Invoked through tend""#),
    )
    .unwrap();

    world.write_change("via-tend.txt", "x\n");

    TestCommand::cargo_bin("tend-ship")
        .unwrap()
        .args(["push", "--force"])
        .env("HOME", world.home.path())
        .env("TEND_SESSION_JSONL", &jsonl_path)
        .env("TEND_SESSION_CWD", &world.canonical_repo)
        // current_dir is somewhere else (would be where tend itself was launched)
        .current_dir(world.home.path())
        .assert()
        .success();

    assert_eq!(
        remote_branch_subject(&world.remote, &world.branch),
        "[CO-7000] Invoked through tend",
    );
}

/// Tend-extension env vars + positional WORKTREE: the positional should
/// take precedence over TEND_SESSION_CWD for the git target.
#[test]
fn positional_worktree_wins_over_tend_session_cwd() {
    let world = TestWorld::new("CO-8000/main");
    let worktree = world.add_worktree("CO-8888-side", "CO-8888/side");

    let some_dir = world.home.path().join("tend-data");
    fs::create_dir_all(&some_dir).unwrap();
    let jsonl_path = some_dir.join("explicit.jsonl");
    fs::write(
        &jsonl_path,
        assistant_text_jsonl(r#"git commit -m "[CO-8888] Side-worktree change""#),
    )
    .unwrap();

    fs::write(worktree.join("here.txt"), "x\n").unwrap();

    TestCommand::cargo_bin("tend-ship")
        .unwrap()
        // `CO-8888-side` is the positional WORKTREE; it overrides TEND_SESSION_CWD
        .args(["push", "CO-8888-side", "--force"])
        .env("HOME", world.home.path())
        .env("TEND_SESSION_JSONL", &jsonl_path)
        .env("TEND_SESSION_CWD", &world.canonical_repo)
        .current_dir(&world.canonical_repo)
        .assert()
        .success();

    assert_eq!(
        remote_branch_subject(&world.remote, "CO-8888/side"),
        "[CO-8888] Side-worktree change",
    );
}

/// pfwl subcommand routes through to push --force-with-lease.
#[test]
fn pfwl_uses_force_with_lease() {
    let world = TestWorld::new("CO-6000/fwl");
    world.write_fixture_session(
        "fwl-session",
        &assistant_text_jsonl(r#"git commit -m "[CO-6000] Rewrote the history""#),
    );
    world.write_change("rewrite.txt", "y\n");

    TestCommand::cargo_bin("tend-ship")
        .unwrap()
        .args(["pfwl", "--force"])
        .env("HOME", world.home.path())
        .current_dir(&world.canonical_repo)
        .assert()
        .success();

    assert_eq!(
        remote_branch_subject(&world.remote, &world.branch),
        "[CO-6000] Rewrote the history",
    );
}

/// No JSONL for cwd → exit 2 with a hint to use -m.
#[test]
fn no_session_jsonl_exits_with_code_2() {
    let world = TestWorld::new("CO-3333/no-session");
    world.write_change("solo.txt", "x\n");

    TestCommand::cargo_bin("tend-ship")
        .unwrap()
        .args(["push", "--force"])
        .env("HOME", world.home.path())
        .current_dir(&world.canonical_repo)
        .assert()
        .code(2);
}

// ---- Test harness ----------------------------------------------------------

struct TestWorld {
    home: TempDir,
    _repo: TempDir,
    canonical_repo: PathBuf,
    remote: PathBuf,
    _remote_dir: TempDir,
    branch: String,
}

impl TestWorld {
    fn new(branch: &str) -> Self {
        let home = TempDir::new().unwrap();
        let repo = TempDir::new().unwrap();
        let remote_dir = TempDir::new().unwrap();
        let remote = remote_dir.path().to_path_buf();
        let canonical_repo = repo.path().canonicalize().unwrap();

        // Bare remote
        run_git(remote.parent().unwrap(), &["init", "--bare", remote.to_str().unwrap()]);

        // Working repo (start on the requested branch)
        run_git(&canonical_repo, &["init"]);
        run_git(&canonical_repo, &["checkout", "-b", branch]);
        run_git(&canonical_repo, &["config", "user.email", "test@example.com"]);
        run_git(&canonical_repo, &["config", "user.name", "Test"]);
        run_git(&canonical_repo, &["config", "commit.gpgsign", "false"]);
        run_git(&canonical_repo, &["remote", "add", "origin", remote.to_str().unwrap()]);

        // Initial commit so HEAD exists; push to remote so it has a base.
        fs::write(canonical_repo.join("README.md"), "test\n").unwrap();
        run_git(&canonical_repo, &["add", "."]);
        run_git(&canonical_repo, &["commit", "-m", "initial"]);
        run_git(&canonical_repo, &["push", "-u", "origin", "HEAD"]);

        TestWorld {
            home,
            _repo: repo,
            canonical_repo,
            remote,
            _remote_dir: remote_dir,
            branch: branch.to_string(),
        }
    }

    fn fixture_dir(&self) -> PathBuf {
        let encoded = encode_path_for_test(&self.canonical_repo);
        let dir = self.home.path().join(".claude/projects").join(encoded);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_fixture_session(&self, name: &str, jsonl: &str) {
        let path = self.fixture_dir().join(format!("{name}.jsonl"));
        // Ensure newline-terminated, matching real transcripts.
        let contents = if jsonl.ends_with('\n') {
            jsonl.to_string()
        } else {
            format!("{jsonl}\n")
        };
        fs::write(path, contents).unwrap();
    }

    fn write_change(&self, name: &str, contents: &str) {
        fs::write(self.canonical_repo.join(name), contents).unwrap();
    }

    /// Create a worktree under `<main-repo>/.worktrees/<dir_name>` on
    /// branch `branch_name`. Returns the canonicalized worktree path.
    fn add_worktree(&self, dir_name: &str, branch_name: &str) -> PathBuf {
        let wt_dir = self.canonical_repo.join(".worktrees").join(dir_name);
        run_git(
            &self.canonical_repo,
            &["worktree", "add", "-b", branch_name, wt_dir.to_str().unwrap()],
        );
        wt_dir.canonicalize().unwrap()
    }

    fn fixture_dir_for(&self, path: &Path) -> PathBuf {
        let encoded = encode_path_for_test(path);
        let dir = self.home.path().join(".claude/projects").join(encoded);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}

fn run_git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git").current_dir(cwd).args(args).output().unwrap();
    if !output.status.success() {
        panic!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}

/// Build an assistant-message JSONL line whose text content is `text`,
/// with the minimum JSON-escaping needed for our fixtures (escape `\`,
/// `"`, and `\n`). Real transcripts come from Claude itself, but in
/// tests we hand-roll the strings.
fn assistant_text_jsonl(text: &str) -> String {
    let escaped: String = text
        .chars()
        .flat_map(|c| match c {
            '\\' => vec!['\\', '\\'],
            '"' => vec!['\\', '"'],
            '\n' => vec!['\\', 'n'],
            c => vec![c],
        })
        .collect();
    format!(
        r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":"{escaped}"}}]}}}}"#,
    )
}

fn encode_path_for_test(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect()
}

fn remote_branch_subject(remote: &Path, branch: &str) -> String {
    let out = Command::new("git")
        .args([
            "-C",
            remote.to_str().unwrap(),
            "log",
            "-1",
            "--pretty=%s",
            &format!("refs/heads/{branch}"),
        ])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn remote_branch_sha(remote: &Path, branch: &str) -> String {
    let out = Command::new("git")
        .args([
            "-C",
            remote.to_str().unwrap(),
            "rev-parse",
            &format!("refs/heads/{branch}"),
        ])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn local_head_subject(repo: &Path) -> String {
    let out = Command::new("git")
        .args(["-C", repo.to_str().unwrap(), "log", "-1", "--pretty=%s"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

