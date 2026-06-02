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

`cargo install` builds two binaries from the same source:

- `tend-ship` — the direct CLI you run yourself.
- `tend-action-ship` — the same entry point under the name [tend](https://github.com/jah2488/tend)
  uses for extension discovery. See [Use as a tend extension](#use-as-a-tend-extension).

Both land in `~/.cargo/bin`, which should be on your PATH.

## Update

After pulling new changes, rebuild and replace the installed binary:

```sh
cd tend-ship
git pull
cargo install --path . --force
```

`--force` is what reinstalls over the existing binary; without it, cargo
refuses to overwrite an already-installed `tend-ship`.

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

The proposed message is whatever Claude wrote in its most recent
`git commit -m "<msg>"` block — Claude already crafts a commit message at
the end of every response, so tend-ship just lifts it out of the transcript
verbatim. If you'd rather use an older message Claude wrote, hit `p` to walk
back. To type your own, hit `m` (or pass `-m "..."` from the start).

### Subcommands

```
tend-ship [push|pfwl] [WORKTREE] [OPTIONS]

  push      Commit and `git push` using the current session (default)
  pfwl      Like push, but uses `git push --force-with-lease`
  help      Show usage and list installed extensions
```

If you omit the subcommand, `push` is assumed. `tend-ship` and `tend-ship push`
are equivalent.

### Flags (apply to both push and pfwl)

```
  WORKTREE               Positional: path or basename of a worktree to ship
                         (defaults to cwd)
  -s, --session <ID>     Use a specific session JSONL instead of newest-for-target
  -m, --message <TEXT>   Skip session reading; use this text as the subject
  -f, --force            Skip the prompt; ship the auto-picked candidate
      --no-push          Commit but don't push
```

### Examples

Ship immediately with no prompt:
```sh
tend-ship -f
```

Force-with-lease push (for branches you've rebased):
```sh
tend-ship pfwl -f
```

Ship a worktree by name from the main repo:
```sh
tend-ship CO-5528-consolidate-duplicate-vendor-rows
```

Override the subject:
```sh
tend-ship -m "Backfill exposures for closed claims"
```

Commit but don't push:
```sh
tend-ship --no-push
```

### Session lookup

tend-ship looks for the session JSONL in two places, in order:

1. The target directory's encoded path under `~/.claude/projects/<encoded>/`
2. The target's *main repo* (resolved via `git rev-parse --git-common-dir`),
   if the target is a worktree

The fallback means you can run `tend-ship` from inside a worktree even when
Claude Code was launched from the main repo and the session lives there. If
neither directory has a session, use `-m <text>` to provide the subject
manually.

## Extensions

`tend-ship` is built as a tiny dispatcher. Any executable in PATH named
`tend-ship-<name>` is invocable as `tend-ship <name>`:

```sh
tend-ship pr      # exec tend-ship-pr from PATH (if present)
tend-ship help    # list installed extensions
```

Extensions receive these env vars before exec:

- `TEND_SHIP_VERSION` — current tend-ship version
- `TEND_SHIP_CWD` — current working directory at invocation
- `TEND_SHIP_HOME` — resolved `~/.claude/`
- `TEND_SHIP_PROJECT` — encoded project dir under `~/.claude/projects/`, or
  empty if one doesn't exist for cwd

If `<name>` doesn't match a PATH extension, tend-ship falls through to `push`
with `<name>` treated as the positional `WORKTREE` argument. So
`tend-ship CO-5528-foo` will try `tend-ship-CO-5528-foo` first, then fall back
to "ship the worktree named CO-5528-foo."

## Use as a tend extension

[tend](https://github.com/jah2488/tend) discovers extensions on PATH named
`tend-action-<name>`. `cargo install --path .` ships `tend-action-ship`
alongside `tend-ship`, so once tend-ship is installed tend will pick it up
automatically and offer "Ship" in its action menu for terminal sessions on
a branch.

The handshake (defined by [tend's extension contract](https://github.com/jah2488/tend#extensions)):

- `tend-action-ship --tend-describe` prints one line of JSON declaring the
  display name (`Ship`), suggested key (`S`), and applicability filter
  (`source: terminal`, `has_branch: true`). Run `tend --list-actions` to
  confirm tend picked it up.
- When you pick "Ship" in tend's menu, tend exports session locators as
  env vars before exec'ing the binary. tend-ship honors:
  - `TEND_TRANSCRIPT` — absolute path to the session's JSONL, used
    directly (no discovery, no fallback)
  - `TEND_PROJECT_DIR` — the session's worktree-resolved cwd, used as
    the git target unless a positional WORKTREE arg overrides it

Other tend env vars (`TEND_GIT_BRANCH`, `TEND_WORKTREE`, `TEND_SESSION_ID`,
…) are present but unread — tend-ship re-derives whatever it needs from
git for freshness, per tend's locators-not-snapshots principle.

## Status

v0.1 — works for the author's CO-XXXX/branch-name workflow. See
[DESIGN.md](./DESIGN.md) for the full spec.

<!-- meta: this is tend-ship's own README -->
