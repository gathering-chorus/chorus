# Brief: Sexuality Collection Page — RDF-Backed Model Gallery (#377)

**From:** Silas (Architect) → **To:** Kade (Engineer)
**Date:** 2026-02-25
**Card:** #377 — Sexuality collection page — RDF-backed model gallery with content drill-in

## Context

The full media harvest is complete (#376). Fuseki now holds **13.07M triples** across 26 volume-sharded collection graphs + 1 models graph. This is the data layer for the sexuality collection page — analogous to Music's collection page but for models→content.

## What's in Fuseki

| Graph | Items | Triples |
|-------|-------|---------|
| `.../media/models` | 22,735 models | 135,925 |
| `.../media/VideosNew` | 1,718,466 items | 12,029,190 |
| `.../media/VideosRilez-Ta` | 13,465 items | 94,250 |
| ... (24 more volumes) | ... | ... |
| **Total** | **1,870,839** | **13,072,520** |

## Ontology (v1.1.0)

Key classes and properties for this card:

```turtle
jb:Model (subClassOf foaf:Person)
    jb:modelName "dewi-b"
    jb:modelChecksum "5dbe90bc..."
    jb:filePath "/Volumes/VideosNew/Models/dewi-b.jpg"
    jb:photoFilename "dewi-b.jpg"
    jb:fileSize 39310

jb:MediaPhoto / jb:Video / jb:MediaArchive (subClassOf jb:MediaItem)
    jb:photoFilename "filename.jpg"
    jb:filePath "/Volumes/VideosNew/model-name/filename.jpg"
    dc:format "image/jpeg"
    jb:fileSize 168438435
    dc:created "2024-02-13T23:33:26Z"^^xsd:dateTime
    jb:sourceVolume "VideosNew"
```

## SPARQL Queries You'll Need

### 1. List all models (for gallery grid)

```sparql
PREFIX jb: <https://jeffbridwell.com/ontology#>
SELECT ?model ?name ?filename ?path WHERE {
  GRAPH <https://jeffbridwell.com/pods/jeff/media/models> {
    ?model a jb:Model ;
           jb:modelName ?name ;
           jb:photoFilename ?filename ;
           jb:filePath ?path .
  }
}
ORDER BY ?name
```

Returns 22,735 rows. You'll want pagination (LIMIT/OFFSET) and a letter filter (FILTER(STRSTARTS(?name, "a"))).

### 2. Content for a model (drill-in)

The link between models and content is the directory name in the file path. A model named "dewi-b" has content at paths like `/Volumes/*/dewi-b/*`:

```sparql
PREFIX jb: <https://jeffbridwell.com/ontology#>
PREFIX dc: <http://purl.org/dc/terms/>
SELECT ?item ?type ?filename ?format ?size ?created WHERE {
  GRAPH ?g {
    ?item a ?type ;
          jb:photoFilename ?filename ;
          jb:filePath ?path ;
          dc:format ?format .
    OPTIONAL { ?item jb:fileSize ?size }
    OPTIONAL { ?item dc:created ?created }
    FILTER(CONTAINS(?path, "/dewi-b/"))
    FILTER(?type IN (jb:MediaPhoto, jb:Video, jb:MediaArchive))
  }
  FILTER(STRSTARTS(STR(?g), "https://jeffbridwell.com/pods/jeff/media/"))
}
ORDER BY ?filename
```

### 3. Collection stats (for summary card)

```sparql
PREFIX jb: <https://jeffbridwell.com/ontology#>
SELECT
  (COUNT(DISTINCT ?photo) as ?photos)
  (COUNT(DISTINCT ?video) as ?videos)
  (COUNT(DISTINCT ?archive) as ?archives)
WHERE {
  GRAPH ?g {
    { ?photo a jb:MediaPhoto }
    UNION { ?video a jb:Video }
    UNION { ?archive a jb:MediaArchive }
  }
  FILTER(STRSTARTS(STR(?g), "https://jeffbridwell.com/pods/jeff/media/"))
}
```

## Profile Image Serving

Model profile images are at paths like `/Volumes/VideosNew/Models/dewi-b.jpg` on the secondary Mac (192.168.86.242). When images-api is running, these are served at:

```
http://192.168.86.242:8082/api/images/{model-name}
```

The existing gallery proxy in the app (`/gallery/proxy/image/:name`) already handles this. So for the collection page, model thumbnails can use the same proxy path the current gallery uses.

## Page Structure (Suggested)

Like the Music collection page pattern:

1. **Gallery grid** — alphabetical model cards with profile image thumbnails, letter filter (A-Z)
2. **Model detail** — click a model → see all their content (photos, videos, archives) with type badges and file sizes
3. **Stats header** — total models, photos, videos, archives (from SPARQL)
4. **Search** — filter models by name (client-side on the paginated set, or SPARQL FILTER with CONTAINS)

## Route Suggestion

```
/collections/sexuality           → model gallery grid
/collections/sexuality/:name     → model detail with content list
```

## Existing Patterns to Follow

- `collection-gallery.ejs` + `gallery.handler.ts` — current gallery (HTTP proxy based)
- `music-collection.ejs` + `music.handler.ts` — RDF-backed collection with SPARQL queries
- `gallery.service.ts` → for the image proxy (reuse for profile image serving)

## Dependencies

- **#375 DONE** — `jb:mediaUrl` property added to ontology v1.1.0
- **#376 DONE** — Full harvest: 1.85M items in Fuseki
- Images-api (192.168.86.242:8082) needed for profile image serving — when it's down, show placeholder thumbnails

## What's Out of Scope

- Model→content RDF relationships (jb:hasContent) — linking via path CONTAINS for now
- Video streaming from collection page — future card
- Incremental harvest updates — separate ops concern
