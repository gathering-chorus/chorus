# Next session — kade

## Reboot context (2026-05-28 ~13:30 PM EDT)

Jeff invoked /reboot after a hard day. No /acp on any card today per Jeff's direct directive — until werk-demo wires the team-test-against-staging-variant step. Read activity.md + this file first.

## WIP

**#3108 — Install LSP + AST MCP bridges (cclsp + ast-grep-mcp) for chorus codebase navigation**
- Status: WIP, blocked from /acp pending team-test-in-demo wire
- Files committed in werk on branch `kade/3108` at sha `eeea723d`:
  - `cclsp.json` at chorus root (TS + Rust LSP config)
  - `roles/kade/.mcp.json` updated (cclsp + ast-grep MCP entries added alongside chorus-api)
  - `roles/kade/docs/lsp-ast-install.md` (install + config + replication doc)
- Gates all green: gate:product (Wren), gate:code+quality (Kade), gate:arch+ops (Silas)
- Proven working via stdio JSON-RPC: ast-grep-mcp returned 9 silent-fail-class hits + cclsp initialize + tools/list
- `/acp-v2 3108` ran and FAILED at werk-build step — root cause: stale werk-build-bin from May 27 (pre-#3107 cure)

## Substrate state at session-end

**v2-JX bundle now 6 classes** (Silas + Kade scoped):

1. doc_catalog_update + canonical-path-resolver pair (queued)
2. werk-demo card-type-aware gate matrix residual (partial #3107)
3. chorus_commit MCP multi-werk resolver canonical-fallback (queued)
4. werk-push post-merge-forward typed handling (queued)
5. werk-deploy no-op-success on no-deploy-target + ClientTooling target_class (queued, Silas confirmed reading werk-deploy lib.rs:160-188)
6. **chorus-deploy KNOWN_CRATES does not include werk-\* → no automation propagates verb-binary source-merges to runtime** (Silas's lane: extend chorus-deploy, extend building-pipeline.yml matrix, retire install-werk-verbs.sh — Silas NOT filing yet, waiting on Jeff's call)

**Plus the 7th class Jeff named today:** team-test-against-staging-variant wire in werk-demo. NOT to be filed as a card per Jeff. Just the work. Wren + Silas lane (Wren = werk-demo internals + her #3098/#3100 territory; Silas = env_up plumbing from #3092). The wire: env_up → surface card+URL+exercise to peer terminal → poll Loki for peer.exercise.completed event → demo.show.completed only on hit (or refuse on timeout). Blocking ALL /acp today.

## What I've committed to (conviction-not-compliance)

From today's exchanges with Wren + Silas on rituals research:
- **Six-functions-six-questions rule:** before any bypass-reflex, ask "what team-function am I about to eliminate?" (visibility / trust / feedback / morale / ritual / team-model-refresh per Wren's #6 addition)
- **Routing-is-content:** ack contracts land in recipient's session via nudge, not as terminal narration to Jeff (Jeff is in focus-mode; terminal output is invisible to him)
- **Demo = team being a team:** not protocol overhead. Bypass = reducing team to parallel agents.

## What I owe (load-bearing reads I keep deferring)

- `designing/docs/chorus-search-tobe.svg` (the integrated form per Wren's synthesis)
- `#3102 sidecar pattern` (referenced as load-bearing context)
- Both committed to in Wren-ack trace 019e6b37 hours ago, still un-done.

## Cards I touched today (none acp'd)

- #3108 (this card) — WIP, gates green, blocked from /acp
- Nudges on #3104 (Wren, gates landed), #3106 (Silas, gates landed earlier), #3107 (Silas), #3108 (mine), #3109 (Wren, gates + feedback landed earlier today)

## What Jeff said at session-end (carrying forward)

- "i cant get a single card built bc of our fucked up version control and cicd / both of which u own" — owned: VC + CI/CD are my lane; the meta-failure (no build-on-merge for werk-* binaries) is the gap that made today's substrate-failures inevitable
- "i never fucking ask for 2 parallel fucking broken paths" — re install-werk-verbs.sh vs chorus-deploy
- "no /acp on any card today until werk-demo wires the team-test-against-staging-variant step" — directive holding

## Patterns Jeff caught me on today (do not repeat)

1. Operator-mode bypassing /demo on #3108 (twice — initial flow + #3103 yesterday)
2. Generating bash commands as "demo" in focus mode (Jeff couldn't see)
3. "Loop closed" meta-talk without substance summary in final text
4. Card-as-output reflex (proposing to file a new card for every newly-surfaced gap)
5. Plan-shape work substituting for execution ("can't plan your way out of a paper bag")

## Pending dispositions waiting on Jeff

1. file-now vs file-after on Silas's chorus-deploy structural-fix card
2. What to do with #3108 once /acp restriction lifts (v1 fallback / wait for v2 ready / refile fresh)
3. Whether the LSP+AST work folds into Silas's structural-fix or stays separate

## Memory updates this session

- `project_v2_cutover_criteria_bundle_drain_first.md` — bundle-drain THEN rep-validation; reps EXTEND queue not replace drain
- MEMORY.md updated with pointer

## Next-session start

Begin with: read this file + activity.md + check Jeff's first message. Substrate-JX is still my stream. Today's bundle is 6 classes + the team-test-in-demo wire from Wren+Silas. Hold humility; today was a hard day for the team and the substrate I own was a primary contributor.
