# Brief: Docker Native Module Strategy — better-sqlite3 in Container

**From**: Kade (Engineer)
**To**: Silas (Architect)
**Date**: 2026-02-18
**Priority**: P1 (blocks Photos CQRS browse)

## Context

The Photos Browse CQRS implementation is done at the application layer (handler reads SQLite directly, 24 tests passing). But it doesn't work in Docker because `better-sqlite3` native binary won't compile inside the container.

## Root Cause Chain

1. **Container image**: `node:18-alpine` (Node 18.20.8)
2. **Package**: `better-sqlite3@12.6.2` requires Node 20+ (`"node": "20.x || 22.x || 23.x || 24.x || 25.x"`)
3. **Container startup**: `npm install --ignore-scripts && npm rebuild better-sqlite3` — install works, rebuild produces intermediate objects but no final `.node` binary
4. **Result**: `require('better-sqlite3')` fails with "Could not locate the bindings file" → `extractPhotos()` returns empty → browse page shows "No photos in the collection yet"

The app worked locally before Docker because macOS host has the native module pre-compiled (it was installed with the correct Node version on the host).

## Additional Issues Found and Fixed

- **Dual-start bug in `app-state.sh`**: Was starting both Docker container AND local Node.js on port 3000. Fixed — Docker is now the only app runtime.
- **Dockerfile**: Added `build-base python3` for native module compilation, `--ignore-scripts` to skip `prepare` hook.
- **Terraform**: Changed from `node:18-alpine` image pull to `build {}` block (matches Fuseki pattern). Added named volume for `node_modules`, Photos library mount, env vars.
- **macOS TCC**: Docker Desktop now has Full Disk Access for Photos library mount.

## Options

### A. Upgrade Docker image to Node 20+ (Recommended)
- Change `FROM node:18-alpine` → `FROM node:20-alpine`
- Matches `better-sqlite3@12.6.2` engine requirement
- Also fixes 3 other `EBADENGINE` warnings (marked, node-sarif-builder, walk-up-path)
- **Risk**: Node 20 may introduce breaking changes. Need to verify tests pass.
- **Question for you**: Does this align with any Node version policy? The host runs Node 18 — should host and container match?

### B. Pin older better-sqlite3 compatible with Node 18
- Downgrade `better-sqlite3` from 12.6.2 to ~11.x (last Node 18 compatible release)
- Keeps Node 18 everywhere
- **Risk**: Older version may have bugs, security issues, missing features

### C. Run app locally (not in Docker) for dev
- Revert to local Node.js for development, Docker only for Fuseki/WebVOWL/observability
- This is how it worked before today's session
- **Risk**: Divergence between dev and prod. But there is no "prod" — this is Jeff's personal infrastructure.

## My Take

Option A is cleanest. Node 20 is LTS and widely stable. The `EBADENGINE` warnings tell us the ecosystem is already past Node 18. But I want your input on whether host and container should match versions, and whether a Node upgrade has broader implications for the architecture.

## Questions for Silas

1. **Node version policy**: Should host and container run the same Node version? Or is divergence acceptable for a dev environment?
2. **Option preference**: A, B, or C? Or something I haven't considered?
3. **Broader Docker strategy**: The container bind-mounts the host project dir + uses a named volume for `node_modules`. This works but has friction (native module compilation, `--ignore-scripts` workaround). Is there a better container architecture for this project?
