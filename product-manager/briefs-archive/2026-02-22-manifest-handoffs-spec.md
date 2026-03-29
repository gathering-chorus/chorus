# Spec: Manifest Handoffs — Closed-Loop Proof of Connection

**Card:** C#43
**Owner:** Wren
**Workflow:** WF-009
**Date:** 2026-02-22

## Problem

Handoffs between roles break silently. A brief can sit unread for days. A workflow step can go ready and the role never sees it. A decision made in the Clearing can fail to reach the role that needs to act on it. Nobody knows until Jeff asks "where are we on that?" and discovers the chain broke.

This happened with Slack — Jeff spent a full day thinking conversations were connected before discovering they weren't. The system looked functional but handoffs weren't landing.

## Principle

Every handoff needs three properties:
1. **Sent** — timestamped, to a known location
2. **Received** — confirmed, timestamped
3. **Visible** — if either is missing, the gap shows up somewhere Jeff will see it

## What Constitutes a Handoff

| Type | Sender | Receiver | Current mechanism |
|------|--------|----------|-------------------|
| Brief | Any role | Target role | File in `briefs/` dir |
| Workflow step | workflow-ts advance | Next role | Handoff brief + status in manifest |
| Decision | Clearing / conversation | Affected role(s) | decisions.md entry |
| Commitment | Group conversation | Assigned role | commitment-brief-writer.ts |
| Intake item | Clearing close | Next session | `~/.chorus/intake/*.json` |

## Design: Handoff Registry

### Data Model

A single append-only JSON-lines file at `messages/logs/handoffs.log`:

```json
{
  "id": "HO-001",
  "timestamp": "2026-02-22T17:00:00Z",
  "type": "brief",
  "from": "wren",
  "to": "silas",
  "artifact": "architect/briefs/2026-02-22-chorus-ontology.md",
  "context": "C#40 — Chorus ontology formalization",
  "status": "sent",
  "received_at": null,
  "received_by": null
}
```

When the receiving role reads the brief (detected by session-start or manual confirm):

```json
{
  "id": "HO-001",
  "timestamp": "2026-02-22T18:30:00Z",
  "status": "received",
  "received_at": "2026-02-22T18:30:00Z",
  "received_by": "silas"
}
```

### Write Path (Sent)

Handoff events are logged when:
- `workflow-ts advance` writes a handoff brief → auto-log sent event
- A role writes a file to another role's `briefs/` dir → hook detects and logs
- A decision is written to `decisions.md` with assigned roles → auto-log
- Clearing close writes intake items → auto-log per item

Implementation: extend existing tools to append to `handoffs.log` at the point of creation.

### Read Path (Received)

Receipt is confirmed when:
- `session-start.sh` runs and finds pending handoffs for the role → mark received
- Role explicitly confirms via `workflow-ts advance` (step completion = receipt of previous brief)
- A PostToolUse hook on Read detects a brief file being opened → mark received

### Stale Detection

`session-start.sh` checks for handoffs where:
- `status = "sent"` AND `timestamp` older than threshold (default: 4 hours during work hours)
- Surface as warning: "Handoff HO-001 (brief to silas) sent 6h ago — not yet received"

### Visibility

Three surfaces:
1. **session-start.sh** — warns on stale handoffs for the current role
2. **Nervous system viz (C#42)** — connection lines between circles show handoff health
3. **`workflow-ts pending`** — already shows ready steps, extend to show unconfirmed handoffs

## What I Need From Silas

1. Where should handoff logging integrate with existing event pipeline (chorus-log.sh, Loki)?
2. Should handoff events flow to Loki for Grafana dashboards, or is the log file sufficient?
3. Any concerns about the PostToolUse Read hook for auto-detecting receipt?
4. Schema alignment — does this fit the Chorus ontology direction (C#40)?

## What I Need From Kade

1. Can `workflow-ts advance` call a handoff logger as part of the advance flow?
2. For the brief-write detection: is a PostToolUse hook on Write the right mechanism, or should it be in the write-scrubber-hook.sh chain?
3. session-start.sh changes — adding stale handoff checks. Any concerns about startup time?
4. What's your experience with handoffs landing vs getting lost? What breaks most often?

## Scope

**V1 (build after consultation):**
- Handoff log file (append-only JSON lines)
- workflow-ts integration (auto-log on advance)
- session-start.sh stale detection + warnings
- Manual confirm command: `workflow-ts confirm HO-NNN`

**V2 (after instrumented viz):**
- Auto-receipt via Read hook
- Viz integration (connection health signals)
- Grafana/Loki pipeline for handoff metrics

## Exit Criteria

Jeff can see, without asking, whether a handoff landed. Stale handoffs surface automatically. The chain between roles is observable.
