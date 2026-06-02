# tend-ship — Design

Status: Draft (2026-06-02)
Owner: Matt Rice <matt.rice@kin.com>

## Purpose

A single-purpose CLI that ends a Claude Code session by composing a commit
message from the session's transcript, running `git add -A && git commit && git push`,
and exiting. Replaces the manual "read assistant summary, paste commit block"
step at the end of every conversation.

Inspired by [tend](https://github.com/jah2488/tend) — reads the same
`~/.claude/projects/<encoded>/*.jsonl` files. Where tend observes, tend-ship acts.

## Non-goals

- **Not a Claude Code plugin.** Plugins extend one running session; tend-ship
  works across sessions and lives outside the Claude process.
- **No LLM call.** Commit subject is extracted deterministically from the
  existing transcript — no `claude --resume`, no API cost, no network call
  beyond `git push`.
- **No live-session injection.** Doesn't write to running sessions or terminals.
- **No multi-action surface.** Commit-and-push only. Other operations (test,
  lint, PR-open) are out of scope for v1.

### Acknowledged trade-off

The commit subject is whatever Claude wrote in the `git commit -m "<msg>"`
block of its response. If you made manual edits after Claude's response, or
staged a subset of the changes, the message may not match the diff. The
interactive preview + `[p]revious` traversal + `[m]` / `-m` override exist
to catch this.
`claude --resume -p "commit and push"` would close that gap at the cost of
an LLM call; that path is explicitly out of scope.

## CLI

`tend-ship` is structured as a small dispatcher with two built-in subcommands
(`push` and `pfwl`) and PATH-based discovery for extensions. Invoking
`tend-ship` with no subcommand defaults to `push`. Both subcommands share the
same flow; they differ only in how they push.

```
tend-ship [SUBCOMMAND] [WORKTREE] [OPTIONS]

SUBCOMMANDS (built-in)
  push    Commit and `git push` using the current Claude session
          (the default if no subcommand is given)
  pfwl    Like push, but uses `git push --force-with-lease`
  help    Show usage and list discovered extensions

EXTENSIONS (PATH-discovered)
  Any executable in PATH named `tend-ship-<name>` is invocable as
  `tend-ship <name>`. If `<name>` is not a PATH extension, tend-ship falls
  through to `push` with `<name>` treated as the positional WORKTREE arg.
```

### `push` and `pfwl` subcommands

Both subcommands accept the same flags. The only behavioral difference is
that `pfwl` adds `--force-with-lease` to the final `git push`.

```
tend-ship [push|pfwl] [WORKTREE] [OPTIONS]

POSITIONAL
  WORKTREE               Path or basename of a worktree to ship.
                         Defaults to cwd.

OPTIONS
  -s, --session <id>     Use a specific session JSONL instead of newest-for-target
  -m, --message <text>   Skip session reading; use <text> as the subject
  -f, --force            Skip confirmation; ship the auto-picked candidate
      --no-push          Commit but don't push
  -h, --help

EXIT CODES
  0  Success or nothing-to-commit
  1  User aborted at confirmation
  2  No usable session (no JSONL, no commit-m candidates found, detached HEAD,
     not a git repo, worktree name not resolvable)
  3  Git or hook failure (stderr forwarded verbatim)
```

## Behavior

### `push` (default subcommand)

The behavior of `tend-ship` and `tend-ship push` is identical. `pfwl` is the
same flow except for the final `git push --force-with-lease`.

0. **Resolve target directory:**
   - If a positional `WORKTREE` arg was passed and it looks like a path
     (contains `/`, starts with `~`, `.`, or `/`), use it as-is.
   - Else if `WORKTREE` was passed as a bare name, run `git worktree list
     --porcelain` from cwd, find the worktree whose basename matches, and
     use its path. Ambiguous or unfound matches → exit 2 with `available:
     <list>`.
   - Else, target = cwd.
1. Verify target is a git repo. If `git status --porcelain` is empty, print
   `Nothing to commit.` and exit 0.
2. Verify HEAD on target is on a branch (not detached); else exit 2 with a
   hint.
3. **Locate session JSONL** (skipped if `-m` was passed). Encode target
   → `~/.claude/projects/<encoded>/`. Encoding rule:
   - Each character in the absolute path that doesn't match `[A-Za-z0-9-]`
     becomes `-`. Existing `-` characters are preserved as-is.
   - Adjacent transformed characters produce adjacent `-`s (no collapsing).
     So `/.worktrees` → `--worktrees`.
   - The leading `/` becomes a leading `-`.

   Verified against the existing
   `~/.claude/projects/-Users-mattrice-programming-work-claims-dir--worktrees-CO-5281-controller-parser/`
   entry, which round-trips correctly with this rule.
4. Pick the most recently-modified `*.jsonl` in that directory.

   **Fallback:** if the target's encoded directory doesn't exist or contains
   no JSONL files, query `git rev-parse --git-common-dir` from the target.
   For a worktree, this resolves to the *main repo's* `.git`. Take its
   parent (the main repo path), encode that, and try again. This is the
   common case where Claude Code was launched from the main repo and the
   user is shipping a worktree.

   If neither location has a session, exit 2 with a hint to use `-m`.

   The fallback only applies when `-s <id>` is **not** passed; `-s` requires
   the explicit project dir match.
5. Read the JSONL into memory and walk lines in reverse. For each record
   where `type == "assistant"` and the message contains at least one text
   content block, scan the concatenated text for `git commit -m "<msg>"`
   substrings. Each match is a candidate. Use Claude's literal message
   verbatim — no sentence-splitting, no filter, no inference.

   The user's commit-message convention (per their global CLAUDE.md) is
   "single-line subject, no body, no multi-`-m`, no HEREDOC", so the parser
   reads from the opening `"` to the next unescaped `"`.
6. The list of candidates is in reverse-chronological order (newest
   response's commit-m blocks first). If both lists are empty (no response
   contained a `git commit -m "..."` block), exit 2 with a hint to use `-m`.
7. Parse branch via `git symbolic-ref --short HEAD`. If matches `^(CO-\d+)/`,
   set `ticket = CO-XXXX`; else `ticket = none`. (Ticket is displayed in the
   preview but **not** re-applied to the auto-picked message — Claude
   already wrote the `[CO-XXXX]` prefix into the commit-m line.)
8. Compose proposed message:
   - Auto-picked candidate (from session) → use **verbatim** with no
     re-prefixing.
   - `-m <text>` or interactive `[m]` override:
     - If `<text>` starts with `[`, use verbatim
     - Else if `ticket` is set, prefix: `[<ticket>] <text>`
     - Else, use `<text>` as-is
9. Print preview:
    ```
    branch:    <branch>
    ticket:    <ticket or "(none)">
    changes:   <last line of `git diff --stat HEAD`>
    session:   <jsonl path relative to home>  (<age, e.g. "3m ago">)
    message:   <proposed message>
    ```
10. Prompt: `Ship? [Y]es / [n]o / [p]revious / [m]essage override: `

    State model: tend-ship maintains a cursor over the list of candidate
    commit messages, in reverse-chronological order (cursor starts at the
    most-recent / auto-picked candidate; `p` advances toward older).
    Candidates can be enumerated lazily.

    - Enter or `y`/`Y` → continue to step 11 with the candidate at the cursor
      (or with the active `[m]` override, if one is in effect — see below)
    - `n`/`N` → exit 1
    - `p`/`P` → advance cursor to the next-older candidate, re-compose the
      message (step 8), re-print preview, re-prompt. At the boundary (no
      older candidate exists), print `No older candidates.` and re-prompt
      with the current preview unchanged.
    - `m`/`M` → read a single line from stdin as a manual subject override;
      use it verbatim (no candidate filter, no length cap, no truncation —
      same semantics as `-m <text>` from the CLI). Re-print the preview with
      the override active and re-prompt. While an override is active, `p`
      clears the override and returns to cursor-based navigation. The user
      may press `m` again to re-enter override text.
    - Any other input → re-prompt with no change
11. Run `git add -A && git commit -m "<message>"`. If the pre-commit hook
    fails, forward stderr and exit 3. Do not push.
12. Probe upstream: `git rev-parse --abbrev-ref @{u}` (2>/dev/null).
    - Success → `git push` (or `git push --force-with-lease` for `pfwl`)
    - Failure (no upstream tracking) → `git push -u origin HEAD` (or
      `git push -u origin HEAD --force-with-lease` for `pfwl`)
    - Forward git's output regardless.
13. Print:
    ```
    ✓ [<branch> <short-sha>] <message>
    ```
    Exit 0.

### `--force` / `-f`

Steps 1–9 + 11–13. The prompt at step 10 is skipped; the first auto-picked
candidate is committed and pushed. The preview is still printed for the
record.

### Flag combinations

- `-f` + `-m <text>`: ships `<text>` without prompting (session reading skipped).
- `-f` + `--no-push`: commits the auto-picked candidate without prompting and
  without pushing.
- `-s <id>` + `-m <text>`: `-m` wins (session reading is skipped entirely);
  `-s` is silently ignored. Prefer not to pass both.

### `--message <text>` / `-m`

Steps 1–2, 7, 8 (using `<text>` directly as the subject, applying ticket
prefix only if `<text>` doesn't already start with `[`), 9, 10 (unless `-f`),
11–13. Session reading is skipped entirely.

### `--no-push`

Steps 1–11 only. Step 12 (push) is skipped.

### `--session <id>` / `-s`

Replaces step 4. Instead of picking the newest JSONL, use the file at
`~/.claude/projects/<encoded>/<id>.jsonl`. If the file doesn't exist, exit 2
with a hint. All other steps unchanged.

## Tend integration

tend-ship consumes [tend's extension contract](https://github.com/jah2488/tend#extensions).
`cargo install --path .` ships a second binary, `tend-action-ship`, which is
the same entry point under the name tend's discovery looks for on PATH.

### Self-description

`tend-action-ship --tend-describe` prints one line of JSON to stdout:

```json
{"name":"Ship","key":"S","description":"...","when":{"source":"terminal","has_branch":true}}
```

tend tolerates unknown fields, so `description` is forward-only — it has
no effect on tend today but is useful for any future menu enrichment and
for `tend --list-actions` output.

### Inbound env vars (read by tend-ship)

tend exports a full set of session locators on dispatch. tend-ship only
reads the two it actually needs:

| Variable           | Meaning                                                         |
| ------------------ | --------------------------------------------------------------- |
| `TEND_TRANSCRIPT`  | Absolute path to the session's `*.jsonl`. **Discriminator** — if set, extension mode is active and no session discovery runs. |
| `TEND_PROJECT_DIR` | Session's worktree-resolved cwd. Used as the git target if no positional WORKTREE arg is passed. |

Other env vars tend exports (`TEND_VERSION`, `TEND_ACTION`,
`TEND_SESSION_ID`, `TEND_SESSION_NAME`, `TEND_SOURCE`, `TEND_GIT_BRANCH`,
`TEND_WORKTREE`) are present but unread. Anything tend-ship needs from
git, it re-derives from git at invocation time — locators not snapshots.

### Behavior under extension mode

- The `WORKTREE` positional arg, if present, still wins for the git target.
- The transcript at `TEND_TRANSCRIPT` is read **fresh from disk** on every
  invocation, so tend-ship stays correct even if tend's view is stale.
- The cwd-based session-discovery fallback (worktree → main-repo via `git
  rev-parse --git-common-dir`) is bypassed — tend has already picked the
  session.
- Explicit `-s <id>` from the user still overrides everything.

## Extensibility

tend-ship supports third-party subcommands via the same convention git, cargo,
and kubectl use: any executable in PATH named `tend-ship-<name>` is invocable
as `tend-ship <name>`. Extensions are wholly independent programs in any
language; they don't link against tend-ship or import its code.

### Dispatch rules

1. `tend-ship` with no args → run built-in `push`.
2. `tend-ship <name> [args...]` where `<name>` matches a built-in (`push`,
   `pfwl`, `help`) → run that built-in with `[args...]`.
3. `tend-ship <name> [args...]` where `<name>` is not a built-in → look up
   `tend-ship-<name>` in PATH:
   - Found → `exec` it with `[args...]`, with extension env vars set (see
     below). tend-ship is replaced by the extension process; the extension's
     exit code is the process exit code.
   - Not found → fall through to `push` with `<name>` treated as the
     positional `WORKTREE` argument. (This is how `tend-ship CO-5528-foo`
     works without needing an explicit subcommand.)

### Environment passed to extensions

tend-ship sets these env vars before `exec`-ing an extension:

| Variable             | Value                                                  |
| -------------------- | ------------------------------------------------------ |
| `TEND_SHIP_VERSION`  | `tend-ship --version` string (semver)                  |
| `TEND_SHIP_CWD`      | Current working directory at invocation                |
| `TEND_SHIP_HOME`     | `~/.claude/` (resolved)                                |
| `TEND_SHIP_PROJECT`  | The encoded path under `~/.claude/projects/` for cwd, |
|                      | if it exists; empty otherwise                          |

Extensions own their own arg parsing, help, error reporting, and exit codes.
tend-ship makes no guarantees about what an extension does.

### `tend-ship help` output

Lists built-ins, then walks PATH for `tend-ship-*` executables and lists each
on a single line. If an extension supports `--help-summary` (one-line stdout
description), tend-ship calls it to annotate the listing; otherwise just the
name is shown. Failures to invoke an extension during help-listing are
silently ignored.

### Stable surface for extensions

The contract extensions can rely on:

1. The four `TEND_SHIP_*` env vars above
2. The `~/.claude/projects/<encoded>/` layout that tend-ship targets
3. tend-ship's own behavior is *not* part of the contract — extensions must
   not parse `tend-ship` stdout or shell out to it

That's deliberately small. Anything larger should be implemented as a separate
Rust library crate (e.g., the JSONL parsing logic, once it stabilizes — see
"Future work").

### Out of scope for v1

No hook system (pre-commit / post-commit lifecycle scripts), no config file,
no plugin loader, no in-process FFI. Those can be added later without breaking
the subcommand-discovery model. If a real need emerges, hooks would be the
natural next addition.

## Architecture

Single-crate Rust binary. The binary is a small dispatcher that delegates to
either the `ship` module or an exec'd extension.

```
tend-ship/
├── Cargo.toml
├── README.md
├── DESIGN.md            (this file)
├── src/
│   ├── main.rs          Argv dispatch: built-in subcommand vs PATH extension
│   ├── dispatch.rs      Subcommand resolution, env setup, exec
│   ├── help.rs          `tend-ship help` — built-ins + PATH walk
│   ├── ship/
│   │   ├── mod.rs       The ship subcommand entry point
│   │   ├── cli.rs       clap derive struct (ship-specific flags)
│   │   ├── encode.rs    cwd → ~/.claude/projects/<dir>
│   │   ├── session.rs   JSONL discovery + `git commit -m "..."` extraction
│   │   ├── branch.rs    git symbolic-ref + ticket regex
│   │   └── git.rs       status / diff --stat / commit / push wrappers
│   │                    (std::process::Command — no libgit2 dependency)
└── tests/
    ├── dispatch.rs      Subcommand routing, "not a subcommand" error path
    ├── ship/encode.rs   Path encoding (incl. .worktrees) unit tests
    ├── ship/session.rs  `git commit -m "..."` extraction against fixture JSONLs
    └── e2e.rs           End-to-end against tempdir git repo + tempdir HOME
```

### Runtime dependencies

| Crate        | Purpose                              |
| ------------ | ------------------------------------ |
| `clap`       | Arg parsing (derive feature)         |
| `serde_json` | JSONL line parsing                   |
| `regex`      | Branch ticket extraction             |
| `anyhow`     | Error propagation through `main`     |
| `dirs`       | `$HOME` resolution for `.claude/` |

### Dev-dependencies

| Crate         | Purpose                                  |
| ------------- | ---------------------------------------- |
| `assert_cmd`  | Invoke the compiled binary in tests      |
| `tempfile`    | Tempdir for git + HOME in e2e            |
| `predicates`  | Assertions on stdout/stderr in e2e       |

No `libgit2` / `git2` crate. Shelling out to `git` keeps the dependency surface
small and matches the user's actual git setup (hooks, signing config, etc.) for
free.

## Testing

### Unit
- `encode`: paths with `/`, `.`, mixed; particularly verify `.worktrees`
  encoding against the user's actual `~/.claude/projects/` layout
- `branch`: ticket regex matches, detached-HEAD case
- `session::extract_commit_messages`: single match, multiple matches in
  order, no-matches-from-prose, message containing brackets and punctuation

### Integration
- `session`: walk-back behavior against fixture JSONLs in `tests/fixtures/`,
  including "no commit-m blocks", "multiple commit-m blocks in one response",
  and "multiple responses each with one commit-m block"
- `dispatch`: argv routing — no args → ship; explicit `ship` → ship;
  `help` → help; unknown name with matching PATH executable → exec;
  unknown name with no PATH executable → exit 2 with the standard error.
  Use a tempdir + custom `PATH` to stage fake `tend-ship-foo` executables.

### End-to-end
One test in `tests/e2e.rs`:
- Tempdir as fake `$HOME` with `.claude/projects/<encoded>/<uuid>.jsonl` fixture
- Tempdir as the git working tree (one initial commit + one staged change)
- Tempdir as bare repo serving as `origin`
- Invoke `tend-ship` via `assert_cmd` with `--force`
- Assert: exit 0, commit landed on bare remote, message matches expectation
- Repeat with `--force --no-push`; assert commit landed locally but bare
  remote unchanged

No live network. No real Claude API. No real `~/.claude/`.

## Repository layout & lifecycle

- Lives at `~/programming/tools/tend-ship/`
- `main` branch
- First commit: this design doc
- Second commit: Cargo skeleton + `tend-ship --help`
- Subsequent commits: one module at a time, each with its tests
- Once functional + tested, decide whether to push to a personal GitHub repo
  and (optionally) open the conversation with jah2488 about path 2 / path 3

## Future work (not v1)

1. **Sibling-binary or shared-parsing PR to tend.** Once tend-ship works, open
   an issue with jah2488: "Open to a sibling binary that shares parsing logic,
   or to extracting a `tend-core` crate?" Drive the conversation with concrete
   working code.
2. **Claude self-invocation.** Update the global response-convention block 3
   to emit `tend-ship` (or `tend-ship -f`) instead of the four-line git block,
   so the commit subject is drawn from the live session automatically rather
   than re-derived from the JSONL.
3. **Lifecycle hooks.** A config file (`~/.config/tend-ship/config.toml`)
   declaring pre/post-commit and pre/post-push hooks that tend-ship invokes
   during the `ship` flow. Slots in alongside subcommand extensions but is
   focused on customizing the existing flow rather than adding new commands.
4. **Shared library crate.** If the extension ecosystem grows and multiple
   extensions need to parse the same JSONL files or encode the same paths,
   extract the relevant `ship/*` modules as a `tend-ship-lib` crate that
   extensions written in Rust can depend on. (Non-Rust extensions still get
   the env-var contract.)

## Open implementation questions

1. **Multi-text-block messages.** If an assistant message contains multiple
   text content blocks (rare), concatenate them with a space before scanning
   for `git commit -m "..."` substrings.
2. **Trailing-newline JSONL files.** Some JSONL writers add a trailing
   newline; the reverse iterator must tolerate a blank final line.
