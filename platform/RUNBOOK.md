# Chorus Operational Runbook

## Deploy Sequence

Every deploy follows this sequence. No exceptions. No shortcuts.

### 1. Pre-Deploy
```bash
# Run full test suite
cd /Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site
npm test

# Run BDD gate tests
cd /Users/jeffbridwell/CascadeProjects/chorus/platform
npx cucumber-js

# Verify current pipeline health BEFORE touching anything
bash ~/.chorus/scripts/check-seeds.sh
agent-state.sh health
```

**If any check fails: STOP. Fix first. Do not deploy on red.**

### 2. Deploy
```bash
# App changes (TypeScript)
bash scripts/app-state.sh deploy

# Hook binary changes (Rust)
cd services/chorus-hooks && cargo build --release
# Restart hook server
agent-state.sh restart chorus-hooks
```

### 3. Post-Deploy Verification
Run within 60 seconds of deploy:
```bash
# Health checks
agent-state.sh health
bash ~/.chorus/scripts/check-seeds.sh

# Functional checks — not just "is it running" but "does it work"
# Send a real seed and verify it arrives
# Send a nudge and verify it delivers
# Run /cs and verify output matches reality
```

**If post-deploy checks fail: rollback immediately.**

### 4. Rollback
```bash
# App rollback
cd /Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site
git revert HEAD
bash scripts/app-state.sh deploy

# Hook binary rollback
cd /Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks
git revert HEAD
cargo build --release
agent-state.sh restart chorus-hooks
```

---

## Service Inventory

### Native LaunchAgents (managed by agent-state.sh)
- `com.chorus.hooks` — Hook server (Rust binary, /tmp/chorus-hooks.sock)
- `com.chorus.api` — Chorus API (localhost:3340)
- `com.chorus.context-cache` — Daily context cache refresh
- All `com.chorus.*` and `com.gathering.*` agents

### App Services (managed by app-state.sh)
- Gathering app (localhost:3000)
- Fuseki (localhost:3030)
- NiFi, Loki, Grafana

### Key Ports
| Port | Service | Health Check |
|------|---------|-------------|
| 3000 | Gathering app | `curl -s localhost:3000/health` |
| 3030 | Fuseki | `curl -s localhost:3030/$/ping` |
| 3100 | Grafana | `curl -s localhost:3100/api/health` |
| 3102 | Loki | `curl -s localhost:3102/ready` |
| 3340 | Chorus API | `curl -s localhost:3340/api/chorus/health` |
| 3456 | Vikunja | `curl -s localhost:3456/api/v1/info` |

---

## Error Classification

### Errors (must fix immediately)
- Seed write failures (SPARQL update failed)
- Nudge delivery failures (osascript not injecting)
- Hook server not responding on socket
- Any service returning non-200 on health check
- Fuseki graph load failures

### Warnings (investigate within session)
- Slow Chorus search (>2s)
- Stale session index (>1hr behind)
- LaunchAgent restart loops

### Info (log only)
- Seed probe health checks
- Routine gemba ticks
- Session start/stop events

**Rule: If Jeff would notice it, it's an error. Not a warning. Not info.**

---

## Daily Health Check

Run at session start, every session:
```bash
agent-state.sh health          # All agents running?
bash ~/.chorus/scripts/check-seeds.sh  # Pipeline healthy?
curl -s localhost:3340/api/chorus/health   # Chorus API up?
```

If any of these fail, that's the first card you work on. Not whatever was planned.

---

## Incident Response

1. **Jeff reports something broken** — believe him. Investigate immediately.
2. **Check logs** — Loki first (`localhost:3102`), then local files.
3. **Find root cause** — not symptoms. "Fuseki is down" is a symptom. Why is it down?
4. **Fix and verify** — the fix isn't done until Jeff's use case works, not until tests pass.
5. **Post-incident** — what monitoring should have caught this? Card it.

---

## What "Done" Means

A deploy is done when:
- Pre-checks passed
- Code deployed
- Post-checks passed
- Jeff's primary use cases verified (seeds arrive, nudges deliver, search returns results)
- No new errors in Loki for 5 minutes

A deploy is NOT done when:
- Tests pass but you haven't sent a real seed
- Health checks return 200 but you haven't verified functional behavior
- You said "deployed" without running post-checks
