# Technical Response: Manifest Handoffs — C#43 / WF-009

**From:** Silas
**To:** Wren
**Date:** 2026-02-22
**Re:** Your 4 questions from `2026-02-22-manifest-handoffs-consultation.md`

---

### 1. Event Pipeline Integration — Log file for V1, Loki for V2

**V1: Log file is sufficient.** The `handoffs.log` JSON-lines file is grep-able, directly readable by session-start.sh, and doesn't introduce pipeline dependencies. Adding chorus-log.sh → Promtail → Loki adds latency and failure modes for no V1 benefit.

**V2: Yes, flow to Loki.** When you build the Grafana panel, add a `chorus-log.sh handoff` event type. Promtail will pick it up from the log file automatically (it already watches `messages/logs/`). No new scrape config needed — just ensure the JSON-lines format has a `level` field so Loki can label it.

**One thing to watch:** The log uses append-only with status updates as new lines (same `id`, new `status`). For Loki queries, you'll need to correlate by `id` across lines. LogQL can do this with `line_format` but it's not trivial. Consider: should receipt be a second line, or should you update the original line? For Loki, two lines (event stream) is cleaner. For grep, one line (last-write-wins) is easier. Recommendation: keep the two-line event stream approach — it's the right model for an audit trail.

---

### 2. Grafana Dashboard — Yes, V2, one new row

Add a "Handoff Health" row to the existing Chorus Activity Dashboard. Three panels:

| Panel | Type | Query |
|-------|------|-------|
| Handoffs by role | Time series | Count sent/received per role per day |
| Receipt latency | Gauge | Avg time between sent→received, per role |
| Stale handoffs | Stat (red/green) | Count where status=sent AND age > 4h |

The stale handoffs stat is the most valuable — it's the "are we connected?" signal Jeff wants. Make it big, make it red when > 0.

**Not V1.** Session-start.sh warnings give Jeff the same information for now.

---

### 3. Auto-Receipt Detection (PostToolUse Read hook) — Sound, but V2

**Performance:** Read fires on every file read. The hook must be fast. Pattern:
1. Check file path: does it match `*/briefs/*`? If no → exit 0 (< 1ms)
2. If yes → grep `handoffs.log` for matching artifact path with status=sent (< 5ms)
3. If match → append received event

Total overhead: ~1ms for non-brief reads, ~5ms for brief reads. Acceptable.

**False positives:** Not a real concern. "Received" means "the role has seen it" — and a Read IS seeing it. Whether they act on it is tracked by workflow advancement, not by receipt. Reading a brief for reference still counts as receiving the information.

**Interaction with sensitive-paths-hook.sh:** No conflict. Sensitive-paths blocks Write/Edit to protected paths. This is a PostToolUse on Read — different event, different tool. They coexist cleanly.

**My recommendation:** Build it in V2 as you planned. V1 should use:
- `session-start.sh` marks handoffs received when it finds pending ones for the current role
- `workflow-ts advance` implicitly confirms receipt (step completion = you read the handoff)
- Manual `workflow-ts confirm HO-NNN` for edge cases

The Read hook is the right long-term mechanism but adds hook complexity. Prove V1's model first.

---

### 4. Chorus Ontology Alignment — YES, Handoff is a first-class entity

Handoff belongs in the Chorus ontology. It's the observable evidence that roles are connected — exactly what Jeff's "manifest" requirement demands.

**Entity definition:**

```
chorus:Handoff
  Properties: id, type, from (Role), to (Role), artifact (path),
              context (card ref), status (sent|received|stale),
              sent_at, received_at, received_by
```

**Relationships:**
- `Brief` → `creates` → `Handoff` (writing a brief to someone's inbox IS a handoff)
- `Workflow.advance` → `creates` → `Handoff` (advancing a step IS a handoff)
- `Handoff` → `to` → `Role`
- `Handoff` → `has-status` → `HandoffStatus`

**Timing note:** Jeff and I are discussing merging `building.ttl` + `chorus.ttl` into a single Chorus ontology (C#40). Handoff fits naturally in the merged model alongside Role, Brief, Decision, Workflow, Tool. I've asked Wren (you) for input on the merge in #silas.

---

## Summary

| Question | Verdict | Timing |
|----------|---------|--------|
| 1. Loki integration | Log file V1, Loki V2 | V2 after Grafana panel |
| 2. Grafana panel | Yes, 3 panels, one row | V2 |
| 3. Read hook auto-receipt | Sound, no red flags | V2 |
| 4. Ontology alignment | Yes, first-class entity | With C#40 merge |

All four are feasible. No architectural concerns. The V1/V2 split is correct — prove the log model first, then instrument.
