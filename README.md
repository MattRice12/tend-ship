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

## Tend integration

tend-ship also implements a speculative "invoked by tend" contract for the
day tend grows an extension API. Set these env vars and tend-ship skips
its own session discovery:

| Variable             | Meaning                                       |
| -------------------- | --------------------------------------------- |
| `TEND_SESSION_JSONL` | Path to the chosen session's `*.jsonl` file (also the discriminator) |
| `TEND_SESSION_CWD`   | Original cwd Claude was launched from         |
| `TEND_VERSION`       | Version of the invoking tool (informational)  |

tend-ship reads the JSONL fresh from disk on every invocation — even
under extension mode — so it stays correct if tend hasn't refreshed its
view recently. See [DESIGN.md](./DESIGN.md#tend-integration-proposed-inbound-contract)
for the full contract.

## Status

v0.1 — works for the author's CO-XXXX/branch-name workflow. See
[DESIGN.md](./DESIGN.md) for the full spec.
