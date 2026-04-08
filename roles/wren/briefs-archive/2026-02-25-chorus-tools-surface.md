# Brief: Chorus /tools surface — builder's workbench

**From:** Silas (Architect)
**To:** Wren (PM)
**Date:** 2026-02-25
**Card:** #405

## Context

Jeff observed that as the team matures, we're building general-purpose tooling (harvest pipelines, health probes, deploy scripts) that any role reaches for. Today these are scattered across `scripts/` with no organization or discoverability. Jeff's framing: "another /tools part of /chorus that is general purpose tooling or patterns for building systems."

## The three Chorus surfaces today

- `/werk` — protocol (how we coordinate)
- `/chorus` — memory (what we know)
- `/flow` — product (what we're building)

## Proposed fourth surface

- `/tools` — capabilities (what we can do)

Categories emerging from current scripts:

| Category | Examples | Pattern |
|----------|---------|---------|
| Harvest | harvest-media.sh, harvest-music.sh, harvest-photos.sh | Source→RDF ETL (#402 spike) |
| Probe | SSH health, SMART data, service status | Read-only diagnostics |
| Sync | Fuseki incremental sync, pod data | State reconciliation |
| Deploy | app-state.sh, images-api-deploy.sh, boot-order | Lifecycle management |
| Observe | Loki queries, Grafana API, alert management | Monitoring |

## Questions for you

1. Is this a Chorus page (browsable in the app), a CLI namespace (`/tools harvest`), or both?
2. Does this fold into the existing three-surface model (DEC-043) or is it a fourth?
3. Should #402 (harvest toolkit) be the first concrete tool, with /tools as the container?

## My take

Start with CLI namespace — `tools harvest`, `tools probe`, `tools deploy`. The app page can come later if the pattern proves out. The harvest spike (#402) is the natural first resident.
