# Operations Assessment — What We Have, What Needs Fixing, What's Missing

Last updated: 2026-02-25 | Werk v1.3.20

Companion to: `spine-architecture.md` (coordination) and `chorus-clearing-architecture.md` (awareness/interaction).

---

## 1. Container Estate

**What We Have:**
15 containers on Library, all running and healthy (except MailHog — no health check). Plus 7 LaunchAgents on Library and 3 on Bedroom. Three Docker groups:

| Group | Containers | Health Checks | Memory Budget |
|-------|-----------|---------------|---------------|
| Application | Express, Fuseki, WebVOWL | 3/3 ✓ | Express unbounded, Fuseki 2GB cap, WebVOWL unbounded |
| Content | WordPress, MySQL, MailHog | 1/3 (WordPress only) | All unbounded |
| Observability | Prometheus, Grafana, Loki, Promtail, Alertmanager, Node-exporter, Blackbox, mysqld-exporter | 6/8 (blackbox + mysqld missing) | ~2GB total cap |
| Coordination | Vikunja | 1/1 ✓ | Unbounded |

**What Needs Fixing:**
- **5 containers lack health checks**: MySQL, MailHog, blackbox-exporter, mysqld-exporter, and one WordPress config gap. ADR-011 targets 100%.
- **Memory limits missing on 5 containers**: Express app, WebVOWL, WordPress, MySQL, MailHog have no Docker memory cap. On a 16GB host, any one of these can balloon and OOM the system — exactly what Fuseki did yesterday (4GB heap → 4.1GB actual → system at 97%).

**What's Missing:**
- **Container restart alerting**: We have `ContainerRestarting` (>3 in 1h) but no alert for a single restart. First restart = signal, third restart = the signal we missed.
- **Resource limit enforcement policy**: No standard for what each container gets. Should be codified — every container gets `mem_limit` + health check, no exceptions.

---

## 2. Memory & Resource Management

**What We Have:**
- Fuseki: 1g heap / 2GB Docker cap (fixed yesterday from 4g/unlimited)
- Observability stack: All 8 containers capped (~2GB total)
- Prometheus alert: `HighMemoryUsage` fires at >90% host memory

**What Needs Fixing:**
- **Express app has no memory limit**. It's a Node.js process that can grow unbounded. Default V8 heap on 16GB host = ~4GB. Should cap at 1.5-2GB.
- **WordPress + MySQL unbounded**. Low-traffic but still risk vectors.
- **Alert threshold too high**. 90% on 16GB = 14.4GB used. By the time it fires, we're already swapping. Should be 80% warning / 90% critical.

**What's Missing:**
- **Per-container memory dashboard panel**: We have aggregate host memory. We don't have per-container memory tracked over time in a dedicated panel. When the next OOM happens, we need to see which container grew.
- **OOM kill detection**: Docker logs OOM kills but we don't alert on them. A `container_oom_events` metric from cAdvisor would catch this.

---

## 3. Deploy Pipeline

**What We Have:**
- `app-state.sh` with 6 commands: start, stop, restart, deploy, rollback, status
- Deploy lock (expanded to cover start/restart/deploy/rollback)
- Health gate (30s timeout on /health endpoint)
- Rollback via `previous` image tag
- Disk check before deploy (>10GB free)

**What Needs Fixing:**
- ~~**Terraform confusion**~~: RESOLVED — #139 migrated to docker-compose. `app-state.sh` now wraps `docker compose` commands.
- **Named volume auth persistence**: Fuseki's `shiro.ini` (Shiro auth config) persists in the named volume across container recreation. If `ADMIN_PASSWORD` env var changes, the old `shiro.ini` wins — Fuseki only writes it on first boot. Fix: remove volume and recreate. Pattern applies to any service that writes config to a persistent volume on first run.
- **Deploy timing not measured**: We know deploys take ~10-22s but don't have a Prometheus metric for deploy duration. Card #138 (build health lava lamp) would close this.

**What's Missing:**
- **Automated rollback on health failure**: Deploy waits for health, but if health fails it just prints an error. Should auto-rollback to previous image.
- **Deploy event in chorus-log**: Deploys aren't logged to the coordination layer. Every deploy should emit a chorus-log event with duration, image SHA, and outcome.
- **Blue-green or canary**: Not needed at current scale, but worth noting — we do destroy-and-recreate, which means ~5s downtime per deploy.

---

## 4. Alert Routing ✅ SHIPPED (2026-02-23, #154)

**What We Have:**
- 30+ Prometheus alert rules (service, host, network, external)
- 3 Grafana/Loki alert rules (chorus operations)
- Grafana notification policy → chorus-log webhook contact point (WIRED)
- chorus-api `POST /api/chorus/alert` → chorus.log → Promtail → Loki (LIVE)
- macOS desktop notification (Basso sound) for critical + firing alerts (LIVE)
- **Chorus ops** (`chorus-ops.sh`): Unified ops daemon. `defects` subcommand queries Loki every 5 min, deduplicates error patterns, auto-creates board cards. `health` subcommand pre-fetches system state, reasons via claude -p, acts on findings. Three defect tiers: P1 critical (auto-card + assign), P2 warning (auto-card), pattern (log until threshold). Replaces former `defect-poller.sh` + `ops-agent.sh`.

**Still Needs:**
- **Alertmanager vs Grafana alerting dual-path**: Prometheus rules fire through Alertmanager (null receiver). Grafana Loki rules fire through Grafana webhook. Should converge on one path.
- **Alert acknowledgment**: No way to mark an alert as "seen" or "working on it".
- **Runbook links**: Alerts fire but don't link to remediation steps.

---

## 5. Disk Management

**What We Have:**
- C1: App I/O on local SSD (enforced)
- C2: Library disk at 76% used (1.4TB of 1.8TB, 441GB free). Music rescue (#311) freed ~1TB.
- Constraint alert: `DiskSpaceWarning` at >85%, `DiskSpaceLow` at <10%
- Jeff planning to free ~1TB of source media

**What Needs Fixing:**
- **Docker image garbage**: 121.3GB reclaimable (93% of images), 24.6GB build cache. No automated pruning. Should schedule weekly `docker system prune --filter "until=168h"` via cron.
- **Docker volume orphans**: 32 volumes, only 9 active. 23 orphans consuming 2.1GB. Minor but messy.

**What's Missing:**
- **Harvester disk impact tracking**: C7 requires disk impact estimates before approving harvesters. Music = 71MB, Photos = 1.7GB estimated. But we don't have a metric that tracks pod storage growth over time. When Photos harvester runs at full scale, we need to see the growth curve.
- **Fuseki TDB2 size monitoring**: Fuseki data volume grows with every sync. No metric tracks its size. Should be a Prometheus gauge.

---

## 6. Backup & Recovery

**What We Have:**
- `backup-pods.sh`: Daily tar.gz of SOLID pods to secondary Mac
- 7 daily + 4 weekly rotation
- Automated restore verification
- WordPress and Ghost backup scripts

**What Needs Fixing:**
- **Backup success/failure not monitored**: No Prometheus metric for backup outcome. If the cron job fails silently, nobody knows until data is needed.
- **Backup cron timing unclear**: Script exists, install script exists, but unclear if cron is actually installed and running. Needs verification.

**What's Missing:**
- **Fuseki backup**: Pod Turtle files are backed up, but Fuseki TDB2 (the SPARQL index) is not. TDB2 can be rebuilt from pods via fullSyncAll, but that takes 5+ minutes with 6k+ files. A periodic Fuseki snapshot would speed recovery.
- **Configuration backup**: Docker configs, Terraform state, Prometheus rules, Grafana dashboards — none backed up to secondary Mac. If primary SSD fails, we rebuild everything from git, but Grafana dashboard customizations (UI edits) would be lost.
- **Recovery runbook**: No documented procedure for "primary Mac dies, rebuild from scratch." Should cover: (1) restore pods from backup, (2) git clone all repos, (3) terraform apply / docker-compose up, (4) verify fullSyncAll completes, (5) verify observability stack.
- **Off-site backup**: Both copies (primary + secondary) are in the same house. House-level disaster (fire, flood, theft) loses everything. Even a single encrypted backup to cloud (S3/B2) would close this gap. Not urgent given C6 (no cloud dependencies for core), but worth flagging.

---

## 7. Monitoring & Observability

**What We Have:**
- 9 Grafana dashboards (home-cloud, home-network, app-operations, chorus-activity, docker-containers, node-metrics, logs-explorer, service-overview, cost-dashboard)
- Prometheus with 19 scrape jobs, 15s interval, 15d retention
- Loki for log aggregation (all containers via Promtail)
- Blackbox exporter: HTTP probes for all services + ICMP for 22 LAN devices
- Cost metrics exporter (Claude + Twilio + Clearing → Prometheus)
- Ambient session indexing (fswatch → chorus-index)

**What Needs Fixing:**
- **Prometheus 15d retention is short**. Trend analysis (e.g., "is Fuseki memory growing month over month?") requires longer windows. 30d minimum, 90d preferred. Disk cost: ~2GB/month at current cardinality.
- **Loki retention not configured**. Default is unlimited — will grow unbounded. Should match Prometheus at 30-90d.
- **Dashboard drift**: Provisioned dashboards allow UI edits (`allowUiUpdates: true`), which means manual changes in Grafana aren't captured in git. Any dashboard tweak made in the UI is lost on Grafana restart. Should either disable UI edits or export periodically.

**What's Missing:**
- **Application metrics**: Express app doesn't export Prometheus metrics. Request rate, latency percentiles, error rate, active connections — all invisible. A `/metrics` endpoint with `prom-client` would give us the full RED (Rate, Errors, Duration) picture.
- **Fuseki metrics**: Fuseki has a built-in metrics endpoint but we're not scraping it. Query latency, graph count, TDB2 compaction events — all available, none collected.
- **SLO/SLI framework**: We have alerts for "is it broken?" but no definition of "is it good enough?" An SLO like "99.5% of health checks pass per week" would formalize what "healthy" means.

---

## 8. Network & Security

**What We Have:**
- ADR-012: All non-app services bound to 127.0.0.1 (14/15 verified)
- 22 LAN devices monitored via ICMP (blackbox-exporter)
- Cloudflare tunnel for external access (monitored)
- SOLID OIDC (Pivot) for authentication

**What Needs Fixing:**
- **Pivot mobile login**: Status TBD — may still be broken. Blocks mobile access.
- **No HTTPS between internal services**: Services communicate over plain HTTP on localhost. Acceptable for single-host, but worth documenting as a deliberate decision (not an oversight).

**What's Missing:**
- **Container image scanning**: No Trivy or similar scanning on built images. Pre-commit hook runs Trivy on code, but Docker images may contain vulnerable base packages. Card #79 (external security scanning spike) tracks this.
- **Secrets management**: Credentials in `terraform.tfvars` and `.env` files. No HashiCorp Vault or similar. Acceptable for single-user local infra, but should be documented as deliberate.
- **Network segmentation**: All 15 containers share the same Docker networks. Observability stack shouldn't need to reach application containers directly (only via metrics endpoints). A stricter network policy would reduce blast radius.

---

## 9. Cost & Capacity

**What We Have:**
- Cost dashboard in Grafana (Claude usage, Twilio, Clearing)
- Python exporter scanning JSONL session files every 5 min
- Fixed cost: $200/mo (Claude Code Max)
- Variable costs tracked: Twilio SMS, Clearing sessions

**What Needs Fixing:**
- **Cost exporter cron needs verification**: Setup instructions exist but unclear if cron is actually running. `crontab -l` should show the entry.
- **cost-log.md compliance**: End-of-session cost entries are inconsistent. Some sessions log, some don't. Fitness function F5 would track this.

**What's Missing:**
- **Capacity planning model**: 16GB RAM, 15 containers. No model for "when do we need the secondary Mac for compute?" As harvesters scale (Photos at 1.7GB, potential Video harvester with 178TB), system load will grow. Need a projection: at what harvester scale does the primary Mac become the bottleneck?
- **Docker resource accounting**: Total memory allocated vs available. With Fuseki at 2GB and observability at ~2GB, that's 4GB committed of 16GB. Express, WordPress, MySQL, WebVOWL, Vikunja = 5 unbounded containers sharing the remaining 12GB (minus OS overhead). Should be tracked.

---

## 10. Operational Processes

**What We Have:**
- Session start gate (enforced via hook)
- Board audit on session start/close (staleness detection)
- Chorus gate registry (209 rules, 18 enforced)
- Deploy lock (cross-role protection)
- Activity.md audit trail
- Werk workflow engine (auto-triggered on card move to Now)

**What Needs Fixing:**
- **191 doc-only rules**: Only 18 of 209 rules are machine-enforced (8.6%). The Stop hook alone would automate the 10-step close-out checklist. Phase 1 (13 rules via Stop + PostToolUse) is the highest-value ops improvement. Mapped last session but not yet carded.
- **Close-out compliance**: End-of-session review happens inconsistently. The session-init-gate works (enforced on start), but there's no equivalent for session close.

**What's Missing:**
- **Incident log**: When things break (like yesterday's Fuseki OOM), there's no structured incident record. We fix it, update state files, maybe write an ADR — but no postmortem template, no "what broke, when, how long, what fixed it, what prevents recurrence" record. Even a simple `incidents/` directory with dated files would help.
- **Operational runbooks**: "Fuseki is OOM" → what do you do? "App won't start" → check what? "Backup failed" → restore how? Currently all tribal knowledge in CLAUDE.md and state files. Should be extracted into step-by-step runbooks.
- **Change log**: Infrastructure changes (yesterday's Fuseki memory fix, alert routing change) aren't tracked separately from architectural decisions. ADRs capture design decisions, but ops changes need their own log — lighter weight, dated, "what changed and why."

---

## Summary Scorecard

| Domain | Have | Fix | Missing | Health |
|--------|------|-----|---------|--------|
| Container estate | 15/15 running | 5 missing health checks, 5 missing mem limits | Restart alerting, resource policy | 🟡 |
| Memory management | Fuseki capped, obs capped | Express/WP/MySQL unbounded, alert threshold | Per-container tracking, OOM detection | 🟡 |
| Deploy pipeline | app-state.sh, lock, health gate | Terraform confusion (#139 fixes), no metrics | Auto-rollback, deploy events, blue-green | 🟡 |
| Alert routing | 30+ rules, webhook wired, desktop notifs, defect poller | Dual alerting paths | Ack workflow, runbook links | 🟢 |
| Disk management | C1/C2 enforced, 77% free | 121GB Docker garbage, no prune cron | Harvester growth tracking, TDB2 size | 🟢 |
| Backup & recovery | Daily pods backup, rotation | No monitoring, cron unverified | Fuseki backup, config backup, recovery runbook, off-site | 🟡 |
| Monitoring | 9 dashboards, 19 scrape jobs | Retention short, Loki unbounded, dashboard drift | App metrics, Fuseki metrics, SLOs | 🟡 |
| Network & security | ADR-012, ICMP probes, Cloudflare | Pivot mobile login | Image scanning, secrets mgmt, segmentation | 🟢 |
| Cost & capacity | Dashboard, exporter, budget | Cron verification, compliance | Capacity model, resource accounting | 🟢 |
| Processes | Init gate, board audit, workflows | 191 unenforced rules, close-out gaps | Incident log, runbooks, change log | 🟡 |

**Overall: 🟡 — Solid foundation, specific gaps in enforcement and resilience.**

The system works well day-to-day. The risk is in the edges: what happens when something fails (no auto-rollback, no recovery runbook, no backup monitoring), and what happens as load grows (no capacity model, no per-container tracking, no harvester growth metrics). Yesterday's Fuseki OOM was the kind of event that exposes these gaps — it cascaded to app health, tunnel failure, and 97% memory because nothing was capped or tracked at the container level.

---

## Priority Actions (from this assessment)

| # | Action | Effort | Impact | Status |
|---|--------|--------|--------|--------|
| 1 | ~~Wire alert webhook to notification policy~~ | 15 min | Alert routing functional | ✅ DONE |
| 2 | Add `mem_limit` to Express, WordPress, MySQL, WebVOWL, MailHog | 30 min | Prevents next OOM cascade | #139 for Express |
| 3 | Card the Stop hook (close-out enforcement) | 10 min | Automates 10-step checklist | — |
| 4 | Docker prune cron (weekly) | 15 min | Recovers 121GB | — |
| 5 | Verify backup + cost cron are running | 10 min | Confirms assumed-healthy systems | — |
| 6 | Add health checks to remaining 5 containers | 30 min | ADR-011 complete | — |
| 7 | Recovery runbook (primary Mac failure) | 1 hr | Disaster readiness | — |
| 8 | Incident log template + yesterday's Fuseki OOM as first entry | 30 min | Process improvement | — |
| 9 | Express `/metrics` endpoint | 1 hr | App-level observability | Kade |
| 10 | Prometheus + Loki retention config | 15 min | Trend analysis, bounded growth | — |
| 11 | ~~Defect poller (Loki → dedupe → auto-card)~~ | 2 hr | Defect escape hatch | ✅ DONE |
| 12 | ~~macOS desktop notifications for critical alerts~~ | 15 min | Human-facing alerts | ✅ DONE |
