# Brief: Cross-Machine Dependency Visibility

**From**: Wren (PM) → Silas (Architect)
**Date**: 2026-02-25
**Context**: Jeff battled ops issues all morning — Bedroom dependencies hit Kade's work empirically. ADR-012 + #382 boot orchestration exist but roles still can't see cross-machine dependencies at a glance.

## Problem

Every feature touching photos or media has a hidden dependency on Bedroom being healthy. Roles discover this mid-work, not at boot. That's failure demand — 30 minutes debugging before actual work starts.

You've built the pieces: ADR-012 (permissions + registry), docker-startup.sh (boot stages + Bedroom port validation), LaunchAgents on both machines. What's missing is the **visibility layer** that makes this knowledge available to all roles automatically.

## What I Think We Need

A **service dependency manifest** — structured data (JSON or YAML) that answers:
- What services run on each machine?
- What depends on what across machines?
- If Bedroom is unreachable, which cards/features are blocked?
- What's the health check for each service?

And wire it into `werk-init.sh` so every role sees machine state at boot: "Bedroom: healthy" or "Bedroom: images-api unreachable — gallery/media cards blocked."

## What Already Exists (from my review)

| Artifact | What it covers | Gap |
|----------|---------------|-----|
| ADR-012 | Permissions, service registry, machine names | No dependency graph |
| docker-startup.sh | Boot stages, Bedroom port validation | Validates but doesn't surface to roles |
| spike-382 brief | ASCII dependency graph | Embedded in prose, not queryable |
| system-architecture.md | High-level mermaid diagram | Doesn't show cross-machine edges |
| CLAUDE.md service registries | LaunchAgent labels per machine | Static, no health or dependency info |

## Questions for You

1. Is `services.json` (from spike #254) still the right vehicle? Or does this belong in the boot script output?
2. Should `werk-init.sh` do a live Bedroom health check at role boot? (adds latency but surfaces problems early)
3. Is there a structured way to say "these cards require Bedroom" so the board can show blocked state automatically?

## Sizing

I think this is small-medium. The data exists — it's a marshaling and wiring problem, not a discovery problem.
