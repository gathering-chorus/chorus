# Brief: Remaining Guardrails — Build Spec

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-14
**Priority**: P2 — these are real gaps, not urgent fires
**Context**: 4 of 9 guardrail gaps are closed (Husky, Dependabot, CodeQL, dead code). 5 remain. 3 are buildable now, 2 are blocked on Jeff decisions.

---

## Buildable Now

### 1. Fuseki Service Container in CI (~30-45 min)

**Problem**: E2E tests skip all SPARQL paths. The CI comment says it explicitly: "No Fuseki/SPARQL backend in CI."

**What to do**: Add Fuseki as a service container in the E2E job of `.github/workflows/ci-cd.yml`.

```yaml
# In the e2e job, add a services block:
e2e:
  needs: test
  runs-on: ubuntu-latest
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

Then in the E2E setup steps, after scaffolding the pod directory:
```bash
# Load ontology into Fuseki for SPARQL tests
curl -X POST http://localhost:3030/gathering/data \
  -H "Content-Type: text/turtle" \
  --data-binary @src/ontology/jb-ontology.ttl
```

Set `FUSEKI_URL=http://localhost:3030` in the E2E env so the app connects.

**Verify**: Any E2E test that hits SPARQL (dashboard queries, cross-collection traversals) should now work instead of being skipped or mocked.

---

### 2. SHACL Validation in CI (~15 min, depends on #1)

**Problem**: `validate-ttl.sh` runs SHACL validation via the Fuseki container. In CI, no Fuseki means SHACL is silently skipped. Ontology schema violations only caught locally.

**What to do**: Once Fuseki is running in CI (#1), add a step to the CI pipeline that runs SHACL validation.

Two options:

**Option A — Add to E2E job** (simpler, Fuseki already running):
```yaml
- name: Validate ontology (TTL + SHACL)
  run: |
    # TTL syntax check via riot (inside Fuseki container or standalone)
    docker run --rm -v ${{ github.workspace }}/src/ontology:/data \
      stain/jena riot --validate /data/jb-ontology.ttl

    # SHACL validation
    docker run --rm -v ${{ github.workspace }}/src/ontology:/data \
      stain/jena shacl validate \
        --shapes /data/jb-ontology-shapes.ttl \
        --data /data/jb-ontology.ttl
```

**Option B — Standalone job** (cleaner separation):
```yaml
ontology-validate:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - name: Validate TTL syntax
      run: |
        docker run --rm -v ./src/ontology:/data stain/jena \
          riot --validate /data/jb-ontology.ttl
    - name: SHACL conformance
      run: |
        docker run --rm -v ./src/ontology:/data stain/jena \
          shacl validate --shapes /data/jb-ontology-shapes.ttl --data /data/jb-ontology.ttl
```

I'd suggest **Option B** — keeps it fast (~30s), doesn't couple to E2E, and can run in parallel with other jobs. The `stain/jena` image has both `riot` and `shacl` CLI tools.

**This should block the build.** If the ontology doesn't conform to shapes, that's a real problem.

---

### 3. Alert Routing via Alertmanager (~30-45 min)

**Problem**: Prometheus has 8 alert rules defined in `shared-observability/config/prometheus/rules/common-alerts.yml`. They evaluate correctly. But `alertmanagers: []` in `prometheus.yml` — alerts fire into the void.

**What to do**:

**Step 1**: Create Alertmanager config at `shared-observability/config/alertmanager/alertmanager.yml`:

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
      - api_url: '${SLACK_WEBHOOK_URL}'
        channel: '#all-gathering'
        title: '{{ .GroupLabels.alertname }}'
        text: '{{ range .Alerts }}{{ .Annotations.summary }}{{ end }}'
        send_resolved: true

  - name: 'slack-critical'
    slack_configs:
      - api_url: '${SLACK_WEBHOOK_URL}'
        channel: '#all-gathering'
        title: 'CRITICAL: {{ .GroupLabels.alertname }}'
        text: '{{ range .Alerts }}{{ .Annotations.summary }}{{ end }}'
        send_resolved: true
```

**Step 2**: Add Alertmanager to the Docker Compose in shared-observability:

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

**Step 3**: Wire Prometheus to Alertmanager — update `prometheus.yml`:

```yaml
alerting:
  alertmanagers:
    - static_configs:
        - targets:
            - alertmanager:9093
```

**Step 4**: Create a Slack incoming webhook for the Gathering workspace and set `SLACK_WEBHOOK_URL` in the shared-observability env.

**Verify**: Trigger a test alert (stop a container briefly) and confirm the Slack message arrives.

---

## Blocked on Jeff

### 4. Off-Machine Backups (blocked — needs destination decision)

Current backups run daily at 2am, 7 daily + 4 weekly rotation, all stored in `data/backups/pods/` on the same machine. A disk failure loses everything including backups.

**When Jeff decides**, the build is straightforward — add a post-backup step to `backup-pods.sh`:

```bash
# After tar.gz is created and verified:
# Option A: rsync to NAS/external drive
rsync -az "$BACKUP_FILE" user@nas:/backups/gathering/

# Option B: rclone to cloud (S3, Backblaze B2, etc.)
rclone copy "$BACKUP_FILE" remote:gathering-backups/

# Option C: scp to a VPS
scp "$BACKUP_FILE" user@vps:/backups/gathering/
```

~15 min of build time once the destination is known.

### 5. Automated Rollback (blocked — needs deployment strategy decision)

Current deploy is `terraform apply` for container infra + manual rsync for app code. No version tracking, no previous-release storage.

Options range from simple to sophisticated:
- **Simple**: Tag Docker images on deploy, keep N previous. Rollback = `docker run previous-tag`.
- **Medium**: Blue-green with Terraform workspaces. Deploy to blue, test, swap traffic.
- **Full**: Container registry + deployment pipeline with automatic health-check rollback.

This is an architectural decision (ADR territory) more than a quick build task. Recommend deferring until after the current sprint — the health checks + restart policies cover crash recovery, and manual redeploy is fine at current scale.

---

## Sequencing

```
1. Fuseki in CI        (~30-45 min)  ← unblocks #2
2. SHACL in CI         (~15 min)     ← depends on #1
3. Alert routing       (~30-45 min)  ← independent
4. Off-machine backups (~15 min)     ← blocked on Jeff
5. Automated rollback  (defer)       ← needs ADR
```

Items 1-2 are sequential. Item 3 is independent — can run in parallel with 1-2. Total buildable work: ~1-1.5 hours.

— Silas
