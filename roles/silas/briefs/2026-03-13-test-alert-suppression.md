# Brief: Alert noise reduction — Jeff only wants real issues

**From:** Kade
**To:** Silas
**Priority:** P2 (Jeff is getting paged for non-issues)

## Problem

Jeff's Mac is beeping constantly with alerts that aren't real problems. He wants the beep to mean something. Right now it's crying wolf.

I pulled the last 90 minutes from chorus-log — here's what Jeff actually saw:

| Time | Alert | Real issue? |
|------|-------|-------------|
| 9:58 | C3MemoryHeadroom ×2 | **No** — resolved 15 min later, flapping |
| 9:58 | HighMemoryUsage ×2 | **No** — same flap cycle |
| 10:13 | C3MemoryHeadroom resolved | noise |
| 10:13 | HighMemoryUsage resolved | noise |
| 10:15 | FusekiQuerySlow | **Maybe** — photo_listing 5551ms, but it flaps too |
| 10:30 | C3MemoryHeadroom ×2 | **No** — same flap, 17 min later |
| 10:37 | HighMemoryUsage ×2 | **No** — resolved 4 min later |
| 10:41 | DiskSpaceWarning | **Yes** — 86%, but was resolved then refired |
| 10:45 | FusekiQuerySlow resolved then refired | noise |
| 10:55 | Music harvest failed ×2 | **Yes** — JXA extraction broken |
| 10:55 | Music/Photos harvest completed ×6 | **Info, not alert** |
| 11:04 | ExternalTrafficSpike ×2 | **No** — test run burst |
| 11:16 | Documents harvest completed | **Info** — 4,125 items, working correctly |

**Score: ~20 notifications, maybe 2-3 real.** That's a 85% false positive rate.

## Three noise sources

### 1. Flapping alerts (biggest problem)
`C3MemoryHeadroom` and `HighMemoryUsage` fire and resolve every 15 min. They're right at threshold and bouncing. Each cycle = 2 fire notifications + 2 resolved notifications.

**Fix:** Increase `for:` duration on these rules (e.g., 15m → 30m) so transient spikes don't fire. Or add hysteresis — fire at 85%, resolve at 75%.

### 2. Test-triggered alerts
`ExternalTrafficSpike` fires on every test run. Tests burst traffic 5x above baseline.

**Fix:** Alertmanager silence API during test runs. `smoke-check.sh` creates a 10-min silence before running, auto-expires. Pattern:
```bash
SILENCE_ID=$(curl -s -X POST http://localhost:9093/api/v2/silences \
  -H 'Content-Type: application/json' \
  -d '{"matchers":[{"name":"domain","value":"app","isRegex":false}],
       "startsAt":"'$(date -u +%Y-%m-%dT%H:%M:%S.000Z)'",
       "endsAt":"'$(date -u -v+10M +%Y-%m-%dT%H:%M:%S.000Z)'",
       "createdBy":"test-runner",
       "comment":"Suppressing alerts during test run"}' | jq -r '.silenceID')
```

### 3. Harvest notifications as alerts
Harvest completions (#1346) come through the same notification channel as alerts. A "✅ Harvest done: documents" is useful info but shouldn't beep the same as "🔴 Disk full."

**Fix:** Either silent notifications (no sound) for harvest completions, or route them to a different notification group.

## Jeff's bar

> "I want the Mac to beep at me when it is a real issue"

**Real issues:** disk actually filling, service down, harvest failed, tunnel down.
**Not real issues:** memory bouncing near threshold, test traffic, query slightly slow, harvest completed successfully.

## Existing pattern

`alertmanager.yml` already has deploy-aware inhibit rules — same pattern extends here.

## What I need from you

1. Tune flapping alerts (increase `for:` or add hysteresis)
2. Wire test silences into smoke-check.sh
3. Separate harvest completion sounds from alert sounds in alert-notifier.py
4. Card or JDI — your call on scope
