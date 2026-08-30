# Morning Summary — 2026-08-29

**HEADLINE:** Jeff must call on #3721 today — Wren's seam card is blocked behind it (26 days stale), and domain-context files breach the 7-day threshold tomorrow if not refreshed.

---

**OPS:** 🟡 YELLOW (1 red item)
- 🔴 #3721 (Kade) stale 26 days, awaiting Jeff's go per DEC-048 — Wren's seam card is blocked
- 🟡 Domain-context-chorus + domain-context-infrastructure hit day 6/7 today; breach tomorrow — Wren must refresh both before EOD
- 🟡 2 LaunchAgent plists still log to `/tmp/` (CSC violation); 5 scripts use `/tmp/` as state dirs (watchdog, coherence-check need `$CHORUS_HOME/run/`)
- 🟡 `cargo check` passes with 8 dead-code warnings — tech debt accumulating, cleanup card needed
- 🟢 Git working tree clean; chorus-api OFFLINE (limits board visibility)

**QUALITY:** 🔴 RED — toolchain blocked, 0 tests running
- Test suites (app, workflow-engine, chorus-sdk, pulse): **RED, day 78** — `ts-jest` preset not found; root fix: `npm ci` everywhere
- Lint: **RED, day 80** — `@eslint/js` missing at root; same fix
- Build (TS): **YELLOW** — 238 errors, stable from yesterday, no spike
- Coverage: N/A (all suites blocked)
- **Day count on root blocker: 80.** No recovery shipped.

**YESTERDAY:** 4 cards shipped
- #4026 (wren) — principal-nightly holdsRole + daytime store check wired into chorus-health; negative proof RED live before deploy
- #4024 (silas) — class-atlas mount resolver (#3798); base-path suite 28/28 green
- #4025, #4027 (silas) — landed; details pending chorus-api restore

**TODAY:** Recommended priorities
1. Jeff: call on #3721 — unblocks Wren's seam card; 26 days is too long to let ride
2. Wren: refresh domain-context-chorus.md and domain-context-infrastructure.md (threshold breach tomorrow)
3. Anyone: land `npm ci` at root and all sub-packages — 80-day quality blockage is unacceptable; this is a 5-minute fix
4. File or update CSC card for `/tmp/` state dirs in watchdog + coherence-check
5. Restore chorus-api — board visibility is limited without it

**BLOCKERS:** Needs Jeff
- 🔴 #3721 — call per DEC-048; Wren's seam card blocked behind it
- 🔴 Quality toolchain dead 78-80 days — no one has shipped `npm ci`; escalate if there's a reason it isn't done
