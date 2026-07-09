# ADR-051: Instance placement — per-domain named graphs, declared on the shape, no default

**Status:** Draft — 2026-07-09 (Wren, at Jeff's direction; Silas review pending — coherence/OWL-DBA lane; Jeff final).
**Card:** #3558 (recovery that exposed the gap) · #3630 (Fuseki lock, complementary) · #3583/#3593 (V1 retirement, consumers of this rule).
**Supersedes:** ADR-025 *placement clause only* — its schema/content separation rule (ontology graph carries schema, never instances) **survives and is restated here**. ADR-025's "all content lives in `urn:chorus:instances`" is retired.

## Context

ADR-025 was proposed 2026-04-24 and ratified 2026-06-17, but its content is pre-V2 law — its own class
table speaks SubProduct, Vertebra, and Machine. Since then the model was rebuilt: domains became the
generative unit (ADR-045), owl-api grew per-kind instance-graph resolution (#3570) with a silent
back-compat default to `urn:chorus:instances` "so unmigrated kinds keep serving."

That default is the defect this ADR exists to kill. Evidence, July 7–9:

- Product instances squatted in the default room in three naming generations at once; the generated
  pages served a stale 4-row copy while the real products sat unreadable in the ontology graph.
- The placement question has now been re-decided in chat **at least four times** (tests re-homing
  2026-06-21/22, the 2026-06-22 "instances is wrong" ruling, Q2 of the #3558 pair, and this ADR) —
  because the rule lived in conversation, nothing could refuse a wrong write.
- Jeff's framing, 2026-07-09: convenience defaults behave like junk food — agents reach for the easy
  room compulsively, regardless of standing rulings (same class as the standing /tmp rule). The only
  fix that holds is removing the convenient path, not restating the rule.

## Decision

1. **Every SHACL shape MUST declare `chorus:instancesGraph`.** The declaration on the shape is the
   single source of truth for where that kind's instances live.
2. **The rule for the value:** `urn:chorus:domains:<domain>`, where `<domain>` is the domain that
   `chorus:definesVocabulary` the class. Nouns home in the domain that owns their vocabulary.
3. **No default, ever.** owl-api's `resolve_instances_graph` back-compat branch is **deleted**:
   a class whose shape carries no declaration is REFUSED at `generate()` ("no instancesGraph declared
   — land the declaration first") and is not mounted by `serve`. The DAL (`chorus-model`) derives its
   write target from the same declaration and refuses likewise. A wrong-room write becomes
   structurally impossible instead of procedurally discouraged.
4. **`urn:chorus:instances` is frozen:** no new writes from ratification day; drained kind-by-kind by
   declared migrations; deleted when empty. Its existence is tracked as a migration-readout column so
   the drain is visible, not remembered.
5. **Schema/content separation carries forward** (the surviving half of ADR-025):
   `urn:chorus:ontology` holds classes, properties, axioms, and shapes — zero instances.

## Vocabulary-owner assignments required by (2) — [SILAS REVIEW]

Live gap, queried 2026-07-09: only Domain→`domains` and Service→`services` have defining domains.
The meta-classes must be assigned owners for the rule to resolve them. Draft positions:

| Class | definesVocabulary owner (draft) | Resulting home |
|---|---|---|
| Product | `domains` (the structural-model domain: the product/domain/service tree) | `urn:chorus:domains:domains` |
| ValueStream, ValueStreamStep | `domains` (same structural layer) | `urn:chorus:domains:domains` |
| Document | `knowledge` | `urn:chorus:domains:knowledge` |

Silas may re-assign any of these in review; the *mechanism* (2) is the decision, the table is a draft.

## Consequences

- **owl-api:** delete the default branch (lib.rs `resolve_instances_graph`); `generate()` hard-errors
  on undeclared shapes; tests pin the refusal. This is the enforcement conversion — Jeff's ruling
  becomes a refusal instead of a memory.
- **chorus-model:** write target derived from the declaration; same refusal on undeclared kinds.
- **Migrations:** first mover is the #3558 set — the 8 products + 8 documents relocate from
  `urn:chorus:instances` to their declared homes in one governed batch (relocation is subject-preserving;
  URIs never change — ADR-025's URI-stability guarantee carries forward).
- **`chorus_migration_readout`** grows a per-kind placement column (declared home vs actual rows) so
  drift is a dashboard fact, not an archaeology project.
- **Interim honesty:** until ratification, current placement (products/documents in
  `urn:chorus:instances`) is legal under ADR-025 and stays untouched. No relocation runs before Jeff
  accepts this ADR — placement has been re-decided in chat too many times already; this one lands as law
  or not at all.

## Lineage

The #3558 pair's Q2 ruling (load where owl-api resolves today) was the correct interim under ADR-025;
this ADR makes the declared per-domain home the rule and Q2's room a migration source. Consistent
progression, not a reversal. Companion security work: #3630 closes the anonymous write door that let
non-conformant data exist at all; ADR-050's DAL-only write path is assumed throughout.
