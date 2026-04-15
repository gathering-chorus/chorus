# Kade — Next Session

## This session (2026-04-15 14:28 – 18:30)

8 cards shipped. Domain-detail page transformed from skeleton to full operating surface. Crash recovery was "dramatically better" — Chorus index reconstructed 5.5 hours in 3 minutes.

**Shipped:**
- #2060 — Domain API consolidation (5 facet endpoints, AX=UX)
- #2069 — Value stream pipeline view (5 stages from existing data)
- #2078 — Docs proxy into Prior Art section
- #2080 — Borg infra on domain pages (domain-scoped via usesEnvironment)
- #2082 — Dependencies facet (direct + shared infrastructure)
- #1910 — Release history (273 chorus releases, git-first)
- #2028 — Domain radius + blast radius + decisions rendering
- #2054/#2070 — Crash recovery ACPs

**Also:** Paired with Silas on #2060, reviewed client onboarding design brief, gated 7 Silas + 1 Wren cards, fixed alerts rendering, merged Docs into Prior Art, wired decisions section

## Pick up
1. **#2068** — Demo skill blocks if AC unchecked (coordination tooling, original sequence)
2. **Persistence section** — render borg:Resource instances per domain (Wren priority)
3. **Gate pass rate metric** — pipeline Prove stage enrichment (from Wren chat on LinkedIn seeds)
4. **Section reorder** — follow value stream per Wren's direction
5. **domain-detail.js refactor** — function signature has 22 params, needs object

## Domain-detail page sections (current)
Pipeline, Release History, Infrastructure, Dependencies, Prior Art, Decisions, Code, Tests, Alerts, Services, Logs, UI Pages, API Contract, Actors, Scenarios, Cards, Completeness, Persistence, Integration, Blast Radius, Gaps

## Key context
- Borg service design read and loaded — 7 domains, herald pattern, Engine→Environment→Resource
- Client onboarding design reviewed — builder feedback posted to cross-role chat
- Crash recovery pattern saved to memory — use Chorus index, roles coordinate directly
- Silas's graph hygiene ADR-022 includes two patterns from this session
