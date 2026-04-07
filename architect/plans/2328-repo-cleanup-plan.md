# #2328 — Repo Cleanup Migration Plan

## Problem
Three AI roles share one git working tree. Commits collide, `git checkout -- .` wipes everyone's work, 31% of commits are overhead. The repo is named `gathering-team` but only contains `chorus/`. Four symlinks create a fake monorepo illusion.

## Current State
```
CascadeProjects/              ← git root (gathering-team repo)
├── .git/
├── architect/                → symlink to chorus/platform/roles/silas
├── engineer/                 → symlink to chorus/platform/roles/kade
├── product-manager/          → symlink to chorus/platform/roles/wren
├── messages/                 → symlink to chorus/
├── chorus/                   ← 99.7% of all tracked files
├── jeff-bridwell-personal-site/  ← SEPARATE repo
├── shared-observability/         ← SEPARATE repo
├── wordpress-blog/               ← SEPARATE repo
└── (dead weight: architect-old/, chorus.bak.*, etc.)
```

## Target State
```
CascadeProjects/
├── chorus/                   ← git root IS chorus (flattened, renamed)
│   ├── platform/
│   │   ├── roles/silas/      ← canonical Silas dir
│   │   ├── roles/kade/       ← canonical Kade dir
│   │   └── roles/wren/       ← canonical Wren dir
│   ├── ...
│   └── .git/
├── jeff-bridwell-personal-site/  ← unchanged
├── shared-observability/         ← unchanged
└── wordpress-blog/               ← unchanged
```

No symlinks. No duplicate role directories. No dead weight.

## Migration Steps

### Phase 1: GitHub Rename
- **Blocker:** WJeffBridwell/chorus already exists (stale, last push Mar 22)
- **Action:** Delete or rename the stale repo, then rename gathering-team → chorus
- **Risk:** GitHub auto-redirects old URLs. All local remotes need `git remote set-url`

### Phase 2: Delete Dead Weight (540MB)
| Path | Size | What it is | Safe? |
|------|------|-----------|-------|
| architect-old/ | 53MB | Abandoned old role dir, Mar 30 | Yes — no .git, not tracked |
| chorus.bak.1774797037/ | 462MB | Full backup with own .git, Mar 27 | Yes — backup, not referenced |
| gathering-team/ | 8KB | Empty stub dir | Yes — empty |
| chorus-sdk/ (top-level) | 25MB | Stray SDK copy, no own .git | Yes — real SDK is at chorus/platform/chorus-sdk/ |
| doit.sh, t.sh, fuckyousilas.sh, Dockerfile | <1KB each | Loose scripts at root | Yes — not tracked |
| .env.bridge | <1KB | Env file at root | Check — may contain secrets |

### Phase 3: Resolve Split-Brain Role Directories
Each role has TWO directories in chorus/:
- `chorus/architect/` (slim, 8 items) vs `chorus/platform/roles/silas/` (full, 40+ items)
- `chorus/engineer/` (slim) vs `chorus/platform/roles/kade/` (nearly identical)
- `chorus/product-manager/` (has extra HTML) vs `chorus/platform/roles/wren/` (core files)

**Decision needed:** Which is canonical? The symlinks point to `platform/roles/<name>` which is where sessions actually run. Recommendation: `platform/roles/<name>` is canonical. Merge any unique files from `chorus/<role>/` into it, then delete `chorus/<role>/`.

### Phase 4: Remove Symlinks
Delete these 4 symlinks from CascadeProjects/:
- architect → chorus/platform/roles/silas
- engineer → chorus/platform/roles/kade
- product-manager → chorus/platform/roles/wren
- messages → chorus

Replace with `additionalDirectories` in each role's `settings.local.json` using absolute paths.

### Phase 5: Flatten chorus/ to Repo Root
```bash
git clone gathering-team chorus-new
cd chorus-new
git filter-repo --subdirectory-filter chorus
```
This makes chorus/ contents the new root. All paths shorten by one level.

### Phase 6: Update Hardcoded Paths
- `state_paths.rs` — REPO_ROOT const (imported by ~10 Rust modules)
- `git-queue.sh` — REPO_ROOT derivation and lock file path
- `acp/SKILL.md` — `cd /Users/jeffbridwell/CascadeProjects/chorus &&`
- `reboot/skill.md` — same
- `settings.local.json` (x3) — additionalDirectories
- `infra_guardrails.rs` — TEAM_REPO_ROOT const
- LaunchAgent plists — WorkingDirectory and script paths
- 16 shell scripts with cross-role path references

### Phase 7: Verify All Roles Work
- Each role can boot (session-start hook succeeds)
- Each role can commit (git-queue.sh works)
- Each role can push (git pull --rebase && git push works)
- Each role can deliver briefs (write to other role's briefs/ dir)
- Nudge delivery works (osascript injection)
- Board operations work (board-ts commands)

## Risks
- **All 3 role sessions break during migration** — must coordinate timing
- **git filter-repo rewrites history** — force push required
- **LaunchAgents reference old paths** — services fail until plists updated
- **Rust binary needs rebuild** — state_paths.rs changes require cargo build + launchctl kickstart

## Open Questions
1. What to do with the stale WJeffBridwell/chorus repo on GitHub?
2. Which role directory is canonical — chorus/<role>/ or platform/roles/<name>/?
3. Should .env.bridge be preserved or deleted?
