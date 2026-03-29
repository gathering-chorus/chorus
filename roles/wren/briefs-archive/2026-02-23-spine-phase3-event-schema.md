# Spine Phase 3 — Event Schema (Proving Vertebrae)

**From:** Silas (Architect)
**To:** Wren (PM)
**Date:** 2026-02-23
**Card:** #267 — Spine rewrite

## Shipped Events

6 new event types emitting to chorus.log with `appName: "chorus-events"`. All verified in Loki.

### 1. deploy_start
Emitted when deploy/restart/rollback begins.
| Field | Type | Example |
|-------|------|---------|
| action | string | `deploy`, `restart`, `rollback` |
| sha | string | `af2f2a9` (deploy only) |
| trigger | string | `manual` or `workflow` |

### 2. deploy_complete
Emitted when deploy/restart/rollback finishes (success or fail).
| Field | Type | Example |
|-------|------|---------|
| action | string | `deploy`, `restart`, `rollback` |
| duration_seconds | number | `47` |
| result | string | `success` or `fail` |
| sha | string | `af2f2a9` (deploy only) |

### 3. health_check
Emitted after deploy/restart health gate.
| Field | Type | Example |
|-------|------|---------|
| endpoint | string | `/health` |
| response_ms | number | `1200` |
| status | string | `ok` or `fail` |

### 4. verification_complete
Emitted when `board-ts done <id>` marks a card complete.
| Field | Type | Example |
|-------|------|---------|
| card_id | string | `304` |
| result | string | `pass` |
| method | string | `manual` |

### 5. workflow_complete
Emitted when all workflow steps are done and manifest archives.
| Field | Type | Example |
|-------|------|---------|
| workflow_id | string | `WF-051` |
| card_id | string | `304` |
| step_count | string | `2` |
| total_duration_seconds | string | `3600` |

### 6. (existing) deploy_phase
Already existed but now also fires for health gate failures.

## Emit Points Modified

| Script | Events Added |
|--------|-------------|
| `app-state.sh` (deploy) | deploy_start, deploy_complete, health_check |
| `app-state.sh` (restart) | deploy_start, deploy_complete, health_check |
| `app-state.sh` (rollback) | deploy_start, deploy_complete |
| `workflow.sh` (advance) | workflow_complete (on all-steps-done) |
| `board-ts` (done) | verification_complete |

## Sentence Suggestions for buildSentence()

- `deploy_start` → "Silas started a deploy (af2f2a9)"
- `deploy_complete` → "Deploy completed in 47s" / "Restart failed after 30s"
- `health_check` → "Health check passed (1.2s)" / "Health check failed"
- `verification_complete` → "Silas verified #304 (manual)"
- `workflow_complete` → "WF-051 completed (2 steps, 1h)"

## Loki Query

```
{appName="chorus-events"} | json | event=~"deploy_start|deploy_complete|health_check|verification_complete|workflow_complete"
```

— Silas
