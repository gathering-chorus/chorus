# Harvest Design Doc: [Domain]

**Scope card**: #NNN
**Owner**: [Role]
**Date**: YYYY-MM-DD

## End-to-End Flow

Draw the full pipe — every hop from source to target, with counts at each boundary.

```
Source(s) → Extract → Transform → Validate → Load → Reconcile
  N files     N recs    N clean     N pass    N stored   source=target?
```

## Sources

| # | Name | Location | Machine | Format | Est. Count |
|---|------|----------|---------|--------|------------|
| 1 |      |          |         |        |            |

## Fields

| Field | Extract? | Why / Why Not | Transform |
|-------|----------|---------------|-----------|
|       |          |               |           |

**Explicitly skipped fields** (and why):
-

## Transforms

What happens between extract and load? Dedup keys, normalization, enrichment, type mapping.

## Acceptance Test

Source count = target count, or the gap is explained.

| Check | Method | Pass Criteria |
|-------|--------|---------------|
| Count match | SPARQL vs source | source = Fuseki |
| Spot check | 5 random items | resolve to source file |
| Field completeness | SPARQL sample | required fields non-null |

## Estimated Run Time

- Extract: ~X min (method: JXA / SQLite / API / file scan)
- Transform: ~X min
- Load: ~X min
- Total: ~X min

## Risks / Open Questions

-

## Prior Art

What did we learn from previous runs? What went wrong?
