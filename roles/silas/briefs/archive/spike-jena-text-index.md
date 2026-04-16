# Spike: Apache Jena Text Index for SPARQL Full-Text Search

**Author:** Silas (Architect)
**Date:** 2026-02-24
**Status:** Complete
**Scope:** Feasibility of Jena Text (Lucene-backed) for notes search in Fuseki

---

## 1. How Jena Text Index Works

Apache Jena Text integrates Lucene (or Solr/Elasticsearch) directly into the SPARQL query engine. The core idea:

- **Triples containing indexed properties get mirrored into a Lucene index** alongside the TDB store.
- Queries use the `text:query` magic predicate to search Lucene, returning subject URIs.
- The SPARQL optimizer can then join those URIs with the rest of the graph pattern.
- **Auto-updating**: When data is modified via SPARQL Update, the Lucene index updates automatically. No external reindex needed for ongoing mutations.

Architecture: `TDB2 dataset` --> wrapped by `TextDataset` --> Lucene index on disk alongside TDB.

The module ships inside the standard Fuseki distribution (`fuseki-server.jar` is a fat JAR). **No additional JARs needed** -- our Fuseki 5.1.0 image already has it.

## 2. Fuseki Assembler Configuration

The current setup creates the `pods` dataset via the Fuseki admin API (`fuseki-init.sh` POSTs `dbType=tdb2`). To enable text indexing, the dataset must be defined via an **assembler configuration file** instead.

### Realistic config for our notes properties

```turtle
@prefix :        <#> .
@prefix fuseki:  <http://jena.apache.org/fuseki#> .
@prefix rdf:     <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs:    <http://www.w3.org/2000/01/rdf-schema#> .
@prefix tdb2:    <http://jena.apache.org/2016/tdb#> .
@prefix text:    <http://jena.apache.org/text#> .
@prefix dcterms: <http://purl.org/dc/terms/> .
@prefix jb:      <https://jeffbridwell.com/ontology#> .

## --- Fuseki service definition ---
:service rdf:type fuseki:Service ;
    fuseki:name "pods" ;
    fuseki:endpoint [ fuseki:operation fuseki:query ; fuseki:name "sparql" ] ;
    fuseki:endpoint [ fuseki:operation fuseki:query ; fuseki:name "query" ] ;
    fuseki:endpoint [ fuseki:operation fuseki:update ; fuseki:name "update" ] ;
    fuseki:endpoint [ fuseki:operation fuseki:gsp-rw ; fuseki:name "data" ] ;
    fuseki:dataset :text_dataset ;
    .

## --- Text dataset wraps the base TDB2 dataset ---
:text_dataset rdf:type text:TextDataset ;
    text:dataset :tdb_dataset ;
    text:index :indexLucene ;
    .

## --- Base TDB2 dataset ---
:tdb_dataset rdf:type tdb2:DatasetTDB2 ;
    tdb2:location "/fuseki/databases/pods" ;
    tdb2:unionDefaultGraph true ;
    .

## --- Lucene index configuration ---
:indexLucene rdf:type text:TextIndexLucene ;
    text:directory "/fuseki/databases/pods-text-index" ;
    text:entityMap :entMap ;
    text:storeValues true ;
    text:analyzer [ rdf:type text:StandardAnalyzer ] ;
    .

## --- Entity map: which properties to index ---
:entMap rdf:type text:EntityMap ;
    text:defaultField "body" ;
    text:entityField "uri" ;
    text:uidField "uid" ;
    text:langField "lang" ;
    text:graphField "graph" ;
    text:map (
        [ text:field "body" ;    text:predicate jb:body ]
        [ text:field "title" ;   text:predicate dcterms:title ]
        [ text:field "label" ;   text:predicate rdfs:label ]
        [ text:field "notes" ;   text:predicate jb:notes ]
    ) .
```

Key choices:
- `text:storeValues true` -- stores the original literal in Lucene so `?literal` output bindings work without a second TDB lookup.
- `text:defaultField "body"` -- bare `text:query` calls search the body field (most common use case).
- `text:graphField "graph"` -- enables graph-scoped text search (each note lives in its own named graph).
- StandardAnalyzer -- good default for English prose; does lowercasing + stop word removal.

## 3. Docker Integration Path

### Current state
- Image: `stain/jena-fuseki:latest` (Fuseki 5.1.0), extended in `docker/Dockerfile.fuseki`
- Dataset created at runtime via `fuseki-init.sh` (POST to admin API)
- Data lives in named volume `jeff-bridwell-personal-site-fuseki-data` mounted at `/fuseki`

### Lightest path: volume-mount the assembler config

**Option A (recommended): Bind-mount a config file into `/fuseki/configuration/`**

Fuseki auto-loads any `.ttl` file in `/fuseki/configuration/` as an assembler description. No image rebuild needed.

```yaml
# docker-compose.yml changes
fuseki:
  volumes:
    - fuseki-data:/fuseki
    - ./docker/fuseki-text-config.ttl:/fuseki/configuration/pods.ttl:ro
```

Then **remove** `FUSEKI_DATASET_1=pods` from the environment (the assembler replaces the API-created dataset). The `fuseki-init.sh` dataset creation logic becomes a no-op check.

**Option B: Bake into Dockerfile**

```dockerfile
COPY docker/fuseki-text-config.ttl /fuseki/configuration/pods.ttl
```

Requires image rebuild but is more immutable. Prefer this for production.

### Migration for existing data

The TDB2 data is already at `/fuseki/databases/pods/` inside the volume. The assembler config points `tdb2:location` at that same path, so **existing data is preserved**. The Lucene index is new and empty -- needs a one-time build.

**One-time index build** (after deploying the config):
```bash
docker exec jeff-bridwell-personal-site-fuseki \
  java -cp /jena-fuseki/fuseki-server.jar \
  jena.textindexer --desc=/fuseki/configuration/pods.ttl
```

After the initial build, the index auto-updates on every SPARQL Update.

## 4. SPARQL Query Migration

### Before (current pattern -- full table scan)
```sparql
SELECT ?s ?title ?body WHERE {
  GRAPH ?g {
    ?s a jb:Note ;
       dcterms:title ?title ;
       jb:body ?body .
  }
  FILTER(CONTAINS(LCASE(?body), "practice"))
}
```
Scans every Note's body literal, applies string comparison. O(n) in number of triples.

### After (text:query -- Lucene lookup)
```sparql
PREFIX text: <http://jena.apache.org/text#>

SELECT ?s ?title ?body ?score WHERE {
  (?s ?score ?lit) text:query (jb:body "practice") .
  GRAPH ?g {
    ?s dcterms:title ?title ;
       jb:body ?body .
  }
}
ORDER BY DESC(?score)
```

### More examples

**Search across all indexed fields (default field = body):**
```sparql
(?s ?score) text:query "meditation zazen"
```

**Phrase search:**
```sparql
(?s ?score) text:query "\"career break\""
```

**Wildcard:**
```sparql
(?s ?score) text:query "gather*"
```

**Search with limit (top 20 results):**
```sparql
(?s ?score) text:query (jb:body "practice" 20)
```

**Multi-field search:**
```sparql
(?s ?score ?lit) text:query (dcterms:title jb:body "heidegger")
```

**Graph-scoped (with graphField configured):**
```sparql
SELECT ?s ?lit WHERE {
  GRAPH ?g {
    (?s ?sc ?lit) text:query "dogen"
  }
}
```

**Boolean operators (Lucene syntax):**
```sparql
(?s ?score) text:query "practice AND enlightenment"
(?s ?score) text:query "zen NOT rinzai"
```

## 5. Performance Characteristics

### Expected improvement for 823 notes

| Metric | FILTER(CONTAINS) | text:query |
|--------|-------------------|------------|
| **Mechanism** | Full scan of all string literals | Lucene inverted index lookup |
| **Time complexity** | O(n) -- reads every literal | O(log n) -- index seek |
| **Expected query time** | 50-200ms (depending on literal size) | 1-5ms |
| **Scales to** | Degrades linearly with data growth | Sublinear; handles millions of docs |

### Index size overhead

- 823 notes, average body ~500 chars = ~400KB text corpus
- Lucene index overhead is typically 20-30% of source text size
- **Expected index size: ~100-200KB** -- negligible
- Even at 10x the current data, we'd be under 2MB

### Rebuild time

- Initial `jena.textindexer` on 823 notes: **under 5 seconds**
- Auto-update latency per SPARQL Update: sub-millisecond (single document index)

## 6. Risks and Gotchas

### Known issues
1. **Index/data desync**: If TDB is restored from backup without the Lucene index, results will be wrong. Fix: re-run `jena.textindexer`. Keep this in the ops runbook.
2. **UID field required for deletes**: Without `text:uidField`, deleting triples does not remove them from the Lucene index. The config above includes it.
3. **Blank nodes**: Text index works with blank nodes but `text:entityField` stores a blank node ID that may not survive TDB compaction. Not an issue for us -- notes use URIs.
4. **Literal language tags**: If notes had `@en` tags, you'd need `text:langField` for proper filtering. Our notes don't use language tags currently, but the field is configured defensively.
5. **StandardAnalyzer stop words**: Queries for common English words ("the", "a", "is") return nothing because they're removed by the analyzer. Use `SimpleAnalyzer` if stop word removal is unwanted.
6. **No partial-word match by default**: "prac" won't match "practice" unless you use wildcard syntax "prac*". This differs from `CONTAINS()` behavior. Users may expect substring matching.
7. **Config file must exist before first Fuseki start** with text dataset. If Fuseki starts and creates a plain `pods` dataset via the API first, the assembler config will conflict. Migration order matters.

### Recovery from index corruption
```bash
# Stop Fuseki
# Delete the Lucene index directory
rm -rf /fuseki/databases/pods-text-index/
# Rebuild from TDB data
java -cp /jena-fuseki/fuseki-server.jar jena.textindexer --desc=/fuseki/configuration/pods.ttl
# Restart Fuseki
```
Data is never lost -- TDB is the source of truth. Lucene index is always rebuildable.

### Version compatibility
- Fuseki 5.1.0 bundles jena-text with Lucene 9.x. No version mismatch risk since it's all in the fat JAR.
- If upgrading Fuseki later, the Lucene index format may need rebuilding (Lucene major versions are not backward-compatible). A `textindexer` re-run handles this.

## 7. Smallest Viable Version

### Minimum to ship

1. **Create** `docker/fuseki-text-config.ttl` (the assembler config above)
2. **Add** bind mount to `docker-compose.yml` (one line)
3. **Remove** `FUSEKI_DATASET_1=pods` environment variable
4. **Redeploy** Fuseki (this creates the dataset from assembler instead of API)
5. **Run** `jena.textindexer` once inside the container to build the initial index
6. **Update** one SPARQL query in the app to use `text:query` as a proof point

### What NOT to do yet
- Don't index every property -- start with `jb:body`, `dcterms:title`, `jb:notes`
- Don't add Elasticsearch/Solr -- Lucene is embedded and sufficient at this scale
- Don't build a search API -- validate with raw SPARQL first
- Don't change the Dockerfile -- bind mount is simpler for iteration

### Estimated effort
- Config + docker-compose change: 30 minutes
- Migration + index build: 15 minutes (including testing)
- First query migration: 15 minutes
- **Total: ~1 hour**

---

## Recommendation

**Do it.** The cost is tiny (one config file, one volume mount, one index build command). The benefit is real -- `text:query` replaces `FILTER(CONTAINS())` with Lucene lookups, giving us sub-5ms search on notes and a foundation that scales to every domain in the knowledge graph. The index auto-updates, so there's no ongoing maintenance burden.

The only structural risk is the migration sequence -- Fuseki must not have a conflicting API-created `pods` dataset when the assembler config loads. Clean migration path: stop Fuseki, deploy config, start Fuseki, run indexer.

Sources:
- [Apache Jena Full Text Search docs](https://jena.apache.org/documentation/query/text-query.html)
- [Fuseki Configuration docs](https://jena.apache.org/documentation/fuseki2/fuseki-configuration.html)
- [stain/jena-docker GitHub](https://github.com/stain/jena-docker/tree/master/jena-fuseki)
- [Minimal Fuseki TDB + text search gist](https://gist.github.com/lawlesst/ba32ad4fcc830b32823a)
- [GitHub discussion: JenaText with FusekiMain](https://github.com/apache/jena/discussions/1950)
