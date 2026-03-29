# Brief: Andon Authoritative State — #1070

**From:** Kade (Engineer)
**To:** Silas (Architect)
**Date:** 2026-03-05
**Card:** #1070 — Restore andon eye
**Priority:** P2

## Context

Jeff can't rely on the andon signal. The root cause: enrichment infers role state from indirect signals (prompt timestamps, PIDs, board-ts snapshots). Three specific failure modes:

1. **Waiting when building** — long tool calls (>120s) with no prompt flip Active to Waiting. Board-ts contention drops the card, removing the "alive + WIP card = Active" fallback.
2. **Active when blocked** — session alive + WIP card = Active, but role is actually stuck waiting for Jeff or another role. No signal until card moves to Blocked column.
3. **Gemba eye drops after 5 min** — gemba file written once at observation start, 300s staleness check in enrichment kills it. Most of a 30-min observation runs dark.

## Design

### Layer 1: Better automatic inference (your scripts)

1. **team-scan.sh** — add `pgrep -P <claude-pid>` child process check. Write results to `{role}-heartbeat.json` with `{"ts":N,"pid":N,"children":N}`. Active children = tool executing = Active even without recent prompt.

2. **andon-enrich.sh** — three changes:
   - Read `{role}-declared.json` as primary state when fresh (<300s)
   - Extend stale-card fallback from 60s to 300s
   - Gemba: replace file-age check with observer PID alive check

3. **No Swift changes** — display already reads enrichment JSON correctly.

### Layer 2: Declared state (new script)

New `role-state.sh` (~30 lines):
```
role-state.sh <role> <state> [card=N] [detail="text"] [gemba=<role>]
```

Writes `/tmp/claude-team-scan/{role}-declared.json`:
```json
{"role":"kade","state":"building","card":1070,"detail":"running tests","ts":1772736719}
```

States: `building`, `blocked`, `waiting`, `observing`, `idle`

Enrichment trusts declared state when fresh. Falls back to inference when stale. needs_jeff signals (SWAT/Blocked cards) still override everything.

### CLAUDE.md integration

Add to card-pull protocol: call `role-state.sh <role> building card=<id>` when moving card to WIP.
Add to blocked protocol: call `role-state.sh <role> blocked detail="reason"`.
Gemba skill already writes gemba files — update to use `role-state.sh` instead.

## AC (from card)

1. Role shows Active/Building during long tool executions (>120s), not Waiting
2. Card persists through board-ts transient failures
3. Gemba eye stays visible for full observation duration
4. Roles can declare blocked and waiting states that override inference
5. No false Waiting when role is actively executing

## What I need from you

This is infra scripts (your vertical). The changes are in team-scan.sh, andon-enrich.sh, and a new role-state.sh. I've spec'd the design — you own the implementation. I'll handle the CLAUDE.md fragment for role protocol integration.

Let me know if the design has gaps or if you'd approach it differently.
