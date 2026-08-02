# Current Work

Last updated: 2026-08-02 06:20 Boston

## WIP
- **#3721** "no failures period, no non-determinism" — 15 commits on `kade/3721`, **CI 39/39 green (0 skipped) on `662dfc2e6`**. Wren reviewed and approved; Silas took his lane. **Waiting on Jeff's go to land** (DEC-048 — not self-accepting).

  The unlock was a CI gate, not a test: four jobs (ESLint ratchet, tsc, clippy, clippy ratchet) were gated `if: github.event_name == 'schedule'`, so every `workflow_dispatch` skipped all four **and still reported the run as success**. "Green" meant a third of the static analysis never ran. Opening them to dispatch immediately surfaced a real ESLint ratchet breach that had been sitting on main.

  Three were genuine shipped defects, not stale tests:
  - **`append_log` silently dropped spine events.** `tokio::fs::File` buffers and does not flush on drop, so `write_all` returned `Ok` while the bytes never reached the OS. That is #3278's "199 of 200 lines, zero corrupt" — a silent-drop problem wearing corruption's clothes. One `f.flush()`. (I first blamed fd pressure and said so in a commit; my own unit test disproved it on CI and I superseded that explanation rather than leaving it standing.)
  - **The #1846 empty-context alarm could not fire.** It counted `content.lines()` at the end of the function, where `content` already includes principles + Athena tree + next-session notes — those clear the 10-line threshold alone. Staleness was mtime-only, so an empty-but-fresh cache was never rebuilt. Verified: zero-byte cache → no error, no repair, still empty 10s later. A role would boot on nothing, silently. Now 0 → 9688 bytes.
  - **The cards "1/529 flake" was a live-production hazard.** `client-3600.test.ts` stubbed `.api`, but `fetchAllTasks()` reads the #3625 short-TTL disk cache first. Cache warm → 3662 real tasks, fails. Cache cold → passes, then writes its two fake tasks *into the live shared cache*, so every real `cards` invocation sees a 2-task board for the TTL window. Fixed via `CARDS_CACHE_DISABLE`; verified 6/6 deterministic AND live cache byte/mtime-identical after the run. Wren owns the seam cleanup.

## Waiting
- **Jeff's go on #3721**, plus his call on the two debt items below. Wren is holding her own seam card behind this one.

## Open debt — deliberately left red rather than faked green
- **`coverage:clearing`** — all 426 tests pass; `src/server.ts` is 73.04% stmts / 56.72% branches against 80/60 floors. Needs real tests, not a lowered floor.
- **`npm:jeff-bridwell-personal-site` 2/4671** — 83 API endpoints have no swagger tags/summaries against a 116 ratchet. Auto-generating 83 summaries would satisfy the ratchet with documentation theater.

## Context
- Canonical 03:00 nightly (2026-08-02) was 4 red: `lint:chorus`, `npm:cards`, `coverage:clearing`, `npm:jeff-bridwell-personal-site`. Landing #3721 clears the first two.
- Silas owns the remaining chorus reds (the hardcoded-LAN-IP nine, service-design suites, coverage floors) and is triaging against the canonical run, not my werk snapshot.
- **My error, logged:** my first nightly run from the werk wrote to the shared `~/Library/Logs/Chorus/nightly-suites.log` and fired a team-wide "34 red" alert at Jeff. Silas caught it. Use `NIGHTLY_LOG_PATH=<werk-local>` for any werk validation run; he's guarding the default.
- Prior WIP #3663 / #3662 (2026-07-22) both closed out; nothing carried.
