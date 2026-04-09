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

## Alerting Pipeline

### How It Works
Three layers: YAML rules → alert-runner.sh → dual delivery (Bridge POST + terminal nudge).

**Alert rules:** `chorus/proving/domains/alerts/*.yml` (8 rules: app-down, daily-review-missing, hook-server-down, nudge-stale, seed-write-failure, startup-sync-failure, synthetic-test, tunnel-down)

**Execution:**
- `com.chorus.alert-runner` — runs every 60s, evaluates YAML rules
- `com.gathering.infra-alert` — runs every 300s, infrastructure health checks
- `com.chorus.deep-health` — runs every 300s, subprocess liveness

**Delivery (DEC-107 — both paths fire every time):**
1. POST to Bridge API (localhost:3470) — persisted
2. Nudge to owning role's terminal — immediate

**Verification:**
```bash
# Check alert runner log for recent SKIP/OK/FIRE lines
tail -20 ~/Library/Logs/Chorus/alert-runner.log

# Run a single rule manually
bash chorus/proving/scripts/alert-runner.sh --rule synthetic-test
```

**Common failure:** Path drift after namespace moves. If alert-runner logs show "started" + "complete" with no rule processing between, check `ALERT_DIR` in `proving/scripts/alert-runner.sh`.

---

## Service Inventory

### Library Mac (192.168.86.36)

#### Native LaunchAgents
All managed via `launchctl`. Plists in `~/Library/LaunchAgents/`.

**Core infrastructure:**
- `com.chorus.api` — Chorus API (localhost:3340)
- `com.chorus.hooks` — Hook server (Rust binary, /tmp/chorus-hooks.sock)
- `com.chorus.alert-runner` — Alert rule evaluation (60s)
- `com.chorus.alert-notifier` — Webhook receiver (port 9095), macOS notifications
- `com.chorus.session-watcher` — Session lifecycle
- `com.chorus.inject-watcher` — Terminal injection
- `com.chorus.bridge-subscriber-{silas,wren,kade}` — Message bridge per role

**Observability:**
- `com.gathering.grafana` — Dashboards (localhost:3100)
- `com.gathering.loki` — Log aggregation (localhost:3102)
- `com.gathering.prometheus` — Metrics (localhost:9090)
- `com.gathering.promtail` — Log shipping
- `com.gathering.alertmanager` — Prometheus alert routing
- `com.gathering.node-exporter` — System metrics (port 9101)
- `com.gathering.blackbox-exporter` — Endpoint probing

**App services:**
- `com.gathering.app` — Gathering app (localhost:3000)
- `com.gathering.fuseki` — SPARQL store (localhost:3030)
- `com.gathering.mysql` — MySQL (localhost:3306)
- `com.gathering.vikunja` — Board (localhost:3456)
- `com.gathering.messaging` — Pulse messaging
- `com.gathering.wordpress` — WordPress

### Bedroom Mac (192.168.86.242)

SSH: `ssh 192.168.86.242` (do NOT use `jeff@` — config supplies username)

**Services:**
- `com.gathering.promtail` — Log shipping to Library Loki
- `com.gathering.ollama` — Local AI
- `com.gathering.images-api-server` — Photos API
- `com.gathering.images-api-video` — Video processing
- `com.gathering.node-exporter` — System metrics
- `com.gathering.navidrome` — Music server

No Docker on Bedroom. All services via LaunchAgents.

### Key Ports (Library)
| Port | Service | Health Check |
|------|---------|-------------|
| 3000 | Gathering app | `curl -s localhost:3000/health` |
| 3030 | Fuseki | `curl -s localhost:3030/$/ping` |
| 3100 | Grafana | `curl -s localhost:3100/api/health` |
| 3102 | Loki | `curl -s localhost:3102/ready` |
| 3340 | Chorus API | `curl -s localhost:3340/health` |
| 3456 | Vikunja | `curl -s localhost:3456/api/v1/info` |
| 3470 | Bridge | `curl -s localhost:3470/health` |
| 9090 | Prometheus | `curl -s localhost:9090/-/healthy` |
| 9095 | Alert notifier | webhook receiver |
| 9101 | Node exporter | `curl -s localhost:9101/metrics` |

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

## LaunchAgent Health

### Validate all plists point to existing scripts
```bash
for plist in ~/Library/LaunchAgents/com.{gathering,chorus}*.plist; do
  label=$(basename "$plist" .plist)
  script=$(/usr/libexec/PlistBuddy -c "Print :ProgramArguments:1" "$plist" 2>/dev/null)
  if [[ -n "$script" ]] && [[ "$script" == /* ]] && [[ ! -f "$script" ]]; then
    echo "MISSING  $label → $script"
  fi
done
```

After any namespace move or script relocation, run this. Agents with broken paths will exit 127 on next load.

### Reload an agent after plist change
```bash
launchctl unload ~/Library/LaunchAgents/<label>.plist
launchctl load ~/Library/LaunchAgents/<label>.plist
```

### Check agent status
```bash
launchctl list | grep <label>
# PID  ExitCode  Label
# -    127       = command not found (broken path)
# -    0         = ran successfully, not currently running
# PID  0         = running
```

---

## Log Locations

| Log | Path |
|-----|------|
| Alert runner | `~/Library/Logs/Chorus/alert-runner.log` |
| Alert delivery test | `~/Library/Logs/Chorus/alert-delivery-test.log` |
| Chorus (structured) | `~/Library/Logs/Chorus/chorus.log` |
| Infra health | `~/Library/Logs/Gathering/infra-alert.log` |
| Alert notifier | `/tmp/alert-notifier.log` |
| Deep health | `~/Library/Logs/Chorus/deep-health.log` |

For application logs, use Loki (localhost:3102) via Grafana (localhost:3100), not `docker logs`.

---

## Daily Health Check

Run at session start, every session:
```bash
# All agents running?
launchctl list | grep -E 'chorus|gathering' | grep -v '\t0\t' | grep -v 'PID'

# Key endpoints up?
for port in 3000 3030 3100 3102 3340 3456; do
  code=$(curl -s -o /dev/null -w '%{http_code}' http://localhost:$port/health 2>/dev/null)
  echo "localhost:$port → $code"
done

# Pipeline healthy?
bash ~/.chorus/scripts/check-seeds.sh

# Alerts firing?
tail -5 ~/Library/Logs/Chorus/alert-runner.log
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
