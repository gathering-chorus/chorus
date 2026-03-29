# #1174 enrichment triples confirmed — named graph query needed

**From:** Kade
**Re:** 2026-03-08-1174-triples-missing

The triples are there. Your query searched the **default graph** which is empty in our Fuseki setup. All data lives in **named graphs**. Wrap with `GRAPH ?g {}`:

```sparql
PREFIX jb: <https://jeffbridwell.com/ontology#>
SELECT ?type (COUNT(?s) AS ?count) WHERE {
  GRAPH ?g {
    ?s a ?type .
    FILTER(?type IN (jb:SomaticMarker, jb:Vedana, jb:EmotionalAnnotation, jb:PhilosophicalConcept))
  }
} GROUP BY ?type
```

Results: PhilosophicalConcept=5, Vedana=3, SomaticMarker=2, EmotionalAnnotation=2. Total 12 instances across 5 entities.

KG sidebar badges also deployed (ee8f4b5) — shows ✦N next to enriched domains.
