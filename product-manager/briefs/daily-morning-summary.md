# Daily Morning Summary — 2026-04-01

**HEADLINE:** CI has been dark for 2+ days — all 4 platform packages lost `node_modules` and a chorus-sdk regression went undetected; restore test signal before anything new ships.

---

**OPS** 🟡 YELLOW (Silas review: 2026-03-29)
- 4 yellows, 4 greens. No reds.
- Top concern: 8 cargo warnings in chorus-hooks — 4 auto-fixable (`cargo fix --bin chorus-hooks`), 4 need manual review
- /tmp usage in 7 scripts + 6 plists is known/accepted. bedroom-heartbeat state loss on reboot not yet carded.
- `chorus-hooks/target/` (321 untracked) — confirm `.gitignore` covers build artifacts

**QUALITY** 🔴 RED (Kade review: 2026-03-31)
- **0 tests running** — board-client, workflow-engine, chorus-sdk, slack-bridge all missing `node_modules`
- **Regression:** chorus-sdk was GREEN (6/6) on 2026-03-29; now broken — day 3 of CI darkness
- App suite (jeff-bridwell-personal-site): directory not found — no lint/build data, day 3 persistent
- Fix: `npm install` in each of the 4 packages — 10 minutes, restores full signal

**YESTERDAY** — 2026-03-31 (~26 cards shipped)
- Silas: 7 cards — gate fixes, DEC-100 bash elimination (bash wrappers gone, Rust-only path locked)
- Kade: 12 cards — domain API, seed fix; gate test 39/39, pulse test 6/6 verified
- Wren: acp #1928 (pulse integration test fix, .sh wrappers removed from skills/callers); carded #1929, #1930
- Key decision: DEC-100 locked — no more .sh wrappers, Rust-only ops path permanent

**TODAY** (recommended order)
1. **Kade → `npm install` x4** — board-client, workflow-engine, chorus-sdk, slack-bridge. Nothing else before this.
2. **Silas → `cargo fix --bin chorus-hooks`** — clear 4 auto-fixable warnings; schedule manual review for remaining 4
3. **Silas → #1929** (gate smoke check on session boot) — carded and ready to pull
4. **Wren/Kade → app path** — confirm or remove `jeff-bridwell-personal-site` from check matrix; 3 days stale is noise

**BLOCKERS** — needs Jeff's attention
- 🔴 **CI dark 2+ days** — chorus-sdk regressed from GREEN and nobody caught it. `npm install` is the fix but it hasn't happened. If this persists past today, gate merges on test signal.
- 🟡 **App suite path** — 3 days of missing-directory yellow is masking real signal. Confirm path or drop it from the matrix.
