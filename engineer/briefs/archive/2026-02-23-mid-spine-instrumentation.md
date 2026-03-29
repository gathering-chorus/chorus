# Mid-Spine Instrumentation — Building Phase Events

**From:** Wren (PM) | **To:** Kade (Engineer) | **Date:** 2026-02-23
**Card:** C#57 Phase 2 | **Priority:** P1

## Context

The Spine Activity view shipped today on `/team` (Spine tab). It shows card lifecycle events (upper third) and quality gate warnings (lower third) — all from `board-ts` CLI via `appName="board-client"`.

The **middle third is empty**. No commit, build, test, or deploy events appear. Jeff noticed immediately: "will be interesting to see how mid-spine events appear — I don't see them yet."

Good news: **many mid-spine events already exist** but use `appName="chorus-events"`. I'm widening the Loki query to include both appNames. Your job is filling the gaps.

## What Already Emits (showing up after query fix)

| Event | Source | Role | Fields |
|-------|--------|------|--------|
| `commit` | team repo post-commit | extracted from msg | `sha`, `message` |
| `commit_linked` | team repo post-commit | extracted | `sha`, `card`, `board` |
| `brief_written` | team repo post-commit | committer | `recipient`, `title` |
| `memory_write` | team repo post-commit | committer | `file`, `path`, `owner` |
| `pre_commit_timed` | app repo pre-commit | "app" | `duration_seconds`, per-check timings |
| `deploy_phase` | app-state.sh | "app" | `phase`, `duration_seconds`, `sha` |
| `deploy_success` | app-state.sh | "app" | build/health/smoke timings, `sha` |
| `deploy_fail` | app-state.sh | "app" | `message` |
| `app_restart` | app-state.sh | "app" | `message` |

## What's Missing (your deliverables)

### 1. Role attribution on app events
All app-state.sh events use `role="app"`. Jeff can't tell who triggered the deploy.

**Fix:** Detect the calling role and pass it. Options:
- Check `$CLAUDE_ROLE` or `$USER` env var
- Accept optional `--role <name>` flag on app-state.sh
- Default to "app" if unknown (backwards-compatible)

### 2. Test execution events
Pre-commit runs tests but only emits an aggregate `pre_commit_timed`. No test-specific event.

**Add:**
```
event=test_suite_start  role=<role>  framework=jest
event=test_suite_end    role=<role>  passed=2310  failed=0  duration_seconds=23
```

Best place: wrap the `npm run test` invocation in the pre-commit hook.

### 3. TypeScript compilation events
`npx tsc` runs in pre-commit and during builds but emits nothing.

**Add:**
```
event=tsc_compile_start  role=<role>
event=tsc_compile_end    role=<role>  duration_seconds=8  errors=0
```

### 4. Push events
No event when code is pushed to remote.

**Add** to app repo pre-push or post-push hook:
```
event=git_push  role=<role>  branch=main  sha=<head>
```

### 5. Build start event
app-state.sh emits `deploy_phase` for build completion but not build start. Can't calculate build duration from a single event.

**Add** before `docker compose build`:
```
event=build_start  role=app  sha=<sha>
```

## Schema Contract

All events MUST include these fields (the spine view parses them):
```json
{
  "timestamp": "ISO-8601",
  "event": "<event_name>",
  "role": "<wren|silas|kade|app>",
  "card_id": "<optional, if known>"
}
```

Additional fields are passed through and displayed as context. The spine view's `buildSentence()` function renders events by type — I'll add cases for the new event types as you ship them.

Use `chorus-log.sh` for emission (keeps `appName="chorus-events"`, Promtail picks it up automatically):
```bash
~/.chorus/scripts/chorus-log.sh <event> <role> [key=value ...]
```

## What You DON'T Need to Do

- No UI changes — the spine view auto-renders any event it receives
- No Loki config — Promtail already ingests chorus.log
- No new appName — stay on `chorus-events`

## Verification

After your changes, the spine view at `/team` → Spine tab should show commit, test, build, and deploy events interleaved with the card lifecycle events. The mid-spine goes from empty to populated.

## Suggested Order

1. Role attribution on app-state.sh (smallest change, immediate visibility)
2. Test + tsc events in pre-commit hook (moderate, high signal)
3. Push event (small)
4. Build start event in app-state.sh (small)

---

*This is C#57 Phase 2. Phase 1 (card quality gates + spine view) shipped today. Phase 3 (Silas — Proving phase gates) is next.*
