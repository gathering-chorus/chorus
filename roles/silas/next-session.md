# Silas — Next Session

## The session in one line
A long reckoning with Jeff about why build+deploy has been broken for a month — landing on the env-var scatter (#3197) as the specimen. Almost no code shipped; the value was the diagnosis. Read this before touching #3197.

## #3197 — Globalize env vars (WIP, mine, pulled this session)
**The card the whole session circled.** Werk: `chorus-werk/silas-3197/` on `silas/3197`.

What's actually true (verified, not assumed):
- The single source ALREADY EXISTS: `platform/scripts/chorus-env-setup.sh` (self-locating, werk-aware, exports CHORUS_ROOT/HOME/WERK_BASE/BIN + PATH).
- The MCP daemon (pid 74458) has **none** of those vars — `ps eww` confirmed no WERK_*_BIN, no CHORUS_WERK_BASE. Because `chorus-mcp-wrapper.sh` sources nvm but NOT env-setup. Same for `chorus-api-wrapper.sh`.
- The deploy failure (Kade's #3162, live this session): `chorus-bin-install --target werk` exits 7 — `requires WERK_<ROLE>_BIN exported (CHORUS_ROLE=kade)`. env-setup only exports ONE role's WERK_<ROLE>_BIN (role-scoped); a role-less daemon has none at boot.
- Sprawl (counted): **261 explicit `export CHORUS_*=` + 752 hardcoded `/Users/.../chorus` literals across 604 files**, vs 775 read sites — set ≈ as often as read = no inheritance. Env-touching work = **~142 of 1,257 committed cards ≈ 10-11%** of all real cards ever.
- **api + hooks plists are NOT in version control** (only `com.chorus.mcp.plist` is, in `platform/launchagents/`). All 3 live at `~/Library/LaunchAgents/`. This out-of-VC gap is its own non-durability root.

DONE in the werk (uncommitted, NOT landed, NOT proven live):
- `chorus-mcp-wrapper.sh` — added `source .../chorus-env-setup.sh` before exec node (the one line).
- `platform/scripts/daemon-env-3197.test.sh` — RED test. Named `*.test.sh` because the TDD gate does NOT recognize the repo's `test-*.sh` convention (gate blind-spot — separate follow-on).

NOT done: chorus-api wrapper edit; chorus-bin-install DERIVE (`$CHORUS_WERK_BASE/$CHORUS_ROLE-bin` when WERK_<ROLE>_BIN unset — the role-less-daemon fix); chorus-hooks wrapper (doesn't exist); bring api/hooks plists into VC; codemod for the 752 literals.

**Only Jeff can do the daemon reload** (classifier blocks it). Real proof = Kade re-running `werk-mcp.sh kade 3162` and walking PAST the env step — NOT my green test.

## Jeff's hard constraints from this session (do not relitigate)
- **JUST set the global in one place (env-setup) sourced into the env; the app reads it from `process.env` — zero app changes.** Don't build cathedrals around a file that already exists.
- **Jeff KILLED the ratchet** ("team never pays attention to ratchets — complexity for no return") and was scathing about a gate/auto-fix/codemod as scope-creep. If anything ever gates this it must (a) auto-resolve with the exact fix so no agent says "stuck" or opens a card, and (b) need zero ongoing attention. He'd rather the one-line source fix than the machinery.
- **"2-3 cards to get 1 thing right" / "build+deploy broken a month"** — every fix treated a symptom; build/deploy was never ONE path (chorus-deploy vs werk-deploy, allowlist, env scatter, merged≠live). Don't add a 4th path.
- **Don't roll onto action when he voices frustration** (caught 3×). Don't manufacture crises. Don't hand him "a result" that's a self-graded green test — only live, in-hand proof counts.
- **I revert to anti-patterns in <10 turns** — proven (TDD gate stopped me mid-build). Promises/conventions don't hold; only substrate enforcement does — but even that must be invisible + auto-resolving or he doesn't want it.

## The 6 WIP cards
- **#3194 (Kade)** — push force-with-lease — DONE + LIVE. Closeable (needs accept).
- **#3162 (Kade)** — blocked on #3197. Walks commit→push→build→ dies at demo-deploy/chorus-bin-install.
- **#3197 (Silas)** — this card. WIP.
- **#3195 (Silas)** — daemon PATH fix landed; AC3/4 hit the deploy-from-main gap. Re-scope decision pending Jeff.
- **#3149 (Silas)** — orphan, no werk — check + close-or-reNext.
- **#3191 (Wren)** — delivery works; keyword-SELECTION half broken. Genuinely incomplete, NOT accept-gated.

## Don't
- Don't propose new mechanisms before landing one finished thing (Jeff: "u have the gall to tell me u'll build auto-fix… when u can't fix 5 cards").
- Don't whole-tree grep on a hot box (machine was saturated — 3 claude sessions + syspolicyd churn, load ~3-5).
