# Brief: Boot-Order Orchestration Shipped (#382)

**From**: Silas (Architect) → Wren (PM)
**Date**: 2026-02-25
**Card**: #382

## Summary

This morning's reboot exposed that our "data center" (Jeff's term — and accurate) had no automated recovery. 15 containers across 4 compose stacks + 2 bare Node processes on Bedroom, all requiring manual intervention. Jeff and I built and demoed boot-order orchestration for both Macs.

## What Shipped

| Piece | Location |
|-------|----------|
| `docker-startup.sh` | `shared-observability/scripts/` — 5-stage boot with health gates |
| `app-state.sh boot` | New command — full data center boot |
| Docker Desktop AutoStart | Enabled (was `False`) |
| LaunchAgent (Library) | `com.chorus.docker-services` — triggers boot script on login |
| LaunchAgents (Bedroom) | `com.gathering.images-api-server` + `com.gathering.images-api-video` — KeepAlive |

## Boot Stages

1. Wait for Docker daemon
2. Observability stack → health gate (Loki + Prometheus)
3. Infrastructure (Vikunja + WordPress, parallel)
4. Application (Fuseki healthy → app)
5. Host port validation — Library (6 ports) + Bedroom (2 ports)

Cold start to full stack: ~66 seconds. Demoed with Jeff — stopped everything, ran `app-state.sh boot`, all services up and validated including Bedroom.

## Card Status

#382 is demoed. Partial demo done (stop all → boot → validate). Full demo (actual reboot) deferred to next natural reboot. Ready for proving gate when you're ready.

## Decision Context

Jeff framed the two Macs as data centers (DEC-054 names them Library and Bedroom). This work treats them that way — boot orchestration, cross-machine health validation, single pane of glass from Library.
