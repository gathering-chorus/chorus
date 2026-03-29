# Brief: Render ops events on /werk spine (#367)

**From:** Silas (Architect) → **To:** Kade (Engineer)
**Date:** 2026-02-24
**Card:** #367 — Render ops events on /werk spine

## Context

Schema v1.1.0 shipped (#364) with 5 new ops event types: `disk_check`, `container_state_change`, `harvest_start`, `harvest_complete`, `cost_report`. Alert-notifier also fixed (#366) — `alert_firing` and `alert_resolved` now emit correct structured fields. Plus existing `ops_agent_run` and `defect_detected` need sentence rendering (currently fall through to default).

## What's Needed

### 1. Add fields to `parseSpineEntries()` (app.ts ~line 1700)

Add these field extractions after the existing fields:

```typescript
alertname: parsed.alertname || '',
severity: parsed.severity || '',
container: parsed.container || '',
from_state: parsed.from_state || parsed.fromState || '',
to_state: parsed.to_state || parsed.toState || '',
collection: parsed.collection || '',
disk_pct: parsed.disk_pct || '',
volume: parsed.volume || '',
items_processed: parsed.items_processed || '',
findings: parsed.findings || '',
cards: parsed.cards || '',
model: parsed.model || '',
summary: parsed.summary || '',
sessions: parsed.sessions || '',
messages_count: parsed.messages || '',
tokens: parsed.tokens || '',
sms_count: parsed.sms_count || '',
sms_cost: parsed.sms_cost || '',
```

Note: `messages` field conflicts with existing message — use `messages_count` in the parsed entry.

### 2. Add `buildSentence()` cases (werk.ejs ~line 857, before `default`)

```javascript
case 'alert_firing':
    return R + '<span class="ev-verb verb-blocked">alert firing</span> ' +
        (e.alertname ? '<span class="ev-ref">' + escS(e.alertname) + '</span> ' : '') +
        (e.severity ? '<span class="ev-context">[' + escS(e.severity) + ']</span>' : '');
case 'alert_resolved':
    return R + '<span class="ev-verb verb-done">alert resolved</span> ' +
        (e.alertname ? '<span class="ev-ref">' + escS(e.alertname) + '</span>' : '');
case 'disk_check':
    return R + '<span class="ev-verb ' + (e.status === 'critical' ? 'verb-blocked' : e.status === 'warning' ? 'verb-warning' : 'verb-done') + '">disk check</span> ' +
        (e.disk_pct ? '<span class="ev-context">' + escS(e.disk_pct) + '%</span> ' : '') +
        (e.status ? '<span class="ev-context">[' + escS(e.status) + ']</span>' : '');
case 'container_state_change':
    return R + '<span class="ev-verb verb-deploy">container</span> ' +
        (e.container ? '<span class="ev-ref">' + escS(e.container) + '</span> ' : '') +
        (e.from_state ? '<span class="ev-context">' + escS(e.from_state) + '</span>' : '') +
        ' → ' +
        (e.to_state ? '<span class="ev-context"><b>' + escS(e.to_state) + '</b></span>' : '');
case 'harvest_start':
    return R + '<span class="ev-verb verb-deploy">started harvest</span> ' +
        (e.collection ? '<span class="ev-ref">' + escS(e.collection) + '</span>' : '');
case 'harvest_complete':
    return R + '<span class="ev-verb ' + (e.result === 'fail' ? 'verb-blocked' : 'verb-done') + '">harvest complete</span> ' +
        (e.collection ? '<span class="ev-ref">' + escS(e.collection) + '</span> ' : '') +
        (e.items_processed ? '<span class="ev-context">(' + escS(e.items_processed) + ' items' +
        (e.duration_seconds ? ', ' + escS(e.duration_seconds) + 's' : '') + ')</span>' : '');
case 'ops_agent_run':
    return R + '<span class="ev-verb verb-deploy">ops agent ran</span> ' +
        (e.findings ? '<span class="ev-context">' + escS(e.findings) + ' finding' + (e.findings === '1' ? '' : 's') + '</span> ' : '') +
        (e.cards ? '<span class="ev-context">→ ' + escS(e.cards) + ' card' + (e.cards === '1' ? '' : 's') + '</span>' : '') +
        (e.summary ? ' <span class="ev-context">— ' + escS(truncS(e.summary, 60)) + '</span>' : '');
case 'defect_detected':
    return R + '<span class="ev-verb verb-blocked">defect detected</span> ' +
        (e.card_id ? cardRef + ' ' : '') +
        (e.pattern ? '<span class="ev-context">' + escS(truncS(e.pattern, 60)) + '</span> ' : '') +
        (e.tier ? '<span class="ev-context">[' + escS(e.tier) + ']</span>' : '');
case 'cost_report':
    return R + '<span class="ev-verb verb-workflow">cost report</span> ' +
        (e.sessions ? '<span class="ev-context">' + escS(e.sessions) + ' sessions, ' : '<span class="ev-context">') +
        (e.messages_count ? escS(e.messages_count) + ' msgs, ' : '') +
        (e.tokens ? escS(e.tokens) + ' tokens' : '') + '</span>';
```

### Verb classes already exist:
- `verb-blocked` = red (for failures, alerts)
- `verb-warning` = amber (for warnings)
- `verb-done` = green (for success, resolved)
- `verb-deploy` = blue (for infra actions)
- `verb-workflow` = purple (for lifecycle)

## AC
- All 9 ops event types render human-readable sentences on /werk spine (not falling through to default)
- `alert_firing` shows alertname + severity
- `disk_check` color-codes by status (ok/warning/critical)
- `container_state_change` shows from → to transition
- Existing spine events unchanged

## Dependencies
- Schema #364 ✅ shipped
- Alert-notifier fix #366 ✅ shipped
- Cost report wiring #370 ✅ shipped
