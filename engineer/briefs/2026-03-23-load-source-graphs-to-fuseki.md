---
from: silas
date: 2026-03-23
card: 1642
type: request
---

# Load validated source data to Fuseki as governed source graphs

Kade — Jeff wants the source data we've validated loaded authoritatively into Fuseki so we can govern from it. Two tasks:

## 1. Load source graphs

Convert the 3 source JSON extracts to TTL and load as named graphs:

- `architect/docs/source-apple-photos-mac.json` → `urn:gathering:photos/source/apple` (24,592 records)
- `architect/docs/source-google-takeout.json` → `urn:gathering:photos/source/takeout` (102,490 records)
- `architect/docs/source-iphone.json` → `urn:gathering:photos/source/iphone` (54,479 records)

**Important:** Check if a Takeout source graph already exists in Fuseki before loading — the existing canonical was built from Takeout, so there may already be source data there. Don't double-load.

Each record maps to existing ICD fields: `jb:filename`, `jb:photoDate`, `jb:width`, `jb:height`, `jb:latitude`, `jb:longitude`, `jb:fileSize`, `jb:mediaType`. Use the canonical property URIs from icd-instance-photos.ttl mappings.

**Missing value handling** (from my independent analysis):
- Apple: lat/lng = -180.0 means missing → omit the triple
- Apple: fileSize = 0 means missing (55 records) → omit
- Takeout: width/height/fileSize = 0 for ALL records → omit (format limitation, not data)
- Takeout: lat/lng = null means missing → omit
- Takeout: filename/dateTaken = "" (2 records) → omit those records entirely
- iPhone: lat/lng = null means missing → omit
- iPhone: fileSize = 0 (130 records) → omit

## 2. Wire era authority into ICD page

The ICD page (`/harvesting/convergence`) needs to query and render the new era authority data. The triples are already in `urn:gathering:icd/current` (Silas loaded them). New sections needed in `icd.service.ts` and `icd.ejs`:

- **Era timeline** — show the 4 eras with date ranges per domain
- **Authority rules** — show golden/supplementary/volume-fill per provider per era
- **Field overrides** — show where a non-golden provider is preferred for specific fields
- **Migration events** — show platform migrations per provider with impact descriptions
- **Metadata ceilings** — show which fields are impossible per provider per era

SPARQL classes to query: `icd:Era`, `icd:EraAuthority`, `icd:EraFieldOverride`, `icd:MigrationEvent`, `icd:MetadataCeiling`. All in graph `urn:gathering:icd/current`.

## Validation Gate (AC for source graph loading)

After loading, SPARQL queries against each source graph must match the 3-way comparison HTML (`architect/docs/photos-source-3way-comparison.html`). Jeff pointed at that HTML as "the data I expect to see." These are the numbers:

| Source | Graph | Records | Dimensions | GPS |
|--------|-------|---------|-----------|-----|
| Apple | `urn:gathering:photos/source/apple` | 24,592 | 100% | 50.2% |
| Takeout | `urn:gathering:photos/source/takeout` | 102,490 | 0% (format ceiling) | 73.1% |
| iPhone | `urn:gathering:photos/source/iphone` | 54,479 | 100% | 89.3% |

Verification queries (run after each load):
```sparql
# Record count
SELECT (COUNT(DISTINCT ?s) AS ?count) WHERE {
  GRAPH <urn:gathering:photos/source/apple> { ?s a jb:Photo }
}

# Dimensions coverage
SELECT (COUNT(?s) AS ?withDims) WHERE {
  GRAPH <urn:gathering:photos/source/apple> { ?s jb:width ?w }
}

# GPS coverage
SELECT (COUNT(?s) AS ?withGPS) WHERE {
  GRAPH <urn:gathering:photos/source/apple> { ?s jb:latitude ?lat }
}
```

If any count is off, the load has a bug. Don't proceed to merge (#1643) until these match.

Jeff's direction: "I want to see this data governed — captured authoritatively so we can govern from it." This is the foundation for the canonical rebuild (#1643).
