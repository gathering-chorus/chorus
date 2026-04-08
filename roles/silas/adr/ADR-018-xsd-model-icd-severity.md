# ADR-018: XSD Model for ICD Severity Tiers

**Status:** Accepted
**Date:** 2026-03-19
**Deciders:** Jeff, Silas, Wren
**Cards:** #1529 (migration), #1528 (normalization), #1527 (enforcement)

## Context

The ICD (Interface Control Document) in SEMANTIC_MAPPER.html originally used three severity tiers for consumer fields:

- **Violation (Required)** — record rejected without this field
- **Warning (Expected)** — accepted but gap tracked
- **Info (Completeness)** — accepted, reported as metric

The three-tier model created a "soft middle" where Warning fields accumulated as data quality debt. Nobody fixed warnings because they didn't block anything. 73 fields across 7 domains sat in this middle tier indefinitely.

Jeff identified the analogy: the ICD should work like an XSD (XML Schema Definition). An XSD doesn't have "optional for things you expect to be there" — it has `minOccurs="1"` and the parser rejects the document if it's missing.

## Decision

**Two tiers only:**

| Tier | Meaning | ICD attribute | Gate behavior |
|------|---------|---------------|---------------|
| **Required** (`violation`) | A source exists that provides this field. Record rejected without it. | `data-severity="violation"` | `validateFromICD()` rejects |
| **Enrichment** (`enrichment`) | No source wired yet. Future capability. | `data-severity="enrichment"` | Not checked — field doesn't exist in current pipeline |

**No middle tier.** No "Warning" or "Expected" or "Info."

### Tier assignment rule

- **Does at least one provider have a `data-maps-to` for this field?** → Required
- **No provider maps to this field yet?** → Enrichment

Source coverage percentage does NOT determine the tier. LinkedIn email at ~30% coverage doesn't make email "Enrichment" — it means 70% of LinkedIn records are rejected at the email field. The source limitation is visible in the ICD coverage %, but the schema doesn't compromise.

### Tier Change Protocol

When a Required field has <50% source coverage and the dry run rejects most records:

1. **Is this field truly required for the canonical type?** If not → move to Enrichment with rationale documented in cf-row comment.
2. **Is this field required for a pipeline operation (e.g., dedup) but not the record itself?** → Two schemas: record schema (lenient) and pipeline schema (strict).
3. **Does moving to Enrichment create downstream compensation?** If the UI needs the field → keep Required, accept smaller dataset.

Every tier change is recorded in the ICD with a comment: why it moved, who decided, what the coverage was.

**Tier changes flow through the ICD (Phase 2), never through the code (Phase 3).** If a developer changes the schema to make a field optional without updating the ICD, that's a process violation.

## Consequences

### Positive
- Eliminates silent data quality debt — no fields sit in a "we'll track it" limbo
- Automation can trust Required fields exist — no fallback logic needed in UI code
- Every field decision is deliberate and recorded
- Lint enforcement via `icd-lint.py --strict` catches any drift

### Negative
- May reject records that were previously accepted with gaps — dataset sizes may shrink
- Requires deliberate decision per field when onboarding new domains — more upfront work
- "Enrichment" fields are invisible to the validation gate — no data quality tracking until a source is wired

### Neutral
- 73 existing fields migrated (Notes 4, Documents 5, Music 7, Stories 6, Photos 13, People 14, Social 24)
- `icd-lint.py --strict` runs as verification — 7/7 domains pass with 0 errors, 0 warnings

## Prior Art

- **XSD (XML Schema Definition)** — `minOccurs="1"` as the model for Required fields
- **Staples ARB ICD template** — field-level mapping with source attribution
- **Staples Athena/Anzo** — ICD as ontology, not document (future: #1530)
- **DEC-095** — define → map → build. The XSD model makes "define" rigorous.
