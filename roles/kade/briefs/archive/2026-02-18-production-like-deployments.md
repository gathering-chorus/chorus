# Brief: Production-Like Deployment Pattern

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-02-18
**Re:** ADR-011 — Jeff wants atomic, verified, reversible deploys
**Priority:** P2 (do after current Photos CQRS work ships)

---

## Jeff's Directive

"Treat deployments like a production environment. A deploy should work all the way or not at all."

Full ADR: `architect/adr/ADR-011-production-like-deployments.md`

---

## What You Need to Build (4 Phases)

### Phase 1: Health Checks (first)

1. **Add `/health` endpoint to Express app.** Return:
   ```json
   {"status":"ok","version":"abc1234","uptime":45,"checks":{"fuseki":"ok","pods":"ok","disk":"ok"}}
   ```
   Check Fuseki reachability, pod directory exists, disk usage below 90%.

2. **Add HEALTHCHECK to the root Dockerfile** (the Node 20 one):
   ```dockerfile
   HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
     CMD wget -qO- http://localhost:3000/health || exit 1
   ```

3. **Delete `terraform/Dockerfile`** — it's Node 18 and conflicts with the root Dockerfile. One Dockerfile per project.

4. **Add health checks to Terraform** for MySQL and Fuseki containers.

### Phase 2: Deploy Pipeline

1. **Rewrite `app-state.sh`** with a new `deploy` command:
   - `deploy`: build → smoke test → terraform apply → health gate → verify → tag
   - `start`: terraform apply with existing image (no build)
   - `stop`: terraform destroy containers
   - `rollback`: swap to previous tagged image, terraform apply
   - `status`: check container health + show versions

2. **Multi-stage Dockerfile**:
   - Stage 1 (deps): `npm ci`, compile native modules
   - Stage 2 (build): copy source, `npm run build:all`
   - Stage 3 (production): copy only dist + node_modules + static assets, non-root user

3. **Image tagging**: `{app}:{git-sha-short}` on build, rotate `latest` and `previous` tags.

### Phase 3: All Projects

Apply the same pattern to WordPress and observability. WordPress is simpler (no custom build), but gets health checks and the deploy/rollback flow.

### Phase 4: Discipline

No more `docker exec` to fix things. If it's broken, fix the code and `deploy`.

---

## Key Decisions Already Made

- **One Dockerfile per project** (root level, not terraform/)
- **`npm ci` not `npm install`** (deterministic, respects lockfile)
- **No source code bind-mounts** (code goes into image at build time)
- **Apple Photos SQLite bind-mount is OK** (read-only, system database, exception documented)
- **Named volumes for persistent data** (MySQL, Fuseki, pods survive deploys)
- **Non-root user in production image** (`USER node`)
- **`wget` for health checks in Alpine** (smaller than curl)

---

## Don't Start Until

- Photos CQRS browse is shipped and working in Docker
- Node 20 upgrade is deployed and stable

This is the next infrastructure improvement after the current feature work lands.

---

— Silas
