# Fuseki 401 Sync Storm — Resolution Steps

**From**: Silas | **Date**: 2026-02-23 00:18 | **Priority**: P1 — active error storm

## What I'm Seeing

477 sync failures in the last 10 minutes. All `401 Unauthorized` on graph loads. fullSyncAll is hammering every music file (~5,800) against Fuseki and getting rejected on every one.

Pattern from Loki:
```
Failed to sync resource | podId: jeff | resourcePath: music/albums/... | error: Graph load failed: 401 Unauthorized
```

## Root Cause (same as #137)

Fuseki container was recreated during your deploy. The `pods` dataset doesn't survive container recreation unless `FUSEKI_DATASET_1=pods` is set in the config — that env var tells Fuseki to auto-create the dataset on boot.

## Diagnostic Steps

```bash
# 1. Check if dataset exists
curl -s http://localhost:3030/$/datasets | python3 -c "import json,sys; print(json.dumps(json.load(sys.stdin),indent=2))"

# 2. Quick test — does the SPARQL endpoint respond?
curl -s -o /dev/null -w "%{http_code}" http://localhost:3030/pods/sparql
# Should be 200. If 401 or 404, dataset is missing.
```

## Fix

If dataset is missing:
```bash
# Create the dataset (use Fuseki admin credentials from terraform.tfvars)
curl -X POST 'http://localhost:3030/$/datasets' \
  -u 'admin:PASSWORD' \
  -d 'dbName=pods&dbType=tdb2'

# Then restart the app so fullSyncAll retries cleanly
../jeff-bridwell-personal-site/app-state.sh restart
```

If dataset exists but still 401 — the Fuseki admin password in the app's env may not match. Check `FUSEKI_ADMIN_PASSWORD` in the Terraform config matches what Fuseki booted with.

## Prevention

Make sure `FUSEKI_DATASET_1=pods` is in whatever config manages the Fuseki container (Terraform or docker-compose). This was the #137 fix — it auto-creates the dataset on every container start.

## Impact

While the storm runs, the app is functional (health check passes) but the knowledge graph is empty — no music, photos, or notes data until sync completes successfully.
