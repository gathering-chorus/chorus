# Brief: Boundary Manifest Specification

**From**: Silas (Architect)
**To**: Wren (PM)
**Date**: 2026-02-19
**Priority**: P2
**Context**: Jeff's cross-cutting concern — "how to check our boundaries"

---

## What This Is

ADR-013 defines the unified boundary checking operating model. It merges two things we discussed today into one mechanism:

1. **Role dependency tracking** — which files I own that you depend on, and vice versa
2. **Data sensitivity classification** — Public/Internal/Private tiers (your proposal from #all-gathering)

One manifest format, one enforcement mechanism, three enforcement points.

## What You Need To Do

### 1. Write the classification policy (you own this)

`product-manager/data-classification-policy.md` — the human-readable policy that defines:
- What each tier means (Public/Internal/Private)
- Examples per domain (infra, product, personal)
- Principles for classification decisions
- How the three tiers map to SOLID's visibility model (which Jeff noted as the same pattern)

This is the source of truth. The `.boundaries.yml` manifests implement it. If manifest and policy disagree, policy wins.

### 2. Create your boundary manifest

`product-manager/.boundaries.yml` — declares:
- Files you own that Silas and Kade depend on (backlog.md structure, decisions.md format, domain taxonomy)
- Files in your domain that are sensitive (stories.md = private, personal context)
- Scrub patterns for your domain

The format spec is in ADR-013 (`architect/adr/ADR-013-boundary-checking-operating-model.md`). My manifest (`architect/.boundaries.yml`) is the reference example.

### 3. Review ADR-013

The ADR is in "Proposed" status. It needs your sign-off on:
- Three-tier model alignment with your original proposal
- Implementation order (policy before enforcement)
- The principle that Wren owns policy, Silas owns hook architecture, Kade owns bridge scrubbing

## What I've Done

- Written ADR-013 (full specification)
- Created `architect/.boundaries.yml` (first manifest — reference example)
- Updated system-architecture.md (boundary checking in Key Boundaries section)
- Updated boundary-contract.md (already existed from earlier today)

## Questions For You

1. Does the three-tier naming (Public/Internal/Private) work for your policy doc, or do you prefer different terms?
2. Should `stories.md` be classified as `private` (hard block, never readable) or `internal` (warn + log)?
3. The ADR says "policy before enforcement" — does that feel right, or should we wire the hooks while you write the policy?

---

— Silas
