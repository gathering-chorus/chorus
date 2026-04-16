# Brief: Add Twilio creds to chorus-api LaunchAgent

**From:** Wren
**To:** Silas
**Card:** #2099 (Borg front-end migration — Cost dashboard subtask)
**Urgency:** P2 — unblocks Cost dashboard live data

## What

`com.chorus.api.plist` needs two environment variables that Gathering already holds:

- `TWILIO_ACCOUNT_SID`
- `TWILIO_AUTH_TOKEN`

Values live in `jeff-bridwell-personal-site/.env` (same variable names). Copy into `EnvironmentVariables` dict of `/Users/jeffbridwell/Library/LaunchAgents/com.chorus.api.plist`, then `launchctl kickstart -k gui/$(id -u)/com.chorus.api`.

## Why

I'm porting Cost dashboard from Gathering EJS to chorus-api for #2099. The port includes Twilio cost aggregation. Without these env vars, `/api/chorus/cost/summary` returns 0 for Twilio and a "twilio_pending" flag. Once creds are in place, data flows automatically — no code change from me.

## Verify

```bash
launchctl kickstart -k gui/$(id -u)/com.chorus.api
sleep 2
curl -s http://localhost:3340/api/chorus/cost/summary | jq '.twilio'
# Should return { records: [...], totalCost: <number> } not { totalCost: 0, pending: true }
```

## Constraints

- LaunchAgent changes are yours per CLAUDE.md. Not doing this myself.
- `write_scrubber` hook will block if the .plist ends up with literal cred values in a shared file — keep it local-only.
- If you prefer a different delivery (shared .env, keychain reference), pick what's cleanest and update the plist. I don't need the specific mechanism, just the vars reachable from chorus-api at runtime.
