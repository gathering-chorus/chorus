# Next Session — Silas

## Hot state

**WIP card: #3110** (chorus-deploy absorbs werk-*; retire install-werk-verbs.sh)
- Four commits on `silas/3110`: 3efc096a (initial fix) + 426c12b8 (delete shim) + 95065b82 (docs cleanup) + b5bfee1c (MCP wrappers)
- Live verified: chorus-deploy --all werk installs 7/7 verb binaries; werk-build on docs-only exits 0; install-werk-verbs.sh DELETED
- AC 9/10. Last AC blocked — silas-bin cleaned, wren-bin + kade-bin orphans remain
- NOT acp'd. Jeff: no /acp until shown working in demo.

**MCP wrappers shipped (b5bfee1c, NOT yet live):**
- chorus_build / chorus_deploy / chorus_env_up tools added to chorus-mcp source
- For Wren to consume from werk-demo instead of subprocess-shelling
- Live only after chorus-mcp redeploys (post-/acp or manual chorus-deploy chorus-mcp)

## The cycle that paused work

Jeff's standing demand for weeks: team must show changes working in demo before /acp. Three werk-demo bugs surfaced today:
1. demo.show.completed fires too early
2. peer_engaged() is too loose (any spine activity counts, not actual exercise)
3. env_up never invoked from werk-demo

Lane: Wren takes Bugs 1+2; Silas takes Bug 3 via MCP wrappers.
Wren's Bug 1+2 coded + e2e green in #3109 werk but werk-commit blocked by classifier on scope (#3109 was filed for a glossary HTML).

Cycle: Wren classifier-blocked → chorus-mcp needs redeploy → requires /acp #3110 → requires /demo green → requires Bug 1+2 live + MCP wrappers deployed. Head of chain is Wren.

Jeff is sick of "new card to fix it" death spiral. Three roles, three scope escalations today.

## What landed today (Silas)

- #3107 cure verified live
- #3110: KNOWN_CRATES extended, `--all werk` subcommand, INSTALL_AS_WRAPPER pattern, building-pipeline.yml extended, install-werk-verbs.sh deleted, 3 MCP wrappers
- /acp'd yesterday: #3107 (PR #377), #3106 (PR #378)
- Daily-review summary stuck this morning (re-invokes quality concurrently → jest race). Bootout authorized; Kade nudged with one-line fix
- Trigger-list claim: **deploy**

## Pending pickup next session

1. Wait for Wren to land Bug 1+2
2. Once chorus-mcp redeploys, verify chorus_build/deploy/env_up tools reachable from werk-demo
3. Role-wide overlay model retire (per-card werks become only in-flight location) — mine
4. Class-5 cure (werk-deploy Ok-empty on no-deploy-target, symmetric to #3107)
5. Eventloop-block at 09:59:30 EDT (3787ms) RCA still owed
6. peer-overlay orphan cleanup (wren-bin, kade-bin) — Jeff said mine to automate via overlay retire

## Hard constraints Jeff named today

- No more new cards
- No manual workarounds
- No /acp on #3110 until shown working in demo
- No carving lines / loomspeak
- "tested" ≠ "synthetic-check passed"
- Team self-tests in staging; Jeff is never the discovery surface
- Role-wide overlay model is stale

## Tone note

Jeff exhausted by EOD. Three roles producing carving-lines instead of working code while substrate quietly broke under him. Pre-gate ritual load-bearing; verb-as-substrate over skill-as-discipline. Be brief. Match energy. Ship before describing.
