# Brief: Foundation Sprint — Security, Pipelines, Observability, API Docs

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-15
**Priority**: P1 — this blocks feature work. Jeff's direction: foundation before features.
**Supersedes**: `2026-02-14-remaining-guardrails-for-kade.md` (folded in here with additions)

---

## Context

Jeff made a clear architectural call: security, data integrity, engineering pipelines, and observability are not features — they're the ground the features stand on. No new feature work (SMS v2, music harvester, etc.) until these four pillars are roughly healthy.

This brief bundles all remaining foundation work into one sprint, sequenced for minimum friction.

---

## Sprint Sequence

### Phase 1: CI Pipeline Completeness (~1 hour)

#### 1A. Fuseki Service Container in CI (~30-45 min)

E2E tests currently skip all SPARQL paths. Add Fuseki as a GitHub Actions service container in the E2E job.

In `.github/workflows/ci-cd.yml`, add to the `e2e` job:

```yaml
services:
  fuseki:
    image: stain/jena-fuseki
    ports:
      - 3030:3030
    env:
      FUSEKI_DATASET_1: gathering
      ADMIN_PASSWORD: admin
    options: >-
      --health-cmd "wget -q --spider http://localhost:3030/$/ping || exit 1"
      --health-interval 10s
      --health-timeout 5s
      --health-retries 5
      --health-start-period 30s
```

After pod scaffold step, load the ontology:
```bash
curl -X POST http://localhost:3030/gathering/data \
  -H "Content-Type: text/turtle" \
  --data-binary @src/ontology/jb-ontology.ttl
```

Set `FUSEKI_URL=http://localhost:3030` in E2E env.

**Verify**: Dashboard SPARQL tests work in CI, not just locally.

#### 1B. SHACL Validation in CI (~15 min)

Add a standalone CI job (runs in parallel with existing jobs):

```yaml
ontology-validate:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - name: Validate TTL syntax
      run: |
        docker run --rm -v ${{ github.workspace }}/src/ontology:/data \
          stain/jena riot --validate /data/jb-ontology.ttl
    - name: SHACL conformance
      run: |
        docker run --rm -v ${{ github.workspace }}/src/ontology:/data \
          stain/jena shacl validate \
            --shapes /data/jb-ontology-shapes.ttl \
            --data /data/jb-ontology.ttl
```

**This blocks the build.** A broken ontology is a structural failure.

---

### Phase 2: Observability Activation (~45 min)

The configs are already written (Silas, 2026-02-15). These steps activate them.

#### 2A. Connect containers to observability network (~10 min)

WebVOWL: `terraform apply` in `terraform/environments/dev/` — the updated `webvowl.tf` adds the observability network.

Vikunja: `cd ../messages/vikunja && docker-compose up -d` — the updated docker-compose adds the observability network.

**Verify**: `docker inspect <container> | grep observability-network` for both.

#### 2B. Restart Prometheus (~5 min)

```bash
docker restart prometheus
```

**Verify**: Check `http://localhost:9090/targets` — should show 3 new targets:
- `blackbox-app` (Express health)
- `blackbox-webvowl` (WebVOWL)
- `blackbox-vikunja` (Vikunja)

All should be "UP" within 30 seconds.

#### 2C. Grafana dashboard panels for new probes (~30 min)

In Grafana (`http://localhost:3100`), update the **Docker Containers** or **Service Overview** dashboard:

Add a "Service Health" row with stat panels:

```
Query: probe_success{job="blackbox-app"}
Title: Express App
Thresholds: 0=red, 1=green

Query: probe_success{job="blackbox-fuseki"}
Title: Fuseki
Thresholds: 0=red, 1=green

Query: probe_success{job="blackbox-webvowl"}
Title: WebVOWL
Thresholds: 0=red, 1=green

Query: probe_success{job="blackbox-vikunja"}
Title: Vikunja
Thresholds: 0=red, 1=green

Query: probe_success{job="blackbox-wordpress"}
Title: WordPress
Thresholds: 0=red, 1=green
```

Export the updated dashboard JSON and save to `shared-observability/dashboards/`.

---

### Phase 3: Alert Routing (~30-45 min)

#### 3A. Alertmanager container

Add to `shared-observability/docker-compose.yml`:

```yaml
alertmanager:
  image: prom/alertmanager:latest
  container_name: alertmanager
  restart: unless-stopped
  ports:
    - "9093:9093"
  volumes:
    - ./config/alertmanager:/etc/alertmanager
  command:
    - '--config.file=/etc/alertmanager/alertmanager.yml'
  networks:
    - observability-network
```

#### 3B. Alertmanager config

Create `shared-observability/config/alertmanager/alertmanager.yml`:

```yaml
global:
  resolve_timeout: 5m

route:
  group_by: ['alertname', 'severity']
  group_wait: 30s
  group_interval: 5m
  repeat_interval: 4h
  receiver: 'slack-default'
  routes:
    - match:
        severity: critical
      receiver: 'slack-critical'
      repeat_interval: 1h

receivers:
  - name: 'slack-default'
    slack_configs:
      - api_url_file: /etc/alertmanager/slack_webhook_url
        channel: '#all-gathering'
        title: '{{ .GroupLabels.alertname }}'
        text: '{{ range .Alerts }}{{ .Annotations.summary }}{{ end }}'
        send_resolved: true

  - name: 'slack-critical'
    slack_configs:
      - api_url_file: /etc/alertmanager/slack_webhook_url
        channel: '#all-gathering'
        title: 'CRITICAL: {{ .GroupLabels.alertname }}'
        text: '{{ range .Alerts }}{{ .Annotations.summary }}{{ end }}'
        send_resolved: true
```

**Security note**: Use `api_url_file` (reads from file) not `api_url` (inline). Store the Slack webhook URL in `config/alertmanager/slack_webhook_url` (plain text, one line). This file should NOT be committed to git — add to `.gitignore`.

#### 3C. Create Slack incoming webhook

In the Gathering Slack workspace:
1. Go to api.slack.com → Your Apps → Gathering Bot
2. Features → Incoming Webhooks → Add New Webhook to Workspace
3. Select `#all-gathering` channel
4. Copy the webhook URL to `config/alertmanager/slack_webhook_url`

#### 3D. Verify

Stop a container briefly, wait 2 minutes, check `#all-gathering` for the alert. Restart the container, wait for the "resolved" message.

---

### Phase 4: API Documentation — Swagger/OpenAPI (~1.5-2 hours)

#### 4A. Install dependencies (~5 min)

```bash
npm install swagger-autogen swagger-ui-express
npm install -D @types/swagger-ui-express
```

#### 4B. Create swagger config (~20 min)

Create `src/swagger.ts`:

```typescript
import swaggerAutogen from 'swagger-autogen';

const doc = {
  info: {
    title: 'Gathering API',
    description: 'Jeff Bridwell Personal Site — SOLID pods, RDF/Turtle, semantic memory',
    version: '1.0.0',
  },
  host: 'localhost:3000',
  schemes: ['http'],
  tags: [
    { name: 'Auth', description: 'SOLID OIDC authentication' },
    { name: 'Books', description: 'Book collection CRUD' },
    { name: 'Property', description: 'Property/houses/gardens/beds CRUD' },
    { name: 'Ideas', description: 'Idea collection CRUD' },
    { name: 'Projects', description: 'Project collection CRUD' },
    { name: 'Capture', description: 'SMS capture channel + triage' },
    { name: 'Gallery', description: 'Local media gallery' },
    { name: 'Dashboard', description: 'SPARQL, sync, analytics' },
    { name: 'Visibility', description: 'Collection access control' },
    { name: 'Admin', description: 'User management, ACLs, groups' },
    { name: 'Pods', description: 'Low-level pod operations' },
    { name: 'Webhooks', description: 'WordPress + Twilio webhooks' },
    { name: 'Profile', description: 'User profile management' },
  ],
};

const outputFile = './src/swagger-output.json';
const routes = ['./src/app.ts'];

swaggerAutogen({ openapi: '3.0.0' })(outputFile, routes, doc);
```

#### 4C. Wire Swagger UI into the app (~10 min)

In `app.ts`, after middleware setup but before routes:

```typescript
import swaggerUi from 'swagger-ui-express';
import swaggerDocument from './swagger-output.json';

// API docs — admin-only (security: don't expose endpoint structure to public)
app.use('/api-docs', adminMiddleware(logger), swaggerUi.serve, swaggerUi.setup(swaggerDocument, {
  customSiteTitle: 'Gathering API',
  customCss: '.swagger-ui .topbar { display: none }',
}));
```

**Security**: The `/api-docs` route is gated behind `adminMiddleware`. API endpoint structure is not public information. This is a security-sensitive route.

#### 4D. Add npm scripts (~5 min)

```json
"swagger:generate": "ts-node src/swagger.ts",
"swagger:dev": "npm run swagger:generate && npm start"
```

Add `swagger:generate` to the `prepare` script so the spec regenerates on build (alongside `sync-docs`).

#### 4E. Annotate routes with tags (~30-60 min)

swagger-autogen reads `#swagger.tags` comments in route handlers. Add to each route group in `app.ts`:

```typescript
// Books
app.get('/api/books', apiLimiter, apiBooksVisibility, bookHandler.listBooks);
// #swagger.tags = ['Books']
// #swagger.summary = 'List all books'
// #swagger.description = 'Returns books in the collection. Visibility-gated.'
```

This is the bulk of the work — ~70 endpoints need tags. swagger-autogen picks up method, path, and parameters automatically; tags and descriptions are the manual part.

#### 4F. Add to CSP if needed

If Swagger UI uses inline scripts, update the Helmet CSP in `app.ts` to allow the swagger-ui-express source.

---

### Phase 5: Per-Endpoint Metrics Dashboard (~30 min)

#### 5A. Grafana panel for endpoint performance

`express-prom-bundle` already emits `http_request_duration_seconds` with labels `method`, `path`, `status_code`. Create a Grafana panel:

**Table panel — "Endpoint Performance"**:
```promql
# Request rate per endpoint
sum(rate(http_requests_total[5m])) by (method, path)

# Error rate per endpoint
sum(rate(http_requests_total{status_code=~"5.."}[5m])) by (method, path)
/ sum(rate(http_requests_total[5m])) by (method, path)

# P95 latency per endpoint
histogram_quantile(0.95, sum(rate(http_request_duration_seconds_bucket[5m])) by (le, method, path))
```

Make the table sortable by rate, error rate, and latency. Add a link column to `/api-docs` for each endpoint.

Export and save to `shared-observability/dashboards/`.

---

## Total Estimate

| Phase | Work | Time |
|-------|------|------|
| 1. CI pipeline | Fuseki service + SHACL job | ~1 hr |
| 2. Observability | Container networks + Prometheus + Grafana panels | ~45 min |
| 3. Alert routing | Alertmanager + Slack webhook | ~45 min |
| 4. API docs | Swagger setup + route annotations | ~1.5-2 hr |
| 5. Metrics dashboard | Per-endpoint Grafana panel | ~30 min |
| **Total** | | **~4.5-5 hr** |

---

## Sequencing Rules

- Phase 1 (CI) is independent — can start immediately
- Phase 2 depends on Silas's config changes (already done)
- Phase 3 is independent
- Phase 4 is independent but benefits from Phase 1 (swagger spec generation can be a CI step)
- Phase 5 depends on Phase 2 (Prometheus must be running with new config)

Phases 1, 3, and 4 can run in parallel if convenient.

---

## What This Unblocks

After this sprint, the four pillars are roughly healthy:

| Pillar | Before | After |
|--------|--------|-------|
| **Security** | E2E at 95%, auth solid | Same + API structure not publicly exposed (Swagger behind admin) |
| **Data** | Backups, SHACL locally | + SHACL enforced in CI pipeline |
| **Pipelines** | Husky, Dependabot, CodeQL, knip | + Fuseki E2E in CI, ontology validation gate, API spec generation |
| **Observability** | Core services monitored | + All containers probed, alerts route to Slack, per-endpoint metrics |

Then feature work (SMS v2, music harvester, vis.js graph, etc.) builds on a sound base.

— Silas
