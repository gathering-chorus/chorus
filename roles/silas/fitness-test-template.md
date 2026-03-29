# Collection Fitness Test Template

**Purpose**: Verify that the ingestion pipeline (Turtle authoring → filesystem → Fuseki sync → SPARQL) produces correct, complete, schema-conformant triples for any collection type.

**Fuseki endpoint**: `http://localhost:3031/pods/sparql`

---

## Layer 1: Pipeline Health

Does the data exist and does Fuseki match the filesystem?

### 1.1 — Resource Count by Type

```sparql
PREFIX jb: <https://jeffbridwell.com/ontology#>

SELECT ?type (COUNT(DISTINCT ?s) AS ?count)
WHERE {
  GRAPH ?g {
    ?s a ?type .
    FILTER(STRSTARTS(STR(?type), 'https://jeffbridwell.com/ontology#'))
  }
}
GROUP BY ?type
ORDER BY DESC(?count)
```

**What it proves**: Resources made it from Turtle files through FusekiSyncService into named graphs. Compare counts against filesystem (`ls data/pods/jeff/{collection}/*.ttl | wc -l`). Mismatch = sync drift.

**Pass criteria**: Fuseki count = filesystem count for each collection.

### 1.2 — Named Graph Inventory

```sparql
PREFIX jb: <https://jeffbridwell.com/ontology#>

SELECT ?g (COUNT(?s) AS ?triples)
WHERE {
  GRAPH ?g { ?s ?p ?o }
  FILTER(CONTAINS(STR(?g), '/{COLLECTION}/'))
}
GROUP BY ?g
ORDER BY ?g
```

Replace `{COLLECTION}` with: `books`, `blog/posts`, `property`, `ideas`, `projects`.

**What it proves**: Each resource has its own named graph. Graph URI maps to filesystem path. Empty graphs = sync wrote the graph but no triples loaded.

**Pass criteria**: Every Turtle file on disk has a corresponding non-empty named graph.

### 1.3 — Orphan Detection (graphs with no typed resource)

```sparql
PREFIX jb: <https://jeffbridwell.com/ontology#>
PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>

SELECT ?g
WHERE {
  GRAPH ?g { ?s ?p ?o }
  FILTER(CONTAINS(STR(?g), '/pods/'))
  FILTER NOT EXISTS {
    GRAPH ?g { ?s rdf:type ?type }
  }
}
```

**What it proves**: Every named graph has at least one typed resource. Graphs without types indicate parsing errors or incomplete writes.

**Pass criteria**: Zero results.

---

## Layer 2: Schema Completeness

Are the expected properties present?

### 2.1 — Property Completeness Scorecard

Per-collection query. Replace `{CLASS}` and the property list per collection config below.

```sparql
PREFIX jb: <https://jeffbridwell.com/ontology#>
PREFIX schema: <https://schema.org/>
PREFIX dcterms: <http://purl.org/dc/terms/>

SELECT
  (COUNT(DISTINCT ?resource) AS ?total)
  (COUNT(DISTINCT IF(BOUND(?title), ?resource, 1/0)) AS ?has_title)
  (COUNT(DISTINCT IF(BOUND(?visibility), ?resource, 1/0)) AS ?has_visibility)
  (COUNT(DISTINCT IF(BOUND(?slug), ?resource, 1/0)) AS ?has_slug)
  # Add collection-specific OPTIONAL bindings below
WHERE {
  GRAPH ?g {
    ?resource a jb:{CLASS} .
    OPTIONAL { ?resource dcterms:title ?title }
    OPTIONAL { ?resource jb:hasVisibility ?visibility }
    OPTIONAL { ?resource jb:slug ?slug }
    # Add collection-specific OPTIONALs here
  }
}
```

**Pass criteria**: Required properties at 100%. Optional properties tracked for trend — declining coverage signals ingestion regression.

### 2.2 — Collection Property Configs

#### Books (`jb:Book`)

| Property | Required | Notes |
|----------|----------|-------|
| `dcterms:title` | Yes | |
| `schema:author` | Yes | |
| `jb:slug` | Yes | |
| `jb:hasVisibility` | Yes | |
| `jb:locationRoom` | Yes | Physical location |
| `jb:locationBookcase` | Yes | Physical location |
| `jb:locationShelf` | Yes | Physical location |
| `jb:coverImage` | Yes | Photo of cover |
| `schema:isbn` | Expected | Missing = manual entry gap |
| `schema:publisher` | Expected | |
| `schema:datePublished` | Expected | |
| `schema:numberOfPages` | Optional | From Open Library enrichment |
| `schema:about` | Optional | Subject tags, multi-valued |
| `jb:openLibraryKey` | Optional | Present if enriched |

#### Blog Posts (`jb:BlogPost`)

| Property | Required | Notes |
|----------|----------|-------|
| `dcterms:title` | Yes | |
| `schema:author` | Yes | |
| `schema:datePublished` | Yes | |
| `schema:dateModified` | Yes | |
| `jb:slug` | Yes | |
| `jb:hasVisibility` | Yes | |
| `jb:excerpt` | Yes | |
| `jb:permalink` | Yes | WordPress source URL |
| `jb:externalId` | Yes | WordPress post ID |
| `jb:harvestedFrom` | Yes | Source URI |
| `jb:harvestedAt` | Yes | Harvest timestamp |
| `jb:hasCategory` | Expected | Multi-valued |
| `jb:hasTag` | Optional | Few posts have tags |

#### Ideas (`jb:Idea`)

| Property | Required | Notes |
|----------|----------|-------|
| `dcterms:title` | Yes | |
| `jb:slug` | Yes | |
| `jb:hasVisibility` | Yes | |
| `jb:hasIdeaStatus` | Yes | Captured/Developing/Parked/Merged |
| `jb:addedAt` | Yes | |
| `jb:summary` | Expected | Brief description |
| `jb:promotedTo` | Optional | Cross-collection link to Project |

#### Projects (`jb:Project`)

| Property | Required | Notes |
|----------|----------|-------|
| `dcterms:title` | Yes | |
| `jb:slug` | Yes | |
| `jb:hasVisibility` | Yes | |
| `jb:addedAt` | Expected | |

#### Property (`jb:House`, `jb:Room`, `jb:Garden`, `jb:GardenBed`, `jb:Land`)

| Property | Required | Notes |
|----------|----------|-------|
| `dcterms:title` or `rdfs:label` | Yes | |
| `jb:hasVisibility` | Yes | (on container, not every sub-resource) |

### 2.3 — Find Resources Missing Required Properties

```sparql
PREFIX jb: <https://jeffbridwell.com/ontology#>
PREFIX dcterms: <http://purl.org/dc/terms/>

SELECT ?title ?resource
WHERE {
  GRAPH ?g {
    ?resource a jb:{CLASS} .
    OPTIONAL { ?resource dcterms:title ?title }
    OPTIONAL { ?resource jb:hasVisibility ?vis }
    OPTIONAL { ?resource jb:slug ?slug }
    FILTER(!BOUND(?title) || !BOUND(?vis) || !BOUND(?slug))
  }
}
```

**Pass criteria**: Zero results for required properties.

---

## Layer 3: Data Richness

Is the data connected and discoverable?

### 3.1 — Subject/Category Tag Coverage

```sparql
PREFIX jb: <https://jeffbridwell.com/ontology#>
PREFIX schema: <https://schema.org/>
PREFIX dcterms: <http://purl.org/dc/terms/>

SELECT ?title (COUNT(?tag) AS ?tag_count)
WHERE {
  GRAPH ?g {
    ?resource a jb:{CLASS} ;
              dcterms:title ?title .
    OPTIONAL { ?resource schema:about ?tag }
  }
}
GROUP BY ?title
ORDER BY ASC(?tag_count)
```

Replace `schema:about` with the appropriate tagging property per collection (`jb:hasCategory` for blog, `schema:about` for books).

**What it proves**: Resources with zero tags are discoverable only by title/author. Tags enable cross-resource and cross-collection discovery. Low tag counts signal enrichment opportunities.

**Pass criteria**: No hard threshold. Track the distribution. Flag resources with zero tags for enrichment.

### 3.2 — Cross-Collection Connection Count

```sparql
PREFIX jb: <https://jeffbridwell.com/ontology#>

SELECT
  (COUNT(DISTINCT ?resource) AS ?total_resources)
  (COUNT(DISTINCT ?connected) AS ?cross_connected)
WHERE {
  {
    GRAPH ?g {
      ?resource a ?type .
      FILTER(STRSTARTS(STR(?type), 'https://jeffbridwell.com/ontology#'))
    }
  }
  OPTIONAL {
    GRAPH ?g1 { ?connected ?p ?target }
    GRAPH ?g2 { ?target a ?targetType }
    FILTER(?g1 != ?g2)
    FILTER(STRSTARTS(STR(?targetType), 'https://jeffbridwell.com/ontology#'))
    BIND(?connected AS ?connected)
  }
}
```

**What it proves**: How many resources link to resources in other collections. This is the cross-domain connection ratio from `system-architecture.md`. If this number stays flat as resources are added, the graph is accumulating without connecting.

**Pass criteria**: Trending upward as content is added. Current baseline to be established.

### 3.3 — Visibility Distribution

```sparql
PREFIX jb: <https://jeffbridwell.com/ontology#>

SELECT ?type ?visibility (COUNT(DISTINCT ?resource) AS ?count)
WHERE {
  GRAPH ?g {
    ?resource a ?type ;
              jb:hasVisibility ?visibility .
    FILTER(STRSTARTS(STR(?type), 'https://jeffbridwell.com/ontology#'))
  }
}
GROUP BY ?type ?visibility
ORDER BY ?type ?visibility
```

**What it proves**: Graduation model is being used. All resources should have a visibility declaration. Distribution shows whether the collection is still in the "workshop" or has graduated content.

**Pass criteria**: Every resource has `jb:hasVisibility`. Distribution reviewed, not enforced — it's a product metric, not a pass/fail.

---

## Layer 4: Consistency Checks

Does the data follow expected patterns?

### 4.1 — Duplicate Detection (same title within a collection)

```sparql
PREFIX jb: <https://jeffbridwell.com/ontology#>
PREFIX dcterms: <http://purl.org/dc/terms/>

SELECT ?title (COUNT(DISTINCT ?resource) AS ?copies)
WHERE {
  GRAPH ?g {
    ?resource a jb:{CLASS} ;
              dcterms:title ?title .
  }
}
GROUP BY ?title
HAVING (COUNT(DISTINCT ?resource) > 1)
```

**Pass criteria**: Zero results (no duplicate titles within a collection). Multi-volume works (Lone Wolf and Cub) should have distinct titles per volume.

### 4.2 — Type Conformance (dual typing check)

```sparql
PREFIX jb: <https://jeffbridwell.com/ontology#>
PREFIX schema: <https://schema.org/>

SELECT ?resource (GROUP_CONCAT(STR(?type); separator=', ') AS ?types)
WHERE {
  GRAPH ?g {
    ?resource a jb:Book .
    ?resource a ?type .
  }
}
GROUP BY ?resource
```

**What it proves**: Books should be typed as both `jb:Book` and `schema:Book`. Blog posts as both `jb:BlogPost` and `schema:BlogPosting`. Missing dual types means schema.org interop is broken.

**Pass criteria**: Every resource has both `jb:` type and corresponding `schema:` type.

### 4.3 — Timestamp Sanity

```sparql
PREFIX jb: <https://jeffbridwell.com/ontology#>
PREFIX schema: <https://schema.org/>
PREFIX dcterms: <http://purl.org/dc/terms/>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>

SELECT ?title ?added
WHERE {
  GRAPH ?g {
    ?resource a jb:{CLASS} ;
              dcterms:title ?title ;
              jb:addedAt ?added .
    FILTER(?added > NOW() || ?added < "2020-01-01T00:00:00Z"^^xsd:dateTime)
  }
}
```

**What it proves**: Timestamps are within expected range. Future dates or dates before the project existed signal bad data or timezone issues.

**Pass criteria**: Zero results.

---

## Running the Tests

### Manual (curl)

```bash
curl -s -H "Accept: application/sparql-results+json" \
  --data-urlencode "query=<SPARQL_QUERY>" \
  http://localhost:3031/pods/sparql
```

### Automated (script pattern)

```bash
#!/bin/bash
# fitness-test.sh — run all checks, report pass/fail

FUSEKI="http://localhost:3031/pods/sparql"
PASS=0
FAIL=0

run_check() {
  local name="$1"
  local query="$2"
  local expected="$3"  # "zero" or "nonzero" or a number

  result=$(curl -s -H "Accept: application/sparql-results+json" \
    --data-urlencode "query=$query" "$FUSEKI" | \
    python3 -c "import sys,json; print(len(json.load(sys.stdin)['results']['bindings']))")

  case "$expected" in
    zero)
      if [ "$result" -eq 0 ]; then
        echo "PASS: $name (0 results)"
        ((PASS++))
      else
        echo "FAIL: $name ($result results, expected 0)"
        ((FAIL++))
      fi
      ;;
    nonzero)
      if [ "$result" -gt 0 ]; then
        echo "PASS: $name ($result results)"
        ((PASS++))
      else
        echo "FAIL: $name (0 results, expected >0)"
        ((FAIL++))
      fi
      ;;
    *)
      if [ "$result" -eq "$expected" ]; then
        echo "PASS: $name ($result results)"
        ((PASS++))
      else
        echo "FAIL: $name ($result results, expected $expected)"
        ((FAIL++))
      fi
      ;;
  esac
}

# Add checks here using run_check "name" "query" "expected"
# ...

echo ""
echo "Results: $PASS passed, $FAIL failed"
exit $FAIL
```

### Dashboard integration

These queries can be wired into the admin dashboard SPARQL tool (current) or a dedicated fitness test page (future). Each query returns structured JSON — straightforward to render as a scorecard.

### CI integration

Run `fitness-test.sh` after Fuseki sync in the test pipeline. Catches ingestion regressions before they ship.

---

## Current Baseline (2026-02-13)

| Collection | Count | Pipeline | Schema | Notes |
|------------|-------|----------|--------|-------|
| Blog Posts | 41 | OK | 100% required props | 3 posts have tags, most don't |
| Books | 19 | OK | 4 books missing optional props | Cat Who Taught Zen most sparse |
| Ideas | 4 | OK | 100% required props | 1 has cross-collection link (promotedTo) |
| Projects | 1 | OK | Minimal properties | |
| Property | 1 Property + 12 Rooms + 6 Gardens + 9 Beds + 1 House + 1 Land | OK | TBD — need property-specific checks | |

## Extending to New Collections

When a new collection type is added (via harvest pipeline or manual creation):

1. Add the class to the Layer 1 type count query — verify resources appear
2. Define required/expected/optional properties in the Layer 2 config table
3. Add collection-specific completeness and gap queries
4. Run the full suite — establish baseline
5. Add to CI
