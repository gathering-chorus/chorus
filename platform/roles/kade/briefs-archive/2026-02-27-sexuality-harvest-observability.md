# Brief: Wire spine events into sexuality harvest scripts

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-02-27
**Re:** Harvest run observability for overnight sexuality pipeline

## Context

Your new sexuality extract/load scripts need spine event emissions so ops can monitor overnight runs. If the extract fails at 500K of 1.8M items, nobody knows until someone checks manually.

## What to add

Wire `chorus-log.sh` calls into your sexuality harvest scripts — same pattern as music (#369):

```bash
CHORUS_LOG="/Users/jeffbridwell/CascadeProjects/messages/scripts/chorus-log.sh"

# At extract start:
"$CHORUS_LOG" harvest.sexuality.extract.started kade "source=mongodb" "expected=1800000"

# At extract end:
"$CHORUS_LOG" harvest.sexuality.extract.completed kade "count=$COUNT" "duration=${ELAPSED}s" "status=ok"

# At load start:
"$CHORUS_LOG" harvest.sexuality.load.started kade "target=fuseki"

# At load end:
"$CHORUS_LOG" harvest.sexuality.load.completed kade "count=$LOADED" "duration=${ELAPSED}s" "status=ok"

# On failure (in error handler / trap):
"$CHORUS_LOG" harvest.sexuality.extract.failed kade "error=$ERR" "count=$PARTIAL"
```

## Why it matters

- `graph-lint.sh` checks manifest drift but only AFTER a run completes
- No alert fires if a run silently dies mid-extraction
- Spine events show up in the Werk spine tab — Jeff and ops can see harvest progress live
- The overnight run especially needs this — nobody's watching at 2am

## Event naming convention

`harvest.<domain>.<stage>.<verb_past>` per DEC-044 / #444 normalization.

Stages: `extract`, `transform`, `load`, `verify`.
Verbs: `started`, `completed`, `failed`.
