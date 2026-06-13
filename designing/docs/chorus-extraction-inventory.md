# Chorus Extraction Inventory — regenerated vs residue

**#2292 deliverable (2026-06-13, Kade).** The definitive, auditable list of every Chorus-concept file stranded in the Gathering repo (`jeff-bridwell-personal-site`), each classified **REGENERATED** (dies in place when its owl-api fan-out leg lands — never moved) or **RESIDUE** (no generated successor — a real move into its ADR-041 home). Supersedes ad-hoc grep. Raw scan: 3-agent sweep 2026-06-12 + authoritative re-scan 2026-06-13.

**Discriminator rule (ADR-041 + the regenerate-or-move model):** a domain *read surface* (handler + page + the service feeding it, where the domain is one of the 34 generated domains) is **REGENERATED** — the owl-api generates its successor and the rip-out rule deletes the hand-built original in the same leg. A piece is **RESIDUE** only if nothing generates a successor: the integration *engine* (NiFi), the *model data* the generator reads (TTLs), the *emit library* (spine-event), ADRs, and bespoke artifacts.

## Headline

**~29 files / ~31.5K LoC of chorus code in gathering — but most of it never moves.** The owl-api fan-out regenerates the bulk (every ICD/flow/team/loom/cards/roles/skills/briefs domain surface), so it dies in place leg by leg. The genuine **move pile (residue) is small**: the NiFi engine, the spine emit lib, the TTL model+instance data, ADRs, and a few bespoke artifacts. The inventory's value is that split — it tells us what costs move-effort (residue) and what costs nothing but a rip-out (regenerated).

| Class | Files | ~LoC | Move effort |
|---|---|---|---|
| **REGENERATED** — domain READ-APIs + model-render views (forge emits successor) | ~14 | ~4.3K | zero — deleted in the generating leg |
| **FUNCTIONAL SURFACE** residue — own-product UIs (board/flow/loom/werk/chorus; forge regenerates the domain-view NOT the UI) | ~7 | ~5.7K | real moves to product homes (Wren's split; ⚠ borderline rows to eyeball-confirm) |
| **RESIDUE** — engine/lib/model-data/artifacts | ~9 + 14 TTLs | ~3K code + 10.3K TTL | the move-cards |
| **CONSUMER / resolved** | 2 | ~0.5K | quality-scanner→proving/borg (Silas), cost stays (consumer) |

## REGENERATED — dies in place, no move (owl-api fan-out replaces)

**Wren's discriminator (page-owner ruling, folded 2026-06-13):** the forge regenerates the **domain VIEW** (an Athena domain-detail render of the model), NOT the **functional surface** (a product UI with behavior beyond rendering — the cards *board* with kanban+WIP logic, loom's functional pages, the werk dashboard). So a domain READ-API or a model-render view is REGENERATED (dies in place); a functional product UI is RESIDUE that MOVES to its product home — the forge never emits it. The rows below are split on that line.

### REGENERATED — domain READ-APIs + model-render views (forge emits the successor, rip in place)

| Source (gathering) | Type | LoC | Superseded by (fan-out) |
|---|---|---|---|
| `src/handlers/icd.handler.ts` | handler | 278 | generated ICD domain API |
| `src/services/icd.service.ts` | service | 1187 | generated ICD service/read layer |
| `src/validation/icd-validation.ts` | util | 216 | DAL SHACL — superseded (see rip-out precondition) |
| `views/icd.ejs` | view | 752 | generated ICD domain page (model render, API serves own page §5) |
| `src/services/team.service.ts` | service | 425 | generated cards/board domain READ-API (Vikunja read layer) |
| `src/handlers/roles.handler.ts` + `src/services/roles.service.ts` | handler+service | 157 | generated roles domain API (loom) — data endpoints, no functional UI |
| `src/handlers/skills.handler.ts` + `src/services/skills.service.ts` | handler+service | 137 | generated skills domain API — data endpoints |
| `src/handlers/briefs.handler.ts` + `src/services/briefs.service.ts` | handler+service | 165 | generated messages/briefs domain API — data endpoints |
| `src/handlers/decisions.handler.ts` | handler | 139 | generated decisions (loom) domain API |
| `tests/chorus-explorer-filters.test.js` | test | 716 | regenerated with the explorer |

### FUNCTIONAL SURFACES — RESIDUE (own-product UIs; forge regenerates the domain-view, NOT these). ⚠ Wren to eyeball-confirm the borderline rows.

These have behavior beyond rendering the model, so they MOVE to their product home rather than dying in a forge leg. Their *data* may come from a regenerated domain API, but the UI itself is product code.

| Source (gathering) | LoC | ADR-041 product home | Note |
|---|---|---|---|
| `src/handlers/cards.handler.ts` + `src/services/cards.service.ts` | 384 | `directing/clearing/domains/cards` | the BOARD — kanban + WIP logic (Wren's explicit example: NOT forge output) |
| `src/handlers/flow.handler.ts` + `views/flow.ejs` | 1025 | `directing/clearing/domains/cards` (flow view) or `building/werk` | funnel-by-sequence/chunk/domain + 7-day velocity = functional analytics, not a model render |
| `src/handlers/team.handler.ts` + `views/team.ejs` + `views/loom-role.ejs` | 1000 | `shaping/loom` | /loom team dashboard — role tiles + reflections + metrics = functional loom surface |
| `views/werk.ejs` | 1914 | `building/products/werk` | werk dashboard (tabs, charts) = functional surface |
| `views/chorus.ejs` | 958 | `shaping/loom` or athena artifacts | /chorus mind-map — bespoke visualization, not a domain page |
| `src/handlers/hooks.handler.ts` + `views/hooks.ejs` | 455 | `proving/borg/domains/gates` | ⚠ borg-owner (Silas) confirm: generated borg observability page (REGENERATED) vs functional dashboard (residue)? reads chorus.log |
| `src/handlers/fitness-functions.handler.ts` + `views/fitness-functions.ejs` | 444 | `proving/borg/domains/properties` | ⚠ same borg-owner question as hooks |

**Rip-out precondition (icd-validation.ts):** imported by gathering's OWN domain handlers (stories/photos/social). Its REGENERATED rip must NOT fire until the DAL SHACL successor is live for those callers — the rip-out guard on the ICD generation leg (sequencing dependency, not a free delete).

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
| ADR-006-bridge-scope-guardrail | — | **#2298 IS STALE — ALREADY HOME.** Verified 2026-06-13: ADR-006 already lives at `chorus/roles/silas/adr/ADR-006-bridge-scope-guardrail.md`; no copy in the gathering repo. So #2298 ("migrate ADR-006 from gathering") is a no-op — it was already extracted. Future move target per ADR-041 = `shaping/products/loom/domains/decisions` (decisions is a loom domain, NOT athena), which #2298's title's "chorus/designing/decisions" predates. Recommend: close #2298 as already-done or repoint it to the attrition-relocation of chorus's existing ADRs into the loom tree. |

## AMBIGUOUS / consumer rulings (cold-eyes catch, resolved)

| Source | LoC | Ruling |
|---|---|---|
| `src/handlers/quality.handler.ts` + `src/services/quality-scanner.service.ts` + `views/quality-service.ejs` | ~480 | **RESIDUE → `proving/borg`** (Silas's ruling 2026-06-13): it produces a code-health SIGNAL the team/Borg consumes (coverage/complexity/quality trend) across BOTH roots = observation = Borg's lane (quality or properties domain). If a gathering-SPECIFIC lint half is bundled, split that out to gathering; the cross-root quality observer is Borg's. |
| `src/handlers/cost.handler.ts` | ~varies | **CONSUMES-CHORUS, DOES-NOT-MOVE**: reads chorus `clearing/transcripts` to compute Clearing cost — a chorus-data consumer (like a metrics scrape), a Gathering cost-dashboard feature. Stays in gathering. |

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

~33 files enumerated (10 handlers, 8 services, 11 views, 2 utils, 1 test) + 14 TTLs, ~32.4K LoC + 10.3K TTL. Grep targets (nudge/spine/clearing/pulse/role-state/icd/convergence/nifi/cards/loom/werk/hooks/fitness) accounted.

**Revision (cold-eyes, 2026-06-13):** the first pass MISSED the hooks/fitness/quality dashboard cluster (~1.4K LoC reading chorus.log) and misrouted ADR-006 to athena (it's loom). Both added/corrected above. Negative discrimination verified sound — `gathering-graph.ejs` (0 chorus refs) correctly EXCLUDED vs the near-namesake `gathering-chorus-system-graph.ejs` (chorus, included). This is a living document: re-run the scan against this list before #2299's final gate to catch any remaining stray — do NOT treat "complete" as proven until that gate is green.
