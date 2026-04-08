# Brief: Foundation Before Features — Priority Shift

**From**: Silas (Architect)
**To**: Wren (PM)
**Date**: 2026-02-15
**Re**: Jeff's architectural direction on foundation priorities

---

## What Happened

Jeff made an explicit call this morning: **security, data integrity, engineering pipelines, and observability must be roughly healthy before new feature work ships.** He's willing to wait on features (SMS v2, music harvester, etc.) to get the foundation sound.

This is now captured as Design Principle #7 in the guardrails document and informs all near-term sequencing.

---

## What This Means for the Backlog

### Blocked until foundation sprint completes:
- SMS Capture v2 (photo/MMS, link titles, CaptureAdapter, multi-sender)
- Music ontology / harvester prep
- vis.js data graph (ADR-004 Layer 3)
- Any new collection or domain work

### Foundation sprint (Kade, ~4.5-5 hours):

| Phase | What | Why |
|-------|------|-----|
| CI pipeline | Fuseki in CI + SHACL validation gate | Ontology is architecture — must be tested in pipeline |
| Observability | Activate probes, connect containers, Grafana panels | New infra (WebVOWL, Vikunja, capture) has no monitoring |
| Alert routing | Alertmanager → Slack | Alerts fire into void today — nobody gets notified |
| API documentation | Swagger/OpenAPI at `/api-docs` (admin-only) | ~70 endpoints with no browsable reference, no performance visibility |
| Endpoint metrics | Per-route RED metrics in Grafana | Can't optimize or debug what you can't see |

### After foundation sprint:
Feature work resumes with a sound base. Every new feature that ships will include observability as a delivery requirement (ADR-005).

---

## Decisions Reflected

| Decision | Status | Reference |
|----------|--------|-----------|
| Foundation before features | **Jeff decided** | Design Principle #7, guardrails doc |
| Observability evolves with infra | **Accepted** | ADR-005 |
| Swagger for API docs | **Jeff approved** | swagger-autogen + swagger-ui-express |
| API docs behind admin auth | **Silas recommendation** | Security: endpoint structure is not public |

---

## Open Items (Still Need Jeff)

- **Off-machine backups**: Destination decision still pending (NAS, cloud, VPS). This is the highest-risk gap in the data pillar. ~15 min build once decided.
- **Automated rollback**: Deferred — health checks + restart policies adequate at current scale.

---

## What I Need From You

1. **Acknowledge the priority shift** in your backlog / board — feature items should move behind the foundation sprint
2. **SMS v2 brief is paused** — Kade should not start v2 until foundation sprint completes
3. **Any new feature briefs** should include a "Foundation Check" section confirming the four pillars support the feature

— Silas
