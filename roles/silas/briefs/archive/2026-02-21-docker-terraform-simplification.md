# Docker/Terraform Simplification

**From**: Kade (Engineer)
**To**: Silas (Architect)
**Date**: 2026-02-21
**Priority**: P1 — Jeff flagged this as blocking flow
**Related**: ADR-011

## Problem

The Docker/Terraform infrastructure layer is over-engineered for a single-machine setup and has been a source of recurring friction. Jeff sees it too — he flagged that I'm battling this layer frequently.

### What I just fixed (should have never been broken)

1. **Health check lied** — `wait_for_healthy()` returned success when the container was "Running" but `/health` never responded. A crash-looping app passed the deploy health gate.
2. **Rollback was a no-op** — `cmd_rollback()` re-tagged the image then called `docker start`, which just resumed the container with its original image. The rolled-back image was never used.
3. **No disk pre-flight** — the 2026-02-17 disk-full incident killed all 14 containers. No pre-check existed.
4. **npm ci ran on every restart** — 10-20s wasted because the bind-mount pattern requires syncing macOS→Linux node_modules via a named volume.
5. **12 stale image tags** — no cleanup, ~45GB of dead images accumulating.

These are now fixed. But the pattern keeps generating problems.

### Root cause: Terraform managing what docker-compose does simpler

We use Terraform to manage 3 containers on one Mac mini. The result:
- `terraform apply -auto-approve` to restart a container
- State files that drift and need `terraform init`
- Lock files to prevent concurrent applies
- `terraform plan` shows changes when none are intended
- Ghost variables still in `terraform.tfvars` throwing warnings
- Rollback required Terraform recreation (now fixed, but fragile)

### Root cause: bind-mount + named volume conflict

The host project dir is bind-mounted at `/app` for instant `.ejs` view changes. But macOS `node_modules` don't work in Linux, so a named volume masks them. Every container start needs `npm ci` to sync the volume. I added a hash guard (bandaid), but the architecture is fighting itself.

## What I'm asking for

An architectural decision on simplification. Two options I see:

### Option A: Replace Terraform with docker-compose

- `docker-compose.yml` already exists for the bridge. Extend it for the app stack.
- Same infra-as-code benefits, fraction of the ceremony.
- `docker compose up -d` replaces `terraform apply -auto-approve`.
- No state files, no init, no lock files.
- Health checks, restart policies, named volumes all supported natively.
- Rollback: `docker compose pull && docker compose up -d` with tagged images.

### Option B: Remove bind-mount, use image-only deploys

- Stop mounting the host directory. Container runs from the built image only.
- `node_modules` are baked into the image — no named volume sync, no npm ci on start.
- `.ejs` view changes require restart (or a lightweight file-sync tool).
- Cleaner separation: host is for development, container is for running.
- Works with either Terraform or docker-compose.

### Option C: Both

docker-compose + no bind-mount. Simplest possible setup. Deploy = build image + `docker compose up -d`. Restart = `docker compose restart`. View changes need restart but startup is <2s without npm ci.

## My recommendation

Option C. It eliminates both root causes. The bind-mount was a convenience that became a liability. Terraform was chosen when we had more containers and more complexity — we've simplified the stack since then (Ghost removed, Nginx removed).

The tradeoff: `.ejs` view changes need a restart instead of a browser refresh. With npm ci eliminated, restart would be <2s. That's acceptable.

## Jeff's input

Jeff directed this brief. His words: "let's make this better now — it blocks your flow frequently."

— Kade
