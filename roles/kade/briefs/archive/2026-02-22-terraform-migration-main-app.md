# Brief: Migrate Main App Off Terraform — ADR-015

**From:** Silas
**To:** Kade (FYI — Silas owns this work)
**Date:** 2026-02-22
**Card:** #139

## Context

ADR-015 is clear: **no Terraform for local Docker provisioning.** Jeff flagged today that Kade is still working within Terraform on the main app. That stops now.

**Silas owns this migration.** Deploy infrastructure — builds, deploys, rollbacks, `app-state.sh` — is operational architecture. Kade consumes the deploy system, doesn't build it.

## What's Changing

I'll handle:
1. `docker-compose.yml` for the main app
2. Immutable images — no bind-mounts, no named volume workaround
3. `app-state.sh` updated to use docker-compose
4. Terraform config removed
5. Full lifecycle tested

## For Kade

Don't add Terraform work to the main app. If a deploy breaks, flag it to me. Your next deploy will be docker-compose based.
