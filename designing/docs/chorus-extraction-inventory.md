# Chorus Extraction Inventory — regenerated vs residue

**#2292 deliverable (2026-06-13, Kade).** The definitive, auditable list of every Chorus-concept file stranded in the Gathering repo (`jeff-bridwell-personal-site`), each classified **REGENERATED** (dies in place when its owl-api fan-out leg lands — never moved) or **RESIDUE** (no generated successor — a real move into its ADR-041 home). Supersedes ad-hoc grep. Raw scan: 3-agent sweep 2026-06-12 + authoritative re-scan 2026-06-13.

**Discriminator rule (ADR-041 + the regenerate-or-move model):** a domain *read surface* (handler + page + the service feeding it, where the domain is one of the 34 generated domains) is **REGENERATED** — the owl-api generates its successor and the rip-out rule deletes the hand-built original in the same leg. A piece is **RESIDUE** only if nothing generates a successor: the integration *engine* (NiFi), the *model data* the generator reads (TTLs), the *emit library* (spine-event), ADRs, and bespoke artifacts.

## Headline

**~29 files / ~31.5K LoC of chorus code in gathering — but most of it never moves.** The owl-api fan-out regenerates the bulk (every ICD/flow/team/loom/cards/roles/skills/briefs domain surface), so it dies in place leg by leg. The genuine **move pile (residue) is small**: the NiFi engine, the spine emit lib, the TTL model+instance data, ADRs, and a few bespoke artifacts. The inventory's value is that split — it tells us what costs move-effort (residue) and what costs nothing but a rip-out (regenerated).

| Class | Files | ~LoC | Move effort |
|---|---|---|---|
| **REGENERATED** (rip in place, fan-out leg) | 18 | ~9.0K | zero — deleted when the generated successor lands |
| **RESIDUE** (real move) | ~11 + 14 TTLs | ~4.5K code + 10.3K TTL | the actual move-cards |

## REGENERATED — dies in place, no move (owl-api fan-out replaces)

Each is a domain read surface whose generated Domain/Service API + page supersedes it; the rip-out rule (ADR-041 §4) deletes it in the generating leg.

| Source (gathering) | Type | LoC | Superseded by (fan-out) |
|---|---|---|---|
| `src/handlers/icd.handler.ts` | handler | 278 | generated ICD domain API |
| `src/services/icd.service.ts` | service | 1187 | generated ICD service/read layer |
| `src/validation/icd-validation.ts` | util | 216 | DAL SHACL (authored-vs-hydrated) — superseded, not moved |
| `views/icd.ejs` | view | 752 | generated ICD page (API serves own page, §5) |
| `src/handlers/flow.handler.ts` + `views/flow.ejs` | handler+view | 1025 | generated flow page |
| `src/handlers/team.handler.ts` + `views/team.ejs` + `views/loom-role.ejs` | handler+views | 1000 | generated loom/team page |
| `src/services/team.service.ts` | service | 425 | generated cards/board domain service |
| `src/handlers/cards.handler.ts` + `src/services/cards.service.ts` | handler+service | 384 | generated cards domain API |
| `src/handlers/roles.handler.ts` + `src/services/roles.service.ts` | handler+service | 157 | generated roles domain API (loom) |
| `src/handlers/skills.handler.ts` + `src/services/skills.service.ts` | handler+service | 137 | generated skills domain API |
| `src/handlers/briefs.handler.ts` + `src/services/briefs.service.ts` | handler+service | 165 | generated messages/briefs domain API |
| `src/handlers/decisions.handler.ts` | handler | 139 | generated decisions (loom) API |
| `views/werk.ejs` | view | 1914 | generated werk dashboard |
| `views/chorus.ejs` | view | 958 | generated /chorus page |
| `tests/chorus-explorer-filters.test.js` | test | 716 | regenerated with the explorer |

## RESIDUE — real moves, with ADR-041 destinations

These have no generated successor. **Destinations are the LANDED ADR-041 tree** (`roles/silas/adr/ADR-041-*`). Note: there is **no `building/products/chorus`** — coordination surfaces live under `directing/clearing` or `shaping/loom`; the model lives under `designing/athena`.

### Integration engine → `building/products/convergence/domains/integrations/`
| Source | LoC | Note |
|---|---|---|
| `src/services/nifi.service.ts` | 256 | Convergence's NiFi engine. **Creds + PG-map prep = #3383** (gathering-repo) lands FIRST, then this moves clean. No generated successor — it's the engine the generated APIs would call. |

### Emit library → `directing/products/clearing/spine/domains/events/`
| Source | LoC | Note |
|---|---|---|
| `src/utils/spine-event.ts` | 126 | The spine emit lib (Loki + spine-events.json vertebra map). Real code, no generated successor; couples only to Logger. Sibling of the chorus-side emit contract → candidate for `lib/` if shared. |

### Model + instance data (TTL) → `designing/products/athena/domains/domains/`
The OWL/SHACL the generator READS — source-of-truth data, moves to the model home (the generator never regenerates these; it consumes them).
| Source | LoC | Destination |
|---|---|---|
| `src/ontology/icd-ontology.ttl` | 432 | athena/domains/domains (ICD class defs) |
| `src/ontology/jb-ontology.ttl` | 2468 | athena/domains/domains (gathering model — cross-linked to ICD) |
| `src/ontology/jb-ontology-shapes.ttl` + `shapes/photo-shape.ttl` | 113+ | athena/domains/domains (SHACL) |
| `src/ontology/icd-instance-*.ttl` (9 files: photos/music/people/social/documents/stories/notes/notes-v2/webmethods) | 7278 | **convergence/integrations/icd-instances/** (domain instance data the ICD/NiFi pipeline loads) — move ontology class defs FIRST (instances reference their URIs) |

### Bespoke reference artifacts → `designing/products/athena/` (artifacts/knowledge)
No domain API equivalent; reference visualizations.
| Source | LoC | Destination |
|---|---|---|
| `views/chorus-system.ejs` | 904 | athena artifacts (system-state reference) — verify still wired; if dead, retire not move |
| `views/gathering-chorus-system-graph.ejs` | 1026 | athena artifacts (integration graph) |
| `views/chorus-model-data.ejs` | 484 | athena/domains/domains (model explorer) |
| `views/ontology-views/chorus.ejs` | 411 | athena/domains/domains (ontology view) |
| `public/gathering-chorus.html` | 355 | athena artifacts (integration reference) |
| ADR-006-bridge-scope-guardrail (in gathering/data/about) | — | **#2298** → athena/domains/decisions |

## Coupling surface (what any move must shim or cut)

Chorus pieces couple to **exactly 4 gathering-shared modules** — clean, no domain-logic entanglement:
1. **Logger** (`src/logger.ts`) — every handler/service. Chorus provides its own; pass as dependency. (REGENERATED pieces don't need it — they die.)
2. **sparql-constants** (`src/config/sparql-constants.ts`, NS/PREFIX) — icd.service only (REGENERATED).
3. **sparql-escape** (`src/utils/sparql-escape.ts`) — icd.service only (REGENERATED).
4. **Express auth middleware** — applied at route registration in app.ts, NOT inside handlers; stays in gathering when routes move.

The residue pieces couple to almost nothing: nifi.service (zero gathering imports), spine-event (Logger only), TTLs (pure RDF, zero imports). **The residue moves clean** — the coupling is concentrated in the regenerated pieces, which don't move anyway.

## Sequencing (feeds the move-cards)

1. ADR-041 landed ✓ · #3097 chorus-side prep landed ✓.
2. **#3383** (gathering-repo NiFi creds+PG-map prep) — before nifi.service moves.
3. **Model TTLs move** (ontology class defs → instances) → athena/domains + convergence/integrations.
4. **spine-event** → spine/domains/events (or lib/).
5. **Bespoke artifacts** → athena artifacts (verify-wired-or-retire each).
6. **#2298** ADR-006 → athena/domains/decisions · **#2299** final contract gate.
7. REGENERATED pieces need NO move-card — each dies in its fan-out leg's rip-out.

**The crawl-truth driver:** all chorus code (regenerated rips + residue moves) must be out of gathering before Silas's crawler-rewrite leg, or the graph attributes these files to the wrong product.

## Completeness

29 files enumerated (8 handlers, 7 services, 9 views, 2 utils, 1 test) + 14 TTLs, ~31.5K LoC + 10.3K TTL. Grep targets (nudge/spine/clearing/pulse/role-state/icd/convergence/nifi/cards/loom/werk) all accounted. Auditable; re-run the scan against this list to verify zero new strays before #2299's final gate.
