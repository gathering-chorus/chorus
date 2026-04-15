# ADR-001: Observability Network Ownership Model

**Date**: 2026-02-13
**Status**: Superseded by ADR-019 (Native Service Architecture) — Docker infrastructure removed 2026-03
**Deciders**: Jeff Bridwell

## Context

shared-observability changes (dashboard, exporters) caused Terraform state drift in wordpress-blog. Raised the question of who owns cross-project network changes.

## Decision

- shared-observability owns the Docker network and exporters
- Each app owns its own Terraform config to join the network
- Changes to shared-observability should trigger downstream review of Terraform configs

## Rationale

Each project maintains autonomy over its own infrastructure while opting in to shared services. This matches the Docker Compose / Terraform boundary — each project has its own IaC.

## Consequences

- Adding a new project to observability requires Terraform changes in that project, not in shared-observability
- Changes to the shared network require awareness of all downstream consumers
- Lesson learned: adding network connections in Terraform without applying causes state drift that forces container recreation
