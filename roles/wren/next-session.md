# Wren — Next Session

## What Happened (April 10, 2026)

### Shipped
- **#1843** — Fix stale product-manager brief path. Accepted.
- **#1845** — Chorus canonical model in OWL/RDF. chorus.ttl v0.3.0, 40 SubDomains, dynamic instance explorer backed by Fuseki. Accepted.
- **#1849** — Athena CMDB API. 8 named query endpoints at localhost:3340/api/athena/*. Health, products, subproducts, subdomains (with filters), blast-radius, steps, owners, machines.
- **#1858** — Wired Athena UI pages to live API. CSP connect-src fix was the blocker. 4 pages live from Fuseki.
- **#1860** — Athena API endpoint tests. 21 data-driven tests, all filter counts verified against Fuseki.

### In Progress
- **#1863** — Retire domain HTML docs into Athena. Tier 1 done (9 EJS views deprecated with banners). 9 Gathering domains added to Fuseki. Tier 2 (metadata harvest from service design docs) scoped but not started.
- **#1800** — Kade's test isolation. Demo received, not yet accepted by Jeff.

### Cards Created
- #1845, #1849, #1850, #1851, #1858, #1859, #1860, #1862, #1863, #1864

### Key Decisions
- **Product boundary**: Gathering domains belong to Gathering's value stream (Personal/Life), not Chorus spine steps. Use `builtBy` edge, not `primaryStep` in Chorus.
- **Two spirals**: February drawing confirmed — products have separate value streams. Athena shows all 3 streams, product filter narrows.
- **Athena pattern**: Named query API over Fuseki graph, from Jeff's patent (9,552,400 B2). No ad-hoc SPARQL.
- **Domain page as operating surface**: Value stream → step → domain entry. Domain page shows everything: sub-domains, services, code, tests, alerts, monitors, logs, actors, BDD, cards, integrations, blast radius.
- **CMDB discipline**: Picture → ownership → move, one at a time. Named queries, not ad-hoc.
- **AX gap**: 5/31 services have clean agent APIs (per Kade).

### Jeff Insights
- Came in excited to share model work (spreadsheet, garden sketch, draw.io). Session rushed past his sharing.
- "The process of understanding IS the work to get there"
- Domains are atomic services — the spreadsheet was a service inventory, not a taxonomy
- Sub-products compose from services via consumes edges
- The domain page should gate the process end to end — can't work in an incomplete domain
- "Model first, not ready aim fire — we've been fire fire fire aim"
- Pulse is a domain that composes Messages, Streams, Cards, Alerts
- Clearing is a domain (composition), same pattern as Chorus
- ToolsChain is Silas's (infra tooling), not Kade's

### Feedback Recorded
- No AC negotiation — finish the AC or don't demo
- Have a position — don't agree with contradictory statements
- Open in Jeff's browser — use `open <url>`, not chrome-window.sh
- Don't abandon pair partners
- Don't make up what a role is doing during gemba — look at the screen

## Critical Pickup
1. **#1863 Tier 2** — harvest 169 component rows from 6 service design HTML docs into Athena
2. **#1864** — Athena multi-product entry page with 3 value streams and builtBy edges
3. **Vikunja token** — expired, Jeff needs to regenerate. Silas added 401 alerting (#1856)
4. **Move Gathering domains** out of Chorus steps into Gathering value stream steps
5. Accept #1845, #1849, #1858, #1860 when board is back (Vikunja token)

## Session Start
**Start from `/Users/jeffbridwell/CascadeProjects/chorus/roles/wren`**
