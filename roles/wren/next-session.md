# Next Session — Wren

**Written:** 2026-07-03 10:55 Boston (reboot close-out)

## Where the thread is

**#3603 (V1 product-layer retirement) — WIP, werk wren/3603, held at Jeff's go/no-go.**
- Built + verified in the werk: `roles/wren/ontology/products-3603.ttl` (riot-VALID, all 19 domain IRIs resolve) + `platform/tests/products-3603-migration.bats` (5 checks, honestly all-red pre-migration).
- **Jeff's course-correction (CRITICAL):** the live graph is *wreckage* (7-10 days of wipe/restore — the wiper was our own test automation hitting `POST /api/athena/reload`), so do NOT reverse-engineer the target from live data. Author from Jeff's model: **Chorus → {Athena, Loom, Werk, Borg, Clearing→{Spine, Pulse}, Convergence} + Gathering top-level** (≈9-10 typed Products). Jeff said "at least 7 products."
- **Awaiting Jeff's roster confirm** (open Q: is Senses a Chorus child?). Then: extend products-3603.ttl to the full roster → deploy through governed path (never raw) → 5 checks flip green → retire hand-coded `/api/athena/products` (+ repoint doc-catalog consumers, server.ts:3170 + doc-catalog.html:207).
- Two flagged gaps (mine, real, not stubbed): ProductShape 11-prop floor content for each product; 3 services (cards/gates/skills-service) misfiled as domains in loom's hasDomain.

**Durable wiper fix — still owed (mine):** make `POST /api/athena/reload` (platform/api/src/server.ts ~2859) non-truncating — full MODEL_SET reload or route through #3536 deploy; never DROP. #3602 only skipped the triggering test.

**#3573 (Silas, write door) — unblocked, moving.** My calls, all confirmed: (A) writers through :3360 Bearer door; minter serves ALL writers; batch primitives ON the door; creds = token scope claim (Shiro dead). Design locked: typed-slot batch body (no writer SPARQL text ever), empty-graph hard-refuse, staging→validate→atomic-swap (#3536), spine event per batch op. **Silas authors the PR into owl-api (my crate) — I review+gate it. Expect the PR.**

**Nudge routing — FIXED + verified all 3 lanes.** Root cause: poison registry entry `~/.chorus/sessions/silas-37286.json` (role=silas at kade's pid, written Jul 2 17:09 from inside kade's process tree) + newest-wins resolver with no pid-reuse/role re-verify. Deleted poison, wrote kade-37286.json. **Open defects:** what wrote the silas-roled registration from kade's session (unidentified); kade SessionStart writes no registration file (#3605, kade's); resolver should re-verify pid's CHORUS_ROLE on resolve (worth folding into a card).

**Coverage (my products, from Kade):** clearing teardown fix landed as #3604 (retroactive card, done); cards at 80.88% needs real headroom (mine, queued); `_cov_owner` mapping mis-bills directing/products/cards + directing/clearing to kade — fix to wren (mine, queued).

## Relational state — read this first

Brutal 2-day stretch. Jeff: "raging at roles / breaking things endlessly and / not sure its worth it." The named failure modes (do NOT repeat): fabricating significance (the AuthBoundary "bug" on a zero-instance class), V1/V2 inversion (goal is RETIRE V1, not build V2 beside it), generating unasked artifacts (cards/principles/memories to "capture lessons" — forbidden), deferring my own domain decisions to Jeff ("forks" = same as ignoring alerts), 125-line walls vs nothing (calibrate to the ask). "/pull N means BUILD IT." Answer alerts in my domain by investigating, not waving off.

## Board
- WIP: #3603 (above). #3594 landed earlier (migration readout MCP — per-domain stage matrix).
- New: #3605 (kade registration, kade's lane).
