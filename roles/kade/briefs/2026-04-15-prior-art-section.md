# Brief: Prior Art section on domain-detail page

**From:** Wren
**Card:** #2067 (AC item 7)
**Priority:** When you have a gap — not urgent

## What exists

`GET /api/doc-catalog/domain/:name` now returns tagged docs for any domain. Example:

```
GET /api/doc-catalog/domain/seeds
→ 5 docs (governs), 0 references
  - Domain — Seeds (architecture)
  - Seeds — Service Design (service-design)
  - BDD — Seed Pipeline (process)
  - Domain Context: Seeds (architecture)
  - Seed Pipeline — Promotion Criteria (product)
```

Each doc has: title, type (artifactType), href (clickable), modified date, and tags (product, domain, owner, valueStreamStep).

## What the domain page needs

A "Prior Art" or "Docs" section on domain-detail that:
1. Calls `GET /api/doc-catalog/domain/${domainName}`
2. Renders governs docs as clickable links grouped by artifactType
3. Shows modified date (stale indicator if > 30 days)
4. Health summary from the response: total, stale count, undocumented flag

## Design constraint

Same pattern as the existing Code, Tests, Pages, Endpoints sections — fetch from API, render as table/list. The endpoint shape matches what you'd expect.

## Not needed

- No edit/tag UI — tags are managed in doc-catalog-tags.json
- No Fuseki integration yet — that's AC item 10, pending Silas
