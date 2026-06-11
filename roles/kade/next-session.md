# Kade — Next Session

## READ FIRST — behavioral (Jeff named these, hard)
- **Jeff is in auto-accept + focus mode and CANNOT read our output torrent.** Default to ONE line, signal only. Expand only on ask. When blocked: name the blocker + the single action that clears it, then STOP. Never push a dead path or send a commodity explainer he'd get from search in 1ns.
- **Consult memory/Loki/chorus BEFORE asserting.** Verify with the right tool; show observable reality, never assert from memory.

---

**Last session ended:** 2026-06-05 ~17:55 Boston via /reboot. ~9hr session.
(Canonical roles/kade/ is read-only mid-session per the #2913 guard — this lives in the kade-3193 werk; read it via git diff / the werk in the morning.)

## Accomplished (landed live)
- **#3236 + #3241 + #3240** — werk pipeline collapsed: act = local orchestrator (KEPT — the big arc was "keep act, gh gets all our steps," not remove it), run via the ONE `chorus_werk` MCP verb, every step mirrors to gh as `chorus/<step>/<card>` statuses. Self-hosted-runner detour thrown away.
- **#3219 — werk-acp RETIRED, live** (crate + chorus_acp MCP tool + test-acp.sh gone; retirement gate added; verified gone from the running MCP). 2nd real card through the new go/no-go flow.
- **#3237** (Wren's, paired) — werk-demo = blocking human gate (go/no-go/more → exit 0/2/1); werk-accept = go-signal; werk-finalize = mechanical back-half. Built + seam-proven (GO=0/MORE=2) + landed live.
- Reviewed Silas #3242/#3247 (Shaping docs) + Wren #3205 (pulse-gather) — real critique, all acted on.

## WIP
- **#3193 werk-review** — design DONE + committed in THIS werk (`designing/docs/werk-review-service-design.html`, sha 15731f04; opened in my Chrome window for Jeff). Cold-eyes gate: fresh sub-agent (diff+AC only, adversarial), before demo, advisory-first→hard-gate, structured floor (blast-radius + AC-coverage). **NEXT: build the verb from the design.**

## Morning pickup (recommended order)
1. **#3190 werk-test** — keystone: turns today's *advisory* test step into a real enforced gate. Highest value (closes a real gap, not polish).
2. **#3193 werk-review** — build the verb (design's done).
3. Crawler = Silas's morning focus (hydration-divergence / #3055, routed to him).

## Open follow-ons (mine unless noted)
- `werk-code` — STILL NO CARD. Card it (the coding phase as a verb; completes the verb sequence pull→code→commit→…).
- #3244 (gh-status helper + carry-forward — statuses split across 2 commits today), #3113 (MCP-wrap remaining verbs demo/env-down/finalize/do-more+review), #3229, #3201, #3223, #3148 (dep Wren).
- **Silas:** skip-deleted-crates deploy fix (#3250-sibling, surfaced by #3219), #2317 (heartbeat→pulse-gather).

## Watch
- deploy_canonical edge-cases: autobins gap (#3250 fixed) + deleted-crate choke (Silas's follow-on). Verify-LIVE after any crate add/remove.
- chorus_werk-over-MCP has a ~10min timeout → human-gated land runs need act-direct (no timeout) or a prompt go.
- werk-acp-retired.bats test-1 checks dir-existence → false-trips on stale untracked `target/` build artifacts; sharpen to git-tracked check (tiny).
