# Brief: Doc Audit Complete — Silas Sections

**From:** Silas (Architect)
**To:** Wren (PM)
**Date:** 2026-03-03
**Re:** 2026-03-03-doc-audit-silas.md

## Results

### Updated (5 docs — significant drift)

| Doc | Was | Now | What Changed |
|-----|-----|-----|-------------|
| **SOLID-AUTHENTICATION.md** | Feb 21 | Mar 3 | Full rewrite. Old doc described generic Inrupt OIDC flow that no longer exists. Now documents CSS client credentials grant, Pivot fallback, service tokens, Docker networking. 470→120 lines. |
| **INFRASTRUCTURE.md** | Feb 21 | Mar 3 | CSS + Navidrome in service table, deploy pipeline (121s→24s), Promtail daemon scrape, alerting (34 rules + Alertmanager), dashboard count (8→11), C2 threshold (85→95%), container count (16→18), auth section updated. |
| **SYSTEM_ARCHITECTURE.md** | Feb 20 | Mar 3 | CSS + Navidrome in Mermaid diagram, auth boundary rewritten (CSS primary, Pivot fallback), tech stack table (ontology v1.1.0, 2GB Fuseki, Mistral, docker-compose), ADR-015/016 added, container/disk budgets updated. |
| **ONTOLOGY_STATUS.md** | Feb 22 | Mar 3 | Version bump v0.9.0→v1.1.0. Stories, Values, Practices, People domains added to domain table. |
| **infrastructure-constraints.md** | Feb 27 | Mar 3 | Container count 15→18, service table updated (CSS, Navidrome, Alertmanager, Fuseki heap). |

### Stable (no content drift)

| Doc | Last Updated | Why Stable |
|-----|-------------|-----------|
| C4-ARCHITECTURE.md | Feb 28 | Recently updated, structural model hasn't changed |
| CONCEPTUAL_MODEL.md | Feb 14 | Theoretical reference — not affected by infra changes |
| SOLID_PODS_RESEARCH.md | Feb 17 | Research reference — static |
| UNIFIED_INDEX_ARCHITECTURE.md | Feb 7 | Design reference — Fuseki indexing approach unchanged |
| CONTENT_INGESTION_MATRIX.md | Feb 16 | Ingestion tiers unchanged. May need Stories domain row eventually. |
| GLOSSARY.md | Feb 14 | Could use new terms (CSS, client credentials) but not blocking |
| PUBLIC_ACCESS.md | Feb 21 | Access model hasn't changed |
| log-topology.html | Mar 3 | Fresh — Wren built today |
| log-relatedness.html | Mar 3 | Fresh — Wren built today |

### Minor Drift (noted, not blocking)

- **ARCHITECTURE_DECISIONS.md** (Feb 10): Timeline format, missing recent decisions. Low priority — ADR/ directory is canonical.
- **infrastructure-constraints.md**: LaunchAgent types say KeepAlive for pollers that are actually StartInterval. Minor.
- **GLOSSARY.md**: Missing CSS, client credentials, extra_hosts terms. Polish work.

### ADRs (001–016)

All 16 ADRs checked. Content is stable — ADRs don't drift by design (they record point-in-time decisions). ADR-015 and ADR-016 were missing from SYSTEM_ARCHITECTURE.md's ADR table — now added.

## Data for #763

For the doc-drift gate design, here's what this audit proved:

- **5 of 15 docs had significant drift** (33% stale rate)
- **Strongest correlation**: docs that describe implementation details (auth flow, service inventory, tech stack) drift fastest
- **Most resilient**: theoretical/reference docs, ADRs, research docs
- **Trigger patterns**: new container added, auth flow changed, deploy pipeline changed, ontology version bumped
- **Detection heuristic**: if git commits touched `docker-compose.yml`, `src/handlers/login*`, `data/ontology/`, or LaunchAgent plists → check INFRASTRUCTURE.md, SOLID-AUTHENTICATION.md, SYSTEM_ARCHITECTURE.md, ONTOLOGY_STATUS.md

This gives you real data for the doc manifest glob patterns.
