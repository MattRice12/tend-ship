# tend-ship

A single-purpose CLI that ends a [Claude Code](https://claude.com/claude-code)
session by composing a commit message from the session's transcript and
running `git add -A && git commit && git push`.

Inspired by [tend](https://github.com/jah2488/tend) — reads the same
`~/.claude/projects/<encoded>/*.jsonl` files. Where tend observes, tend-ship
acts.

## Install

Requires a [Rust toolchain](https://rustup.rs).

```sh
git clone https://github.com/MattRice12/tend-ship.git
cd tend-ship
cargo install --path .
```

After install, `tend-ship` is available anywhere `~/.cargo/bin` is in your
PATH.

## Usage

From inside a worktree where Claude Code has been running:

```sh
$ tend-ship
branch:    CO-5281/controller-parser
ticket:    CO-5281
changes:    4 files changed, 87 insertions(+), 12 deletions(-)
session:   ~/.claude/projects/.../9c2a….jsonl  (3m ago)
message:   [CO-5281] Move controller parsing into the shared module

Ship? [Y]es / [n]o / [p]revious / [m]essage override: y
✓ 8a3f912 [CO-5281] Move controller parsing into the shared module
```

The proposed subject is drawn from the most recent assistant message in the
session transcript that "looks like a summary" — short, not a question, not a
mid-task narration. If the auto-pick isn't the one you want, hit `p` to walk
back to an older candidate, or `m` to type your own subject.

### Flags

```
tend-ship ship [OPTIONS]

  -s, --session <ID>     Use a specific session JSONL instead of newest-for-cwd
  -m, --message <TEXT>   Skip session reading; use this text as the subject
  -f, --force            Skip the prompt; ship the auto-picked candidate
      --no-push          Commit but don't push
```

### Examples

Ship immediately with no preview prompt:
```sh
tend-ship -f
```

Override the subject entirely:
```sh
tend-ship -m "Backfill exposures for closed claims"
```

Commit but don't push (useful for stacked-PR flows):
```sh
tend-ship --no-push
```

## Subcommands and extensions

`tend-ship` is built as a tiny dispatcher around a `ship` subcommand. You can
extend it with any executable in PATH named `tend-ship-<name>`:

```sh
tend-ship pr      # exec tend-ship-pr from PATH
tend-ship test    # exec tend-ship-test from PATH
tend-ship help    # list installed extensions
```

Extensions receive these env vars before exec:

- `TEND_SHIP_VERSION` — current tend-ship version
- `TEND_SHIP_CWD` — current working directory
- `TEND_SHIP_HOME` — resolved `~/.claude/`
- `TEND_SHIP_PROJECT` — encoded project dir under `~/.claude/projects/`, or
  empty if one doesn't exist for cwd

Extensions own their own argument parsing, help, and exit codes.

## Status

v0.1 — works for the author's CO-XXXX/branch-name workflow. See
[DESIGN.md](./DESIGN.md) for the full spec.
