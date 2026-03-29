# Capability Map

Last updated: 2026-02-13

Jeff's approach: concentric circles — data, security, features, automation — each pass informs the next ring outward. This map tracks what's solid (built on) vs. what's next (build toward).

## How to Read This

- **As-Is**: What exists today and its maturity (Solid / Functional / Partial / Stubbed / Missing)
- **To-Be**: What the target capability looks like — filled in collaboratively with Jeff
- **Gap**: What's needed to close the distance
- Domains: Data, Security, App, Automation — across all projects

---

## 1. DATA

### Ontology
- **Maturity**: Solid
- **As-Is**: v0.4.0 in code. Domains: Property, Books, Blog, Gallery, Profile, Ideas. SHACL shapes exist but minimal (96 lines).
- **To-Be**: _TBD with Jeff_
- **Gap**: Docs previously lagged code (fixed); SHACL shapes could be expanded for validation

### SOLID Pods
- **Maturity**: Solid
- **As-Is**: Filesystem Turtle files in `data/pods/jeff/`. Subdirectories: blog, books, property, projects, ideas, taxonomy, profile, admin.
- **To-Be**: _TBD with Jeff_
- **Gap**: Single-pod, single-user. No federation beyond reference stubs

### Fuseki (SPARQL)
- **Maturity**: Functional
- **As-Is**: Operational. SparqlService wraps HTTP to Fuseki at localhost:3030. FusekiSyncService fire-and-forgets after pod writes.
- **To-Be**: _TBD with Jeff_
- **Gap**: Sync reliability untested at scale; no scaling plan; index is read-only (good)

### WordPress to Pod Sync
- **Maturity**: Functional
- **As-Is**: Webhook mu-plugin fires on post lifecycle. Personal site receives at `/api/webhook/wordpress`. Harvester converts WP posts to Turtle.
- **To-Be**: _TBD with Jeff_
- **Gap**: Fire-and-forget; no retry/dead-letter; webhook endpoint hardcoded

### Google Photos Import
- **Maturity**: Functional
- **As-Is**: OAuth2 flow for importing photos into property albums.
- **To-Be**: _TBD with Jeff_
- **Gap**: —

### Book Upload Pipeline
- **Maturity**: Solid
- **As-Is**: Multi-step: photo upload, Claude Vision classify, Open Library metadata, pod write.
- **To-Be**: _TBD with Jeff_
- **Gap**: —

### Data Export
- **Maturity**: Missing
- **As-Is**: None. Data persists as Turtle files (portable by nature).
- **To-Be**: _TBD with Jeff_
- **Gap**: No explicit export/backup mechanism for pods

### Observability Data
- **Maturity**: Solid
- **As-Is**: Prometheus: 15d retention, 8 scrape jobs. Loki: 7d log retention, JSON parsing, correlationId linking. 5 dashboards.
- **To-Be**: _TBD with Jeff_
- **Gap**: Retention policies may need tuning as data grows

---

## 2. SECURITY

### Authentication
- **Maturity**: Solid
- **As-Is**: SOLID OIDC via Pivot (solidcommunity.net, inrupt.com). Express session with 24h cookies. SolidAuthService at 100% test coverage.
- **To-Be**: _TBD with Jeff_
- **Gap**: —

### Authorization (ACLs)
- **Maturity**: Functional
- **As-Is**: WAC-based. AclService (471 lines) handles private/shared/public. PodWriteService is write choke point.
- **To-Be**: _TBD with Jeff_
- **Gap**: AclService coverage 77%, AclHandler only 20%. Graduation model architected but edge cases untested

### CSRF Protection
- **Maturity**: Solid
- **As-Is**: Custom middleware with token validation on POST/PUT/DELETE.
- **To-Be**: _TBD with Jeff_
- **Gap**: —

### Rate Limiting
- **Maturity**: Solid
- **As-Is**: Separate limiters for general API, auth, webhooks, pod ops.
- **To-Be**: _TBD with Jeff_
- **Gap**: Limits not yet tuned from defaults

### Security Headers
- **Maturity**: Solid
- **As-Is**: Helmet.js with strict CSP (nonce-based), CORS whitelist for SOLID + Google APIs.
- **To-Be**: _TBD with Jeff_
- **Gap**: —

### Webhook Security
- **Maturity**: Functional
- **As-Is**: HMAC validation for WordPress webhooks. Shared secret via env var.
- **To-Be**: _TBD with Jeff_
- **Gap**: HTTP only (no TLS for internal); acceptable for local

### Secrets Management
- **Maturity**: Partial
- **As-Is**: Env vars, some hardcoded in Terraform/Compose (WP creds, Grafana admin, MySQL).
- **To-Be**: _TBD with Jeff_
- **Gap**: No centralized secrets management. Dev-acceptable, not production-ready

### Observability Auth
- **Maturity**: Stubbed
- **As-Is**: Grafana: admin/admin, signup disabled. Prometheus/Loki: no auth.
- **To-Be**: _TBD with Jeff_
- **Gap**: Open APIs on trusted network. Needs reverse proxy + OAuth2 for any external exposure

### TLS/HTTPS
- **Maturity**: Partial
- **As-Is**: Self-signed certs for dev. Cloudflare tunnel script exists. CSP/HSTS headers ready.
- **To-Be**: _TBD with Jeff_
- **Gap**: No end-to-end TLS in local stack; tunnel is outbound-only

---

## 3. APP (Features)

### Books
- **Maturity**: Solid
- **As-Is**: Full CRUD. Upload, classify (Claude Vision), Open Library metadata, pod write. 95% test coverage.
- **To-Be**: _TBD with Jeff_
- **Gap**: —

### Property
- **Maturity**: Functional
- **As-Is**: Houses, gardens, garden beds, rooms, lands, photos. Google Photos integration. Largest handler (1076 lines).
- **To-Be**: _TBD with Jeff_
- **Gap**: 20% test coverage; handler needs decomposition

### Blog
- **Maturity**: Functional
- **As-Is**: WordPress harvest to Turtle. Collection view. 40+ post files in pods.
- **To-Be**: _TBD with Jeff_
- **Gap**: 19% handler coverage; read-only from WP (no round-trip editing)

### Gallery
- **Maturity**: Functional
- **As-Is**: Image listing, proxy with Sharp optimization, Finder-style search.
- **To-Be**: _TBD with Jeff_
- **Gap**: Feature parity unclear — may need more work

### Ideas/Projects
- **Maturity**: Functional
- **As-Is**: Incubation board. Idea + Project CRUD. Collection grouping. Visibility transitions.
- **To-Be**: _TBD with Jeff_
- **Gap**: —

### Profile
- **Maturity**: Solid
- **As-Is**: Identity, SOLID ACLs, photo upload/removal. 88% coverage.
- **To-Be**: _TBD with Jeff_
- **Gap**: —

### Admin/Dashboard
- **Maturity**: Functional
- **As-Is**: User management, SPARQL query tool, activity log, aggregate stats.
- **To-Be**: _TBD with Jeff_
- **Gap**: 24% handler coverage

### Conversational AI
- **Maturity**: Missing
- **As-Is**: Not built. Claude API used for book classification only.
- **To-Be**: _TBD with Jeff_
- **Gap**: Listed in open architectural concerns

### WordPress Site
- **Maturity**: Solid
- **As-Is**: Full WP instance. Mu-plugins for webhook + theme API fix. 16 themes, 7 plugins. Swagger docs (port mismatch).
- **To-Be**: _TBD with Jeff_
- **Gap**: Theme bloat (16 installed, few active); deploy job stubbed

---

## 4. AUTOMATION

### CI/CD (Personal Site)
- **Maturity**: Functional
- **As-Is**: GitHub Actions: test, security, lint, terraform validate, build. Artifact packaging.
- **To-Be**: _TBD with Jeff_
- **Gap**: Pipeline is permissive — test failures don't block. No staging env

### CI/CD (WordPress)
- **Maturity**: Partial
- **As-Is**: GitHub Actions: 4 test suites, deploy stub.
- **To-Be**: _TBD with Jeff_
- **Gap**: Deploy job is a stub; port mismatch in perf tests

### Docker/Terraform (Personal Site)
- **Maturity**: Solid
- **As-Is**: Dockerfile (node:18-alpine), Terraform modules for SOLID pod server, Fuseki, observability network join. `app-state.sh` for lifecycle.
- **To-Be**: _TBD with Jeff_
- **Gap**: —

### Docker/Terraform (WordPress)
- **Maturity**: Solid
- **As-Is**: Docker Compose via Terraform. MySQL volume with prevent_destroy. Network isolation + observability join.
- **To-Be**: _TBD with Jeff_
- **Gap**: —

### Docker Compose (Observability)
- **Maturity**: Solid
- **As-Is**: 7 containers, named volumes, restart policies, management script with health/targets/logs.
- **To-Be**: _TBD with Jeff_
- **Gap**: —

### Testing (Personal Site)
- **Maturity**: Functional
- **As-Is**: Jest: 72 test files, 72% overall coverage. Playwright e2e (chromium). Pre-commit hook (Trivy + lint + unit). Coverage threshold: 80%/75%/60%.
- **To-Be**: _TBD with Jeff_
- **Gap**: Below 80% threshold in aggregate; large handler coverage gaps

### Testing (WordPress)
- **Maturity**: Functional
- **As-Is**: 4 bash suites: infra, app, security, performance.
- **To-Be**: _TBD with Jeff_
- **Gap**: Basic but effective; no unit-level WP testing

### Alerting
- **Maturity**: Partial
- **As-Is**: Prometheus rules defined (10 alerts, 4 groups). No Alertmanager.
- **To-Be**: _TBD with Jeff_
- **Gap**: Alerts fire but don't notify. ALERTING.md documents future path

### Backup/Restore
- **Maturity**: Partial
- **As-Is**: WordPress: full tar.gz + WP-CLI export/restore. Pod data: no explicit backup.
- **To-Be**: _TBD with Jeff_
- **Gap**: Pod data backup is a gap — Turtle files are portable but no automated backup

### Deployment
- **Maturity**: Stubbed
- **As-Is**: Cloudflare tunnel script (outbound-only). No production deployment pipeline.
- **To-Be**: _TBD with Jeff_
- **Gap**: Local dev only. Production path not yet defined

---

## Cross-Cutting Concerns

### Observability Integration
- **Status**: Solid
- All projects on observability-network. Structured JSON logging with correlationId. Prometheus metrics endpoint.

### Cross-Project Coordination
- **Status**: Documented
- ADR-001 defines ownership. Network membership lives in each project's Terraform.

### Ontology Coherence
- **Status**: Needs attention
- Code at v0.4.0, docs now updated. Ontology changes ripple to SPARQL, UI, AI context.

### Production Readiness
- **Status**: Not targeted yet
- Secrets, TLS, deployment, alerting all need work before any external exposure.

---

## Notes

- To-Be columns need Jeff's input — I can see what's built, but the target requires intent
- This map should be updated as each "concentric circle" pass completes
- Items marked Solid are foundation you can build on; Partial/Stubbed are the active edges
