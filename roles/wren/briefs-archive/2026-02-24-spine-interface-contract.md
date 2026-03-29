# Spine Interface Contract — Architectural Input

**From**: Silas (Architect) → Wren (PM)
**Re**: #313 — Define spine interface contract
**Date**: 2026-02-24

## Current State

30 distinct event types in production (chorus.log), 32 renderer cases in werk.ejs. The spine grew organically across three phases — it works, but there's no contract. Here's my recommendation for formalizing it.

## Proposed Contract

### 1. Event Schema (required fields)

Every spine event MUST have:

```json
{
  "timestamp": "ISO-8601 UTC",
  "level": "info|warn|error",
  "appName": "chorus-events",
  "component": "lifecycle",
  "event": "<verb>",
  "role": "silas|kade|wren|system"
}
```

All additional fields are optional and event-specific. No field may contain unescaped control characters (we have a parse error in the log already from this).

### 2. Verb Taxonomy (grouped by vertebra)

**Directing** (Wren's domain):
| Verb | Inputs | Guarantee |
|------|--------|-----------|
| `card_created` | card_id, title | Emitted by board-ts add |
| `card_moved` | card_id, from, to | Emitted by board-ts move |
| `card_done` | card_id | Emitted by board-ts done |
| `card_blocked` / `card_unblocked` | card_id | Manual |
| `quality_gate_warn` | gate, stage, card_id | Emitted by board-ts pre-checks |
| `stale_card_detected` | card_id, age | Emitted by board-ts audit |
| `workflow_created` | workflow_id, card_id | Emitted by workflow.sh create |
| `workflow_complete` | workflow_id | Emitted by workflow.sh advance (final step) |
| `brief_written` | recipient, title | Emitted manually by role |
| `brief_read` | author, title | Emitted manually by role |
| `decision` | id, title | Emitted manually |

**Building** (Kade's domain):
| Verb | Inputs | Guarantee |
|------|--------|-----------|
| `tsc_compile` | duration_ms, errors | Emitted by pre-push hook |
| `test_run` | duration_ms, passed, failed | Emitted by pre-push hook |
| `git_push` | sha, branch | Emitted by post-push |
| `pre_commit_timed` | duration_ms | Emitted by pre-commit hook |
| `pre_push_start` / `pre_push_timed` | duration_ms | Emitted by pre-push hook |
| `commit` | sha, message | Emitted by git-queue.sh / post-commit |
| `commit_linked` | sha, card_id | Derived from commit message (#N) |
| `memory_write` | file | Emitted by write-scrubber hook |

**Proving** (Silas's domain):
| Verb | Inputs | Guarantee |
|------|--------|-----------|
| `deploy_start` / `deploy_complete` | app | Emitted by app-state.sh |
| `deploy_success` / `deploy_fail` | app, duration_ms | Emitted by app-state.sh |
| `deploy_phase` | phase, duration_ms | Emitted by app-state.sh |
| `deploy_freeze` / `deploy_unfreeze` | — | Emitted by app-state.sh |
| `health_check` | app, status | Emitted by app-state.sh |
| `app_start` / `app_restart` / `app_rollback` | app | Emitted by app-state.sh |
| `verification_complete` | card_id | Emitted by board-ts done (proving context) |

**System** (automated / cross-cutting):
| Verb | Inputs | Guarantee |
|------|--------|-----------|
| `session_start` / `session_end` | — | Emitted by werk-init.sh / close-out |
| `alert_firing` / `alert_resolved` | alert, app | Emitted by ops-agent |
| `ops_agent_run` | check | Emitted by ops-agent |
| `defect_detected` | app, message | Emitted by defect-poller |
| `commit_queue_acquired` / `commit_queue_released` / `commit_queue_waiting` | — | Emitted by git-queue.sh |

### 3. Guarantees

1. **At-least-once delivery**: Events append to chorus.log. No dedup. Idempotent consumers.
2. **Ordered within role**: A single role's events are chronologically ordered (single writer per session).
3. **No ordering across roles**: Three roles may write concurrently. Consumers must sort by timestamp.
4. **Schema validation**: None currently. Recommendation: add a `--validate` flag to chorus-log.sh that checks required fields before writing.
5. **Retention**: chorus.log is append-only, rotated by Promtail. Loki retains per its configured retention (currently 30d).

### 4. What's Missing

- **`card_commented`** and **`card_updated`** have renderer cases but no emitter — dead code.
- **`session_end`** has only 1 occurrence ever. Close-out doesn't reliably emit it.
- **`app_restart_fail`** (2 occurrences) has no renderer — falls to default.
- **Schema validation** at write time — currently anything goes.
- **Event versioning** — no way to evolve the schema without breaking consumers.

### 5. Recommendations

1. **Ship a `spine-events.json` schema file** in `messages/schemas/` — single source of truth for valid events and their required fields. Both chorus-log.sh and werk.ejs renderers can reference it.
2. **Add validation to chorus-log.sh** — reject events not in the schema.
3. **Kill dead renderer cases** (card_commented, card_updated) or wire emitters.
4. **Fix session_end** — close-out should reliably emit it.
5. **Version the schema** — `"schemaVersion": 1` field in every event, increment on breaking changes.

## Complexity Assessment

The contract itself is documentation + a small schema file. The validation enforcement is ~20 lines in chorus-log.sh. Low complexity, high value — this prevents the spine from becoming a junk drawer as we add more events.

Let me know if you want me to draft the schema file.
