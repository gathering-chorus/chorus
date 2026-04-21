# Spine Emitter Inventory — #371 / #411

**Date**: 2026-02-26 | **Author**: Silas | **Updated**: #411 migrated 5 shell direct-writers

## Architecture

```
chorus.log ← chorus-log.sh (validated) ← 18 callers (was 13 + 5 direct)
           ← direct JSON writes        ← 3 TypeScript modules only
           → Promtail (chorus-operations job) → Loki

command-errors.log ← command-outcome-hook.sh
                   → NOT scraped by Promtail ← BROKEN

permission-prompts.log ← permission-logger-hook.sh
                       → NOT scraped by Promtail ← GAP
```

## Emitter Inventory

### Via chorus-log.sh (schema-validated)

| Source | Events | Target |
|--------|--------|--------|
| app-state.sh | app_start, app_stop, app_restart, deploy_start/phase/complete/success/fail, health_check, app_rollback, deploy_freeze/unfreeze | chorus.log → Loki |
| chorus-audit.sh | session_start, session_end, disk_check | chorus.log → Loki |
| git-queue.sh | commit_queue_waiting/acquired/released/timeout | chorus.log → Loki |
| cost-report.sh | cost_report | chorus.log → Loki |
| chorus-ops.sh | ops_agent_run, defect_detected | chorus.log → Loki |
| system-state.sh | system_start, system_stop | chorus.log → Loki |
| board.sh | card_created/moved/done/blocked | chorus.log → Loki |
| harvest-media.sh | harvest_start, harvest_complete | chorus.log → Loki |
| pre-commit (app repo) | tsc_compile, pre_commit_timed | chorus.log → Loki |
| pre-push (app repo) | pre_push_start, test_run, pre_push_timed, git_push | chorus.log → Loki |
| alert-notifier.py | alert_firing, alert_resolved | chorus.log → Loki |

### Former direct writers — MIGRATED to chorus-log.sh (#411)

All 5 shell direct-writers now call chorus-log.sh. New event types added to spine-events.json:
- `guardrail_decision` — infra-guardrails.sh
- `write_scrub` — write-scrubber-hook.sh
- `data_classification` — sensitive-paths-hook.sh
- `audit_check` — chorus-audit.sh (log_json)
- `session_start` — `chorus-hook-shim session-start` subcommand (invoked by the SessionStart hook; already existed, just switched to chorus-log.sh call)
- chorus-audit.sh log_event was already matching format — now delegates to chorus-log.sh

### TypeScript writers to chorus.log

| Source | appName | Events | Reaches Loki |
|--------|---------|--------|:---:|
| cards/cli.ts | cards | card_created/moved/done/blocked/updated/commented, board_snapshot, board_audit_start/close, stale_card_detected, quality_gate_warn | Yes |
| cards/cli.ts | chorus-events | verification_complete | Yes |
| workflow-engine/cli.ts | workflow-engine | workflow_advance, workflow_complete | Yes |
| chorus/api/server.ts | grafana-alerts | alert_firing, alert_resolved | Yes |

### Separate log files (NOT in Loki)

| Source | Log File | Events | Reaches Loki |
|--------|----------|--------|:---:|
| command-outcome-hook.sh | command-errors.log | Bash failures, struggle signals | **NO** |
| permission-logger-hook.sh | permission-prompts.log | Every tool call | **NO** |

## Loki Bridge Status

**Working.** `{appName="chorus-events"}` and `{job="chorus-operations"}` both return data. Wren's walkthrough may have hit a timing gap. All chorus.log events reach Loki via Promtail.

**Broken paths:**
1. `command-errors.log` — not in Promtail scrape config. Error introspection events are invisible to Loki.
2. `permission-prompts.log` — not in Promtail scrape config. Tool call tracking invisible.

## Gap List (prioritized)

| Priority | Gap | Impact | Fix |
|:---:|-----|--------|-----|
| P1 | command-errors.log not scraped | Error introspection invisible — defeats the purpose of command-outcome-hook | Add to Promtail config |
| P1 | permission-prompts.log not scraped | Session tool call data invisible — dense spine events can't query it | Add to Promtail config |
| P2 | Decision events (DEC-NNN) | Decisions are file writes with no spine event — spine doesn't know when decisions happen | Add emitter to claudemd-gen.sh or a decision-recording script |
| P2 | Duplicate chorus-log.sh in chorus/ | Old copy without schema validation — could emit invalid events | Delete or symlink |
| P3 | Brief consumption events | brief_written exists but no brief_read/brief_consumed event | Add emitter to role session boot |
| P3 | Build events (non-app) | Swift compiles, LaunchAgent loads have no emitters | Add to relevant scripts |
| P3 | Direct writers skip schema validation | 5 scripts bypass chorus-log.sh — events may not match spine-events.json | Migrate to chorus-log.sh calls |
