# Daily Ops Review — 2026-04-07

> Feeds Wren's morning summary. Each section: status · finding · action.

---

## 1. Hooks Health
**YELLOW** — `cargo check` on `chorus/platform/services/chorus-hooks` finishes clean (0 errors) but emits **2 warnings**: unused field `query` in `ContextSearchResults` (dead_code); 1 is auto-fixable.  
**Action:** `cargo fix --bin "chorus-hooks" -p chorus-hooks`; wire or remove `query` field in `ContextSearchResults`.

## 2. LaunchAgent /tmp Refs
**YELLOW** — **36 `/tmp` references** across 10+ plists in `chorus/proving/config/launchagents/` (hooks, clearing, ops, context-cache ×3, fuseki-perf, posture-capture, harvest-exporter, api). All are stdout/stderr log paths. Volatile — logs lost on reboot.  
**Action:** Migrate log paths to `~/Library/Logs/chorus/`; Silas to own (known pattern, not new).

## 3. CLAUDE.md Conflicts
**YELLOW** — `messages/claudemd/` fragment dir not found (path in spec doesn't match repo layout). Role CLAUDE.md files (engineer, architect, product-manager) updated **today** (Apr 7). Root `chorus/CLAUDE.md` last touched **Apr 3** — 4 days stale vs role fragments.  
**Action:** Sync root `chorus/CLAUDE.md` with current role content; clarify canonical fragment dir path in ops check config.

## 4. CSC Compliance
**YELLOW** — `/tmp/` found in **2 scripts** under `chorus/scripts/`:
- `inject-watcher.sh` — `INBOX_ROOT="/tmp/voice-inbox"` (hardcoded)
- `alert-runner.sh` — cooldown sentinel written to `/tmp/`

(`chorus/messages/TEAM_PROTOCOL.md` has 1 SSH example ref — doc only, not executable, acceptable.)  
**Action:** Move `inject-watcher.sh` inbox root to `$HOME`-relative path; make `alert-runner.sh` cooldown dir configurable.

## 5. Git Dirty State
**GREEN** — Working tree clean. All 7 role directories show no uncommitted changes.  
**Action:** None.

## 6. Stale WIP Cards
**YELLOW** — `backlog.md` header reads **Last updated: 2026-03-24** (14 days stale). WIP section still lists #1674, #1675, #1652. `next-session.md` (Apr 6 evening) states **"WIP: None — all cards shipped."**  
**Action:** Wren to sync `backlog.md` WIP section against Vikunja board first thing this session (board is source of truth per DEC-056).

## 7. Domain Context Freshness
**GREEN** — All 5 `domain-context-*.md` files (chorus, infrastructure, music, photos, seeds) committed **14 hours ago** (Apr 6 16:28). No file >7 days stale. No shipped-card / stale-context mismatch detected.  
**Action:** None.

## 8. Disk Delta
**GREEN** — Apr 5 → Apr 6 baseline: **954.6 GB → 905.9 GB (−5.1%)**. Disk shrank — likely cleanup. Well under 2% growth threshold.  
⚠ Data quality note: Apr 5 baseline has corrupt `percentUsed: 2` field (actual ~51%). Same bug flagged in Apr 5 review — still unresolved.  
**Action:** Fix `percentUsed` calculation in `chorus/platform/scripts/perf-baseline.sh` (carry-over from prior review).

---

_Summary: 4 yellow, 0 red. No blockers. Top actions: sync backlog.md WIP, fix perf-baseline percentUsed bug (carry-over), migrate /tmp log paths._
