# ADR-015: Container Orchestration for Single-Host Services

**Date:** 2026-02-21
**Status:** Accepted
**Context:** Docker/Terraform simplification brief from Kade (2026-02-21), Jeff directive

## Decision

For single-host services in the Gathering portfolio:

1. Use **docker-compose** for container orchestration
2. **No Terraform** for local Docker provisioning
3. **No bind-mounts** for application code — code baked into images at build time
4. IaC discipline preserved: YAML in git, validated in CI, reproducible deploys

## Context

The wordpress-blog project uses Terraform to provision Docker containers with bind-mounted theme directories. This creates:

- **Architectural inconsistency** — shared-observability already uses docker-compose successfully
- **Unnecessary complexity** — Terraform state files drift, lock files block operations, phantom plan changes
- **Deployment fragility** — bind-mounts create drift between git and runtime, require manual sync

Kade documented 5 bugs directly caused by this pattern (health check lies, rollback no-ops, no disk pre-flight, unnecessary npm ci, stale image accumulation).

## Rationale

### Terraform earns its seat when you need:
- Multi-cloud or multi-provider orchestration
- Complex dependency graphs across infrastructure types
- State management across teams or environments
- Infrastructure at scale

### We have:
- Single Mac mini, single Docker daemon, local services
- docker-compose delivers the same IaC guarantees (versioned, declarative, testable, reproducible)
- Terraform adds state management overhead without benefit at this scope

### Bind-mounts are a liability:
- macOS/Linux node_modules incompatibility forces named volume workaround
- Every container start runs npm ci to sync (10-20s wasted)
- Runtime state lives outside the container — breaks immutability
- Rollback is complicated by mounted vs baked code divergence

### Immutable images are the right pattern:
- Code baked at build time — what you test is what you deploy
- No sync, no drift, no volume conflicts
- Restart is <2s without npm ci
- Rollback = deploy previous image tag

## Trade-off

`.ejs` view changes require a container restart instead of a browser refresh. With npm ci eliminated, restart is <2s. Acceptable for the reliability gain.

## Applies to

| Project | Current | Target |
|---------|---------|--------|
| wordpress-blog | Terraform + bind-mount | docker-compose + immutable image |
| jeff-bridwell-personal-site | Terraform + bind-mount + named volume | docker-compose + immutable image (future) |
| shared-observability | docker-compose (already correct) | No change |

## Scope

If the portfolio ever needs multi-cloud provisioning or cross-provider orchestration, Terraform earns its seat back. This decision applies to **single-host Docker** only.

— Silas
