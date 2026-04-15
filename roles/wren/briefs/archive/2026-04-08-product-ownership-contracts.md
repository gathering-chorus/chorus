# Brief: Product Ownership + Horizontal Contracts

**From:** Silas (Architect)
**To:** Wren (PM)
**Date:** 2026-04-08
**Context:** Jeff-led discussions on repo structure, team reliability, and coordination failures

## Decisions from today's discussions

### 1. Product ownership (not infra ownership)

Everything is code, not infrastructure. Each product gets an owner who builds, tests, monitors, and maintains it. The filesystem namespace makes ownership visible: `platform/roles/<owner>/products/<product>/`.

| Product | Owner | Notes |
|---------|-------|-------|
| Clearing | Kade | Multi-party chat |
| Cards | Wren | Board CLI + Vikunja |
| Convergence | Kade (code) / Silas (arch) | ICDs, NiFi, harvesters — needs a home in repo |
| Chorus (protocol) | Silas | Hooks, spine, nudge, role state |
| Loom | Kade | Gathering app — separate repo |
| Designing tools | TBD | Backlog |
| Building tools | TBD | Backlog |

### 2. Pair as default, solo as exception

Inverts the current model. The 28% correction rate and 19x repeat instruction increase show that solo builds drop cross-cutting concerns. Pair on anything that spans products or repos. Solo only for vertical work within your own product.

### 3. Don't close until propagation is done

Cards should not be marked Done when the code ships but the docs, plists, paths, and references haven't been updated. #1308 shipped with "CHORUS_ROOT externalized" but the plists never got deployed. The card said done, the system said broken. A card is done when the downstream is clean — not when the code compiles.

## Horizontal contracts

Jeff's insight: roles have vertical ownership (products) and horizontal responsibilities that cut across all products. The spine event contract we defined for observability is the first of these. There should be contracts for each horizontal concern:

### Observability contract (piloting on Cards — #1807)
- **What:** Products emit spine events in `domain.noun.verb_past` format with structured fields
- **Who produces:** Product owner instruments their code
- **Who consumes:** Silas — correlates across products, detects system-level patterns
- **Contract:** Spine event schema. Product conforms to it. Silas consumes from it. No coupling.
- **Pattern:** Product defines health. Silas provides the platform. Product owner writes alert definitions, Silas runs them.

### Quality contract (future)
- **What:** Products declare their quality gates — what tests exist, what coverage means, what "done" looks like
- **Who produces:** Product owner defines AC, writes tests, runs TDD
- **Who consumes:** Wren — validates AC coverage at demo, Silas — runs gate hooks
- **Contract:** AC format, test naming conventions, demo ceremony. Product conforms. Wren validates.

### Code contract (future)
- **What:** Products declare their build, test, and deploy steps
- **Who produces:** Product owner maintains build config, dependency declarations
- **Who consumes:** Kade — build pipeline, CI. Silas — deploy orchestration.
- **Contract:** Bootstrap script, package.json / Cargo.toml conventions, deploy targets. Products conform. Infra consumes.

### Design contract (future)
- **What:** Products declare their domain model, ICDs, and interface boundaries
- **Who produces:** Product owner + Silas (architecture)
- **Who consumes:** All roles — for cross-product integration, ontology coherence
- **Contract:** Domain context files, ICD provider sections, ontology alignment. Products conform. System integrates.

## The pattern

Each horizontal contract follows the same structure:
1. **Product conforms** — emits/declares in the contract's format
2. **Horizontal owner consumes** — reads the contract, acts on it
3. **No coupling** — product doesn't know how it's consumed. Consumer doesn't know product internals.
4. **The contract is the interface** — same ICD pattern applied inward

This is Jeff's street crew analogy solved: instead of each crew opening and closing the road independently, there's a shared specification of what the road looks like at each stage. Each crew conforms to the spec. The coordinator reads the spec. Nobody needs to know each other's internals.

## Action items

- [ ] #1807 — Pilot observability contract on Cards (Wren instruments, Silas consumes)
- [ ] Formalize pair-as-default into team protocol
- [ ] Formalize "done means propagation done" into card close-out gate
- [ ] Card the bootstrap script (cargo build + npm install + launchctl bootstrap)
- [ ] Card Convergence's home in the repo

---
Auto-generated from Jeff-led discussions on 2026-04-08.
