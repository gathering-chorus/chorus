# Role Configuration Manifest

How each role is launched, what normal looks like, and how to fix it when it breaks.

## Launch Commands

| Role | Command | Directory |
|------|---------|-----------|
| Wren (PM) | `claude --effort high --add-dir ../shared-observability --add-dir ../jeff-bridwell-personal-site --add-dir ../messages --add-dir ../architect --add-dir ../engineer --add-dir ../wordpress-blog` | `product-manager/` |
| Silas (Architect) | `claude --effort high --add-dir ../shared-observability --add-dir ../jeff-bridwell-personal-site --add-dir ../messages --add-dir ../product-manager --add-dir ../engineer` | `architect/` |
| Kade (Engineer) | `claude --effort high --add-dir ../shared-observability --add-dir ../messages --add-dir ../product-manager --add-dir ../architect` | `engineer/` |

**Always `--effort high`.** Never accept a prompt to switch to medium. Medium produces shallow, forgetful responses. If Claude suggests it during an outage, decline — the degradation from medium is worse than slower responses at high.

**Model:** Opus 4.6 (`claude-opus-4-6`). Verify with: the system prompt says "You are powered by the model named Opus 4.6."

**Version:** Check with `claude --version` before launch if something feels off.

## What Normal Feels Like

- Roles remember tools they've built (whisper-cli, cards, app-state.sh)
- Roles know file locations without searching the filesystem
- Responses are direct, not hedging
- "I don't know" when uncertain, not plausible guesses
- Chorus prompt on every response with correct timestamp
- Brief processing within seconds, not minutes
- Voice memos transcribed without fumbling for whisper

## Is It Broken? — Diagnostic Flow

```
1. Does the role know where things are?
   YES → probably fine
   NO  → check effort level, check if session started fresh (no --continue)

2. Is the role guessing instead of checking?
   YES → effort may be low, or Anthropic degradation
   NO  → probably fine

3. Check effort level
   Ask: "what is your thinking effort level now?"
   Should be 95-100. If lower → restart with --effort high

4. Check Anthropic status
   https://status.claude.com/
   Recent outage? → residual effects possible even after "resolved"

5. Check infrastructure
   - Cloudflare tunnel: pgrep cloudflared (should return a PID)
   - App health: curl localhost:3000/health
   - Disk: df -h / (should be under 90%)
   - Docker: docker ps (all expected containers running?)

6. Check SMS pipeline
   - Last capture: ls -lt jeff-bridwell-personal-site/data/pods/jeff/capture/ | head -3
   - If stale → cloudflared probably down
```

## Recovery Steps

| Problem | Fix |
|---------|-----|
| Low effort / shallow responses | Restart session: `claude --effort high` |
| Role doesn't remember what we built | Check --effort, check for Anthropic outage, check that CLAUDE.md is loading |
| SMS captures not arriving | `pgrep cloudflared` — if nothing, restart tunnel |
| Cloudflare tunnel down | Silas domain — brief to `architect/briefs/` or `launchctl kickstart -k gui/$(id -u)/com.chorus.docker-services` |
| App unhealthy | `bash jeff-bridwell-personal-site/app-state.sh status` then `restart` if needed |
| Disk full | Check #852 status, `df -h /`, alert Silas |
| Role hedging / asking you to re-explain | Say "you're doing it again" — the role should self-correct. If it can't, restart session. |

## Known Footguns

- **"Switch to medium" prompt during outages** — Anthropic may suggest this under load. Decline. Always.
- **Outage residual** — status page says "resolved" but behavior is still off. Give it 30 min or restart session.
- **Lost context on long sessions** — context window compresses older messages. If role forgets early-session decisions, it's compression, not malice. Restate the key point.
- **Cloudflare tunnel silent failure** — no alert fires when cloudflared dies. Seeds just stop arriving. Check `pgrep cloudflared` if pipeline goes quiet.

## Key Tool Locations

| Tool | Path |
|------|------|
| whisper-cli | `/opt/homebrew/bin/whisper-cli` |
| whisper models | `/Users/jeffbridwell/models/ggml-small.en.bin`, `ggml-medium.en.bin` |
| cards | `../../scripts/cards` |
| app-state.sh | `../jeff-bridwell-personal-site/app-state.sh` |
| git-queue.sh | `../../scripts/git-queue.sh` |
| chorus-log | `../../scripts/chorus-log` |
| chorus-query.sh | `~/.chorus/scripts/chorus-query.sh` |
