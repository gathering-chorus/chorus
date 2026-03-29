# Brief: Cost Dashboard — Revised Data Source

**From:** Silas
**To:** Kade
**Date:** 2026-02-22
**Card:** Needs card — "Cost dashboard with real-time JSONL aggregation"

## What Changed

The original plan used `stats-cache.json` for Claude Code usage data. That file is stale — stopped updating Feb 16. Wren caught it. Six days of data missing.

## New Data Source: JSONL Session Files

The real-time source is `~/.claude/projects/**/*.jsonl`. Every assistant response in every session writes a record with full token usage breakdown. I tested the aggregation — it works and produces accurate data.

### Mount

In `terraform/environments/dev/main.tf`, mount:
```
~/.claude/projects/ → /app/claude-sessions/ (read-only)
```

This replaces the original plan to mount `stats-cache.json`.

### JSONL Record Structure

Each line is JSON. For assistant records:
```json
{
  "timestamp": "2026-02-22T14:18:28.873Z",  // TOP-LEVEL, not inside message
  "sessionId": "2993d124-...",
  "message": {
    "model": "claude-opus-4-6",
    "usage": {
      "input_tokens": 3,
      "output_tokens": 9,
      "cache_read_input_tokens": 18719,
      "cache_creation_input_tokens": 17404
    }
  }
}
```

Not every line has `usage` — filter for records where `message.usage` exists.

### Role Mapping (from directory path)

| Path contains | Role |
|---------------|------|
| `-architect/` | Silas |
| `-engineer/` | Kade |
| `-product-manager/` | Wren |
| `-personal-site/` | App |

Subagent files live in `<session-id>/subagents/*.jsonl` — include these, map to parent role.

### Aggregation

Group by:
- **Day** — daily message count, session count, output tokens
- **Role** — per-role message count, output tokens, % of total
- **Hour** — hour-of-day distribution (from timestamp, Boston timezone)

### Performance

- ~91 JSONL files, largest 16MB
- Full scan: ~2-3 seconds on Mac Mini M1
- **Cache the result for 60 seconds** — dashboard auto-refreshes every 30s, but we don't need to re-scan every time
- Stream files line by line — don't load entire 16MB files into memory

### Burn Rate Calculation

```
% of month elapsed = (current day - 1) / days in month
% of usage intensity = current month output tokens / max month output tokens (use Feb's actual data as baseline)
```

If intensity % > elapsed %, running HOT. If lower, running COLD. Within 10%, SMOOTH.

### What Else

The rest of the plan is unchanged:
- Twilio SMS via Usage API (credentials in env)
- Clearing sessions via transcript JSONs
- Dark theme dashboard at `/cost`
- Admin-only route
- Navbar link with `target="_blank"`

Full revised plan at: `~/.claude/plans/indexed-imagining-galaxy.md`
