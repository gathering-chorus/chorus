# Kade — Next Session

## Status
Big session. 6 cards shipped, Athena CMDB API built from scratch, AX doc written, Fuseki data corrected.

## This session (2026-04-10)
- **#1800 accepted** — gated integration tests behind RUN_INTEGRATION=true, eliminated SQLite contention
- **#1846 3/4** — rebuilt Gathering, fixed /interaction-patterns 404, 4701 tests green. AC4 (smoke check) waiting Silas
- **#1848** — AX doc at designing/docs/agent-experience.md. Silas reviewed, agreed 6/7 friction items are platform bugs
- **#1849 accepted** — Athena CMDB API: 9 endpoints (health, products, subproducts, subdomains, blast-radius, steps, owners, machines, subdomain detail). Named .sparql files, CORS, _meta envelope, agent-friendly 404
- **#1860 accepted** — 22 integration tests for Athena, data-driven from Fuseki counts, RUN_INTEGRATION gated
- **#1858 demoed** — 4 Athena UI pages wired to live API, CSP connect-src fix, legacy doc links
- **#1863 accepted** — 9 Gathering domains added to Fuseki, deprecation banners on ontology views, hasDomain composition
- Fixed Fuseki port 3031→3030 in 3 files
- Fixed NiFi JDBC path to Photos.sqlite (Silas applied via API)
- Ontology corrections: Pulse/Messages/Streams to Directing, Clearing added as SubDomain, ToolsChain to Silas

## Pick up
- **#1858** — waiting accept (demoed, Wren + Silas gave feedback)
- **#1846 AC4** — post-deploy smoke check, Silas domain
- **#1860 test counts** — need updating after Gathering domains added (Kade 13 not 14, etc.)
- **NiFi ExecuteSQL** — JDBC path fixed, processor stopped, verify query before restart
- **Tier 2 harvest** — scoped in /tmp/pair-1863.md, cheerio parser for 169 component rows from service design HTML

## Key conversations
- Jeff: AX is developer experience for agents. 5/31 services have clean agent paths. The practice is noticing friction and refusing to work around it.
- Jeff: "execution management theater" — demo skill says hard gate but nothing enforces it
- Jeff: don't fix old then rework — build it right once in the new form
- Silas: 6/7 AX friction items are platform bugs. Priority: blast radius gate, services.json, git-queue, grep hook
- Wren: 5/31 is the AX baseline. CMDB should track AX status per service.

## Notes
- Fuseki is port 3030. Not 3031. Memory saved. Three prior "fix" attempts failed.
- `open <url>` for Jeff's browser, not chrome-window.sh
- Chorus API logs go to /tmp/chorus-api.log, not Loki
- dist/ is gitignored in Gathering — deploy rebuilds, don't try to commit dist
