# Brief: Remaining Guardrails — Status & Decisions Needed

**From**: Silas (Architect)
**To**: Wren (PM)
**Date**: 2026-02-14
**Re**: Guardrails gap closure progress and remaining items

---

## Where We Are

Started with 9 guardrail gaps. 4 are closed:

| Gap | Status | Closed by |
|-----|--------|-----------|
| ~~Pre-commit hook~~ | **FIXED** | Kade — Husky installed |
| ~~Dependabot~~ | **FIXED** | Kade — `.github/dependabot.yml` added |
| ~~CodeQL blocking~~ | **FIXED** | Kade — `continue-on-error` removed |
| ~~Dead code detection~~ | **FIXED** | Kade — knip + jscpd + tsconfig flags |

5 remain. I've briefed Kade on all of them.

---

## Buildable Now (3 items, ~1-1.5 hours total)

### Fuseki in CI + SHACL Validation
**Risk being mitigated**: SPARQL queries and ontology schema are untested in the CI pipeline. A broken ontology or SPARQL regression could merge to main without detection.

**What changes**: Fuseki runs as a service container in GitHub Actions. SHACL validation becomes a blocking CI gate. The ontology-is-architecture principle gets enforcement teeth.

**Priority input needed**: This is the highest-value remaining guardrail work. Recommend it goes next after any in-progress items (SMS capture v2, etc).

### Alert Routing
**Risk being mitigated**: Prometheus fires 8 alert rules into nothing. If Express goes down at 2am, nobody knows until Jeff opens his laptop.

**What changes**: Alertmanager container joins the monitoring stack, routes critical alerts to `#all-gathering` Slack channel. ServiceDown and DiskSpaceLow are critical; everything else is warning-level.

**Priority input needed**: Lower urgency than Fuseki-in-CI (Jeff is the only user, and he can see the dashboard). But it's a real gap for when harvesters start running and the system gets less manually supervised.

---

## Blocked on Decisions (2 items)

### Off-Machine Backups
**Risk**: All backups are local. A disk failure loses the backups along with the data. This is the highest-risk gap in the entire guardrail chain.

**Decision needed from Jeff**: Where should off-machine backups go?
- **NAS/external drive** (local network, fast, free) — if Jeff has one
- **Cloud storage** (S3, Backblaze B2, etc.) — $1-5/month for this data volume
- **VPS/remote server** — if Jeff has one

The build work is ~15 minutes once the destination is known. The backup script already creates verified tar.gz archives — just needs a copy step appended.

**Recommendation**: This should be Jeff's next infrastructure decision. Everything else has redundancy except the data itself.

### Automated Rollback
**Risk**: A bad deploy requires manual intervention to recover. Current mitigation is health checks + restart policies (covers crashes) and daily backups (covers data).

**Decision needed**: This is an architectural decision about deployment strategy (simple Docker tag rollback vs blue-green vs full pipeline). Given current scale (single user, single machine), manual redeploy is adequate. Recommend deferring to a future sprint when deploy frequency increases or when the system goes public.

---

## Remaining Risk Landscape

After the buildable work ships:

| Layer | Coverage |
|-------|----------|
| Pre-commit | Full (Husky + Trivy + lint + tests) |
| CI pipeline | Full (tests + E2E + security + SHACL + build) |
| Container runtime | Adequate (health checks, restart, non-root) |
| Monitoring | **Full** (alerts now route to Slack) |
| Data protection | **Gap** — on-machine only |
| Code quality | Full (strict TS, coverage thresholds, complexity) |
| Dead code | Full (knip, jscpd, tsconfig flags) |

One real gap remains: off-machine backups. Everything else is covered or has a conscious deferral (automated rollback).

— Silas
