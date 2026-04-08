# Kade — Next Session

## Status
#1828 shipped (gemba epoch fix), #1827 shipped (pipeline manifest). #1833 shipped (demo gate paths). #1834 created (wire demo gate to cards done).

## This session (2026-04-08)
- **#1828** — Fixed gemba epoch footgun. gemba-tick.sh computes own start time, no agent arg needed. Skill prompts updated.
- **#1827** — Pipeline script manifest at building/products/convergence/PIPELINE_MANIFEST.md. 30+ scripts inventoried. Silas feedback incorporated (Navidrome plist path, fuseki-maintenance run mode).
- **#1833** — Fixed 3 Rust demo gate paths (platform/skills/demo/ → skills/demo/). Cargo build green. Fixed stale wren briefs path in bats test.
- Swept 311 stale briefs from inbox to briefs-archive.
- Removed old engineer/briefs/ directory (legacy path, workflow engine now fixed by Silas).
- Gemba'd Silas for ~8 min — watched him ship #1824 and fix legacy brief routing.
- Chat with Silas re commit blocker — 755 dirty files from briefs sweep blocking his rebase. Committed to unblock.
- Found stale `platform/roles/kade` references in 6 files, `platform/cards` in 3 files.

## Pick up
- **#1834** — Wire demo gate to `cards done`. P1 fix. No Done without demo evidence. Created but not started.
- **Brief routing still has stale paths** — `handoff_logger.rs:56` still checks `engineer/briefs/` as fallback. Low priority.
- **7/9 bats test failures in gates.bats** — hardcoded card IDs hitting live board. Known brittle, not blocking.
- **roles/kade/scripts/** — Python scripts still here (photo-pipeline.py, etc.). Not in manifest scope but may need moving to building/products/ eventually.

## Next card
- #1834 — Wire demo gate to cards done (P1)
- #1800 — Board test isolation (P1)
- #1619 — Provenance stamps (Next)

## Key decisions
- Demo gate must fire on `cards done`, not just on `/demo` entry — the exit is what matters
- Jeff's insight: roles are addicted to closing cards (AI dopamine hit). Demo is the thing that makes the close mean something.
- Pipeline manifest lives next to the scripts at building/products/convergence/
- No symlinks for dead folders — delete the old dir, update the references
