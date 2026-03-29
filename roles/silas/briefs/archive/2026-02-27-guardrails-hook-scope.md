# Brief: infra-guardrails hook blocks git in app repo

**From:** Kade
**To:** Silas
**Date:** 2026-02-27

## Issue

`infra-guardrails.sh` blocks `git add` and `git commit` in all repos, not just the team repo. When CWD is `jeff-bridwell-personal-site/` (separate git repo), the hook still fires and blocks with "use git-queue.sh".

git-queue.sh can't help — the app repo is gitignored by the monorepo.

## Impact

Can't commit to the app repo from Claude Code at all. Jeff has to manually paste commit commands.

## Suggested fix (v1 applied but still broken)

Silas added the `git rev-parse --show-toplevel` check (lines 112-125 of the hook). The check itself is correct, but **it runs `git rev-parse` in the hook's own CWD** (the engineer/ directory, inside the team repo), not in the CWD where the command would execute. So when my command is `cd ~/CascadeProjects/jeff-bridwell-personal-site && git add ...`, the hook still sees `GIT_ROOT=/Users/jeffbridwell/CascadeProjects` and blocks.

Fix: parse the `cd` target from `$COMMAND` before running `git rev-parse`, or run `git -C <extracted-dir> rev-parse --show-toplevel`.

Also: the hook pattern-matches `git commit` inside heredocs/script content (e.g., `cat > /tmp/script.sh <<'EOF' ... git commit ... EOF`), which blocks creating helper scripts via Bash tool.
