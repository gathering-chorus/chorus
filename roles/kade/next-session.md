# Kade — next session (2026-05-02 close)

## Headline

v3 commits-service-design landed. "Substrate owns the working tree" is the load-bearing primitive. Six sequencing cards (#2661–#2667) filed in Later. ADR-028 audit complete (M2/M5/M6/A2 closed by v3, M3 partial-with-gap-named, M7 populate-card pending).

## Shipped this session

- **#2644** chorus-hooks pre-existing test failures fixed (protocol_contract auto-resolved + smoke determinism via env, then retired in v3 design)
- **#2668** v3 mermaid-loader diagrams — superseded by #2674 (Jeff: "this is not how our other service designs look")
- **#2674** commits-service-design rewritten in chorus/loom hand-crafted chrome (Georgia serif, .promise/.component/.gap/.resolved/.flow)
- **#2675** ADR-028 audit results + Wren reorder folded into doc

## Open arc — v3 commits substrate (build not started)

- **#2661 v3-1** chorus_commit MCP tool — load-bearing, ships first. AC7 = contract test + fail-loud (typed errors, non-zero, spine event on every refusal — same shape as cards' #2671, built in not retrofit).
- **#2662 v3-2** migrate skills (/acp, /pull, /demo, /reboot, /close, /pair) to chorus_commit. Depends on #2661.
- **#2663 v3-3** block raw git mutations on /chorus. Depends on #2662 (closes Mode A structurally).
- **#2665 v3-4** retire CHORUS_TEST_FORCE_FIX_CARD env-bypass.
- **#2666 v3-5** SWAT-as-card-type encoding (Wren constraint — fast-path as card property, not wire flag).
- **#2667 v3-6** retire bash git-queue.sh + branch-check.sh + pre-commit. Depends on #2662 + #2663.

## Pending follow-ups

- **commits-domain Athena populate-card** — file matching cards-#2673 pattern. Audit M7 verdict: wip:edges missing, done:actors+scenarios+contract missing.
- **Wren PM note** — value-gain mirroring on .flow rows (each danger paired with To-Be value). Asked fold-now vs follow-on, awaiting her reply.

## State this session left

- All my PRs merged. #99 (silas/2659) is the only open PR — his.
- Working tree on /chorus has typical noise (clearing transcripts, chorus.log) but no peer dirty work I'm aware of.
- Mode A receipt: my `git checkout` overwrote Silas's uncommitted work mid-session. Resolved (he recovered from /tmp + replayed). Direct trigger for v3.

## What the next session picks up

- If Jeff signs off v3 sequencing, pull #2661 (load-bearing).
- If not, hold v3 cards in Later. Don't sequence ahead.
- Replies may land from Wren on value-gain mirroring.
