# ADR-015 Implementation: wordpress-blog → docker-compose

**From**: Silas (Architect)
**Date**: 2026-02-21
**Priority**: P1 — Jeff flagged as blocking flow
**ADR**: ADR-015 (Container Orchestration for Single-Host Services)

## What to build

Migrate wordpress-blog from Terraform + bind-mount to docker-compose + immutable images.

## Steps

1. **Write a Dockerfile** for wordpress-blog that bakes the theme into the image at build time (COPY, not mount)
2. **Write docker-compose.yml** — WordPress container + MySQL, health checks, restart policies, named volumes for data only (not code)
3. **Update app-state.sh** (if wordpress-blog has one) to use `docker compose` commands instead of `terraform apply`
4. **Remove Terraform config** — `main.tf`, `variables.tf`, `terraform.tfvars`, `.terraform/`, `terraform.tfstate*`
5. **Test**: build image, start stack, verify theme renders, verify health checks pass
6. **Document**: update any README or CLAUDE.md references to Terraform

## Constraints

- MySQL data volume must survive container recreation (named volume, not bind-mount)
- WordPress uploads directory must persist (named volume)
- Health check pattern: match shared-observability (wget-based, interval/timeout/retries)
- Port binding: `127.0.0.1:PORT:PORT` (ADR-012 compliance)

## Pattern reference

Look at `shared-observability/docker-compose.yml` — that's the target pattern. Health checks, restart policies, named volumes for data, `127.0.0.1` binds.

## Out of scope (for now)

- jeff-bridwell-personal-site migration (same direction, but separate card when ready)
- CI/CD changes (existing pipeline should work with `docker compose build`)

— Silas
