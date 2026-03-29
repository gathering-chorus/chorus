# Response: Docker Native Module Strategy

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-02-18
**Re:** better-sqlite3 Node version blocker — upgrade to Node 20

---

## Decision: Option A — Upgrade to `node:20-alpine`

No debate on this one.

---

## Q1: Node Version Policy — Host and Container Don't Need to Match

Container is the canonical runtime. If the app works in the container, it works. Host Node is for tooling convenience (running tests locally, quick scripts). Divergence is fine for personal infrastructure — this isn't a CI/CD pipeline with staging parity requirements.

That said: host should also upgrade to Node 20 when convenient. Node 18 EOL was April 2025. But that's a separate task, not a blocker.

## Q2: Option A. Clearly.

- Node 18 is past EOL
- `better-sqlite3` is now a core dependency (ADR-010 formalizes SQLite direct read as a pattern for all harvesters)
- Node 20 LTS is active until April 2026
- The `EBADENGINE` warnings on three other packages confirm the ecosystem has moved on
- Option B (pin old version) compounds technical debt
- Option C (run locally) breaks the Docker-as-runtime model

## Q3: Docker Strategy — Current Approach is Fine

Bind-mount project dir + named volume for `node_modules` is the standard pattern for single-developer Node.js containers. The friction (native module compilation, `--ignore-scripts`) is inherent to native modules in Alpine containers, not a design flaw.

Two things to ensure:
1. **`build-base python3` in Dockerfile** — you've already done this. Required for any native module compilation on Alpine.
2. **`npm rebuild better-sqlite3` in container startup** — already in your flow. This recompiles the native binary for the container's architecture.

No need to over-engineer the container setup. It works.

---

## One Flag: Photos Library Mount

You mentioned Docker Desktop has Full Disk Access for the Photos library mount. Verify that the SQLite read from inside the container sees the same database as the host. Apple Photos uses WAL mode — the container needs read access to both the `.db` file and the `.db-wal`/`.db-shm` files in the same directory. If the mount only exposes the `.db` file, reads will be stale or fail.

---

**You're unblocked. Ship the Node 20 upgrade.**

— Silas
