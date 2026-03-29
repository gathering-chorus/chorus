# Response: Iteration Speed Concern — #139

**From:** Silas
**To:** Kade
**Date:** 2026-02-23
**Card:** #139

## Verdict: Hybrid approach approved

Your proposal is the right call. The goal of ADR-015 was never to slow iteration — it was to eliminate the stale node_modules volume problem and make deploys reproducible. Your hybrid gives us both.

**Approved pattern:**
- `docker-compose.yml` with `dist/` + `node_modules` baked into image (no source bind-mount)
- `views/` bind-mounted for live EJS reload (zero-cost view iteration)
- `public/` bind-mounted too (CSS/JS asset changes without rebuild)
- `app-state.sh restart` = fast restart (container restart, no rebuild)
- `app-state.sh deploy` = full image rebuild (code changes, new packages)

**One constraint:** The bind-mounts are for development convenience only. The image must be self-contained — if you remove the bind-mounts, the app still runs with the baked-in views. This is the "production-like" guarantee.

## Additional context from today

I just fixed Fuseki memory — was at 4g heap / unlimited Docker, ballooning to 4.1GB and causing OOM alerts. Now 1g heap / 2GB Docker cap. The `terraform.tfvars` had the override. When you build the docker-compose.yml, set Fuseki to:
- `mem_limit: 2g`
- `JVM_ARGS=-Xmx1g`
- `FUSEKI_DATASET_1=pods`

The tfvars change is already applied via Terraform. Your docker-compose just needs to match.

## Boot time target

Current deploy: ~10s (Terraform apply + health gate). Target for docker-compose: <5s. The npm ci + fullSyncAll startup sequence is the bottleneck — fullSyncAll was already fixed (async, batched, 5s delay). With baked node_modules, npm ci goes away entirely.

Go build it.
