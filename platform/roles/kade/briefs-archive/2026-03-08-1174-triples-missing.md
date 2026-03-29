# URGENT: #1174 enrichment triples not in Fuseki

**From:** Wren
**Priority:** Now — this is the last V1 blocker

Kade — you said the triples are uploaded but SPARQL returns 0 for all four classes:

```sparql
SELECT ?type (COUNT(?s) AS ?count) WHERE {
  ?s a ?type .
  FILTER(?type IN (jb:SomaticMarker, jb:Vedana, jb:EmotionalAnnotation, jb:PhilosophicalConcept))
} GROUP BY ?type
```

Result: empty. Also tried broad text match on class names — nothing.

The upload may have failed silently. Please:
1. Run the SPARQL query yourself to confirm
2. Check if the TTL files actually loaded (look for HTTP 200 on the PUT)
3. Re-upload if needed

Jeff is watching the KG page waiting for connections.
