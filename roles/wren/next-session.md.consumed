# Next Session — Wren

## This (very short) session shipped (2026-04-25 11:16–11:25)

Boot envelope thesis: **reboot is the test.** Opened with 5-beat narrative naming the through-line — drift on principles page, retracted #2476 gate-product, ADR-026 — all the same shape: contracts that don't survive a fresh seat are paraphrase.

Then Jeff probed: "can you do nudge via mcp" — and the answer was the load-bearing data:
- ToolSearch from this Wren seat returned **no `mcp__chorus__*` tools**. Only Gmail/Calendar/Drive/WebFetch/harness.
- That IS the #2476 reach-test result (negative). Nudged Silas via shell to surface the gap.
- Silas responded: settings.json schema rejects `mcpServers` blocks → he created per-role `.mcp.json` files (roles/wren/.mcp.json, roles/silas/.mcp.json, roles/kade/.mcp.json) with `X-Chorus-Role: <role>` baked in. Verified file exists at `roles/wren/.mcp.json` with chorus-api server pointing at `localhost:3340/mcp`.
- Issued /reboot.

## First action next session (load-bearing)

**Run the reach-test.** First turn after boot envelope:
1. Accept project-trust prompt if Claude Code asks (per Silas).
2. ToolSearch query for `chorus_principles_list` (or any `chorus_*`).
3. If visible: call it, confirm `from=wren, origin=mcp` spine event lands → that closes #2476 AC#7.
4. If still not visible: report back to Silas — `enableAllProjectMcpServers=true` may be needed in settings.json.

After reach-confirmed: route to Kade for /gate-code re-run, then re-/gate-product on #2476 for clean PASS, then full close.

## Open threads (carried from prior next-session)

1. **#2476 close** — pending reach-test (above).
2. **ADR-026** — Jeff sign-off pending: flip Status: Proposed → Accepted; toggle GitHub branch protection on `main` requiring CI green.
3. **gate-product skill amendment (JX/AX gates 8 & 9)** — named, not yet executed. Two retracts in a week is the data.
4. **Drive harvest decision** — scan-first vs ADR-027 first. Wren recommends scan-first.
5. **Principles reference-impl page** — stale card-status sections (FYA, not action).
6. **#2116** — fourth-session flinch.

## WIP at session close
- **#2476** — WIP, gate-product retracted, awaiting reach-test post-reboot.

## Friction this session
- MCP not reachable from Wren seat at session start — `.mcp.json` was missing; Silas created it mid-session. Reboot required to load.
- Schema gotcha banked: `mcpServers` belongs in `.mcp.json` at project root, NOT settings.json.
