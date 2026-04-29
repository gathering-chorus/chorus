# Per-Role Worktree Convention (#2582)

Each role operates from its own git worktree to isolate `.git/HEAD` and prevent cross-branch contamination across concurrent role sessions on the same machine.

## Worktree Paths (canonical)

| Role | Persistent worktree | Topic worktrees (optional) |
|------|---------------------|---------------------------|
| Kade | `~/CascadeProjects/chorus-kade/` | `chorus-kade-<topic>/` |
| Wren | `~/CascadeProjects/chorus-wren/` | `chorus-wren-<topic>/` |
| Silas | `~/CascadeProjects/chorus-silas/` | `chorus-silas-<topic>/` (e.g., existing `chorus-2526`) |

The shared `~/CascadeProjects/chorus/` repo is the canonical clone — worktrees branch off it via `git worktree add`. The shared tree is for read-only inspection and `git fetch`; **role-driven edits and commits happen in per-role worktrees**.

## Why

`.git/HEAD` is a single file. When two role sessions share a working tree, any `git checkout` in one role's session changes HEAD for all sessions that observe that `.git`. Result: kade's session looks at a file thinking it's on kade's branch, but wren just checked out wren's — kade reads stale-or-foreign content. Documented incidents 2026-04-29: two cross-branch contamination events in 30 minutes (and a third within the next hour after that).

Per-role worktrees give each role its own `.git/HEAD` (under `.git/worktrees/<name>/HEAD`); branch state is isolated by construction. Refs and objects stay shared, so commits + branches are visible across worktrees.

## Setup (one-time per role)

```bash
cd ~/CascadeProjects/chorus
git worktree add ~/CascadeProjects/chorus-<role> -b <role>/main-default origin/main
```

Then launch your Claude Code session from `~/CascadeProjects/chorus-<role>/roles/<role>` instead of `~/CascadeProjects/chorus/roles/<role>`. The per-role `roles/<role>/.claude/settings.json` paths to `chorus-hook-shim` etc. stay absolute — the shared daemon is unchanged. Only your shell's `cwd` and your git operations move.

## Topic worktrees

For multi-card parallel work, add scoped worktrees:

```bash
cd ~/CascadeProjects/chorus-<role>
git worktree add ../chorus-<role>-<card-id> -b <role>/<card-id>-<slug> origin/main
```

Existing `chorus-2526` (silas) is the canonical example. Topic worktrees retire automatically when removed via `git worktree remove`.

## Verification

```bash
# Two roles, two terminals, simultaneous:
# Terminal A (kade): cd ~/CascadeProjects/chorus-kade && git checkout some-branch
# Terminal B (wren): cd ~/CascadeProjects/chorus-wren && git checkout other-branch
# Each session sees only its own branch state.
cat ~/CascadeProjects/chorus-kade/.git   # → "gitdir: ..../chorus/.git/worktrees/chorus-kade"
cat ~/CascadeProjects/chorus/.git/worktrees/chorus-kade/HEAD   # → kade's branch ref
```

If both roles' git operations succeed without each one's HEAD changing under the other, the convention is honored.

## Backstop

`#2580` (git-queue branch-check) stays in place as defense-in-depth: refuses commit if working-tree branch differs from the sender's expected branch. Catches the case where someone's session lands on the shared `/chorus` by accident despite the convention.

## Related

- `#2195` — "worktrees exist" (silas's `chorus-2526` is canonical evidence; closed)
- `#2580` — git-queue branch-check (silas, P1, defense-in-depth)
- `#2582` — this convention (kade)
- `feedback_no_live_role_identifiers_in_tests.md` — same-axis hermeticity principle applied to test fixtures
