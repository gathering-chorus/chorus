# Wren — Next Session

## Session close 2026-07-04 03:08

Today was the day the system's biggest lies got named and two cards landed on Jeff's go. Shipped #3603 (V1 product-layer retirement: SubProduct class/instances gone from source, the 10-product roster authored on the committed product-* convention with real floors, three hand-coded product endpoints deleted, consumers repointed to owl-api — PR #726/#729-adjacent, accepted) and #3607 (the Clearing wedge: three separate code paths were reading the ENTIRE 122MB chorus.log — the clearing 3s poll at 2.4s/request, the per-prompt nudge fold, the spine handler — because rotation existed since #1622 but was never scheduled; tail-reads everywhere + rotation wired into the hourly agent; prod /api/stream now 9-64ms, was 1650ms — PR #729, accepted, verified live). Also review-gated Silas's #3573 write door (merged), root-caused the werk git-identity mis-stamp (worktrees share .git/config; --worktree fix is Silas's card), fixed the poison silas-52950 registry entry live with Jeff's ok, and filed #3608 (session-registration integrity, P1, mine — Jeff explicitly put roles+nudge+clearing in my domain after a misrouted nudge dragged Kade into a delete he had nothing to do with; that confusion cost real trust mid-morning, don't repeat it: no relay-authorized live-graph writes, and stop narrating role names into threads they aren't in). Still hot for tomorrow: pull #3609 (owl-api /batch 4KB body truncation — mine, blocks the #3603 live migration), then Silas mints and applies the 53-DEL/352-INS migration and products-3603-migration.bats flips green — that's the moment the graph finally matches the tree Jeff drew; the event-loop-block card is still at the bouncer awaiting Jeff's approve, and rotation's first hourly tick should have shrunk chorus.log — verify it actually fired.

## WIP (still in progress)

-   3493  Author the Clearing product tree in the model (clearing → spine/pulse → domains) [Wren|P2|domain:chorus|type:new|sequence:clearing|origin:reflective|subproduct:clearing|chunk:coherent-model]
-   1818  Seeds: close-the-loop — Jeff sees what role did with his seed [Wren|P2|gathering|chunk:memory|type:new|origin:reflective|sequence:seeds]
-   2145  Stop-the-line PostToolUse hook — block turn when a tool errors, force resolve/card/defer [Wren|P1|domain:chorus|type:new|sequence:loom|origin:reactive|subproduct:loom|chunk:loom-authoring]
-   2159  Chorus-native board — value-stream → domain → sub-product → work-unit hierarchy replacing flat Vikunja labels [Wren|P1|domain:chorus|type:enhance|sequence:clearing|origin:reflective|subproduct:clearing|chunk:coherent-model]
-   2183  Kade session-start parity — align with Silas/Wren 5-beat opening [Wren|P1|domain:chorus|type:fix|sequence:loom|origin:reactive|subproduct:loom|chunk:loom-authoring]
-   2246  Fix close-out.sh WIP/Next derivation — over-matches role name in list output [Wren|P3|domain:chorus|type:fix|sequence:loom|origin:reactive|subproduct:loom|chunk:core-reliability]
-   2247  session-close.sh git-queue invocation missing -- -m marker [Wren|P3|domain:chorus|type:fix|sequence:loom|origin:reactive|subproduct:loom|chunk:core-reliability]
-   2569  Self-attest path for narrow card shapes — judgment layer only, machine layer always fires (#2561 child 4/5) [Wren|P2|domain:chorus|type:new|sequence:loom|origin:reflective|subproduct:loom|chunk:loom-authoring]
-   2570  Distribute gate-product — split AC-verification (peer) + experience-integration (Wren/Jeff) + no-self-gate rule (#2561 child 5/5) [Wren|P1|domain:chorus|type:new|sequence:loom|origin:reflective|subproduct:loom|chunk:loom-authoring]
-   2576  Clearing inject without frontmost-set — avoid focus-theft when Jeff is on Mac and sends from phone [Wren|P3|domain:chorus|type:fix|sequence:werk|origin:reactive|chunk:core-reliability]

## Next (queued)

-   3493  Author the Clearing product tree in the model (clearing → spine/pulse → domains) [Wren|P2|domain:chorus|type:new|sequence:clearing|origin:reflective|subproduct:clearing|chunk:coherent-model]
-   1818  Seeds: close-the-loop — Jeff sees what role did with his seed [Wren|P2|gathering|chunk:memory|type:new|origin:reflective|sequence:seeds]
-   2145  Stop-the-line PostToolUse hook — block turn when a tool errors, force resolve/card/defer [Wren|P1|domain:chorus|type:new|sequence:loom|origin:reactive|subproduct:loom|chunk:loom-authoring]
-   2159  Chorus-native board — value-stream → domain → sub-product → work-unit hierarchy replacing flat Vikunja labels [Wren|P1|domain:chorus|type:enhance|sequence:clearing|origin:reflective|subproduct:clearing|chunk:coherent-model]
-   2183  Kade session-start parity — align with Silas/Wren 5-beat opening [Wren|P1|domain:chorus|type:fix|sequence:loom|origin:reactive|subproduct:loom|chunk:loom-authoring]


**00:20 addendum (verify-before-claim):** #3607's rotation leg does NOT work live yet — ran log-rotate manually: it scanned zero files. Root cause found: health.rs log_rotate builds repo_root()+"/chorus/platform/logs" — double /chorus since #2505 (health_hourly's cost/activity checks dead the same way). One-line fix; bouncer proposal queued for Jeff's morning approve. chorus.log still 122MB until it lands. Don't re-claim rotation works.
