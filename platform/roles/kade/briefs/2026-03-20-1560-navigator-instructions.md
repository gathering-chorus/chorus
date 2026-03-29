# Navigator Instructions: #1560 Implementation Contract Section

**From:** Silas (navigator) | **To:** Kade (driver) | **Date:** 2026-03-20
**Card:** #1560 | **Pipeline:** 1 of 4

## What We're Building

Add Implementation Contract properties to each domain ICD instance. These capture the full operational plumbing: harvester, source path, Fuseki graph, API endpoints, page routes, validation gate, sync script. Direct properties on the domain instance (not a provider section) — SPARQL-queryable for #1553 topology.

## Step 1: TTL — Add Properties to Domain Instances

Add these `icd:` prefixed properties to each `<icd/domain/*>` instance in the 4 TTL files.

### Notes (`icd-instance-notes.ttl`)

```turtle
<icd/domain/notes> a icd:Domain ;
    # ... existing properties ...
    icd:harvesterScript "scripts/harvest-notes-extract.sh" ;
    icd:harvesterTransform "scripts/harvest-notes-transform.sh" ;
    icd:sourcePath "/data/harvest/notes/pending-harvest.jsonl" ;
    icd:podPath "/data/pods/jeff/notes/" ;
    icd:fusekiGraph "urn:jb:notes/" ;
    icd:apiEndpoint "/api/notes/harvest", "/api/notes/harvest/status", "/api/notes/harvest/runs" ;
    icd:pageRoute "/notes", "/notes/:slug" ;
    icd:adminRoute "/admin/harvest/notes" ;
    icd:handlerClass "NotesHandler" ;
    icd:serviceClass "NotesPodService", "NotesHarvesterService" ;
    icd:syncCommand "scripts/harvest-sync-fuseki.sh notes/items --update-manifest notes" ;
    icd:manifestPath "data/harvest/manifests/notes.json" .
```

### Stories (`icd-instance-stories.ttl`)

```turtle
<icd/domain/stories> a icd:Domain ;
    # ... existing properties ...
    icd:harvesterScript "manual" ;
    icd:sourcePath "chorus-sessions" ;
    icd:podPath "/data/pods/jeff/stories/" ;
    icd:fusekiGraph "urn:jb:stories/" ;
    icd:apiEndpoint "/api/stories" ;
    icd:pageRoute "/stories", "/stories/:slug", "/stories/create" ;
    icd:handlerClass "StoriesHandler" ;
    icd:serviceClass "StoriesPodService", "StoryHarvesterService" ;
    icd:validationGate "icd-validation.ts#STORIES_ICD_SCHEMA" ;
    icd:syncCommand "scripts/harvest-sync-fuseki.sh stories --update-manifest stories" ;
    icd:manifestPath "data/harvest/manifests/stories.json" .
```

### Photos (`icd-instance-photos.ttl`)

```turtle
<icd/domain/photos> a icd:Domain ;
    # ... existing properties ...
    icd:harvesterScript "scripts/batch-photo-harvest.sh" ;
    icd:harvesterTransform "scripts/harvest-apple-photos.js" ;
    icd:sourcePath "~/Pictures/Photos Library.photoslibrary" ;
    icd:podPath "/data/pods/jeff/photos/" ;
    icd:fusekiGraph "urn:jb:photos/" ;
    icd:apiEndpoint "/api/photos", "/api/photos/albums", "/api/photos/derivative/:uuid", "/api/photos/fullres/:filename", "/api/photos/faces", "/api/photos/harvest", "/api/photos/reconcile" ;
    icd:pageRoute "/photos", "/photos/album/:slug", "/photos/faces" ;
    icd:adminRoute "/admin/harvest/photos" ;
    icd:handlerClass "PhotoHandler" ;
    icd:serviceClass "PhotoPodService", "PhotoHarvesterService", "PhotoSqliteService", "CrossSourceReconcilerService" ;
    icd:validationGate "icd-validation.ts#PHOTOS_ICD_SCHEMA" ;
    icd:syncCommand "scripts/harvest-sync-fuseki.sh photos --update-manifest photos" ;
    icd:manifestPath "data/harvest/manifests/photos.json" .
```

### People (`icd-instance-people.ttl`)

```turtle
<icd/domain/people> a icd:Domain ;
    # ... existing properties ...
    icd:harvesterScript "scripts/harvest-people.sh" ;
    icd:sourcePath "/tmp/linkedin-export/Connections.csv" ;
    icd:podPath "/data/pods/jeff/people/" ;
    icd:fusekiGraph "urn:jb:people/" ;
    icd:apiEndpoint "/people" ;
    icd:pageRoute "/people", "/people/:slug" ;
    icd:handlerClass "SelfDomainHandler" ;
    icd:serviceClass "PeoplePodService", "PeopleQueryService" ;
    icd:syncCommand "scripts/harvest-sync-fuseki.sh people --update-manifest people" ;
    icd:manifestPath "data/harvest/manifests/people.json" .
```

**Important:** Multi-value literals (like multiple `apiEndpoint` values) — use repeated properties, not comma-separated strings:
```turtle
    icd:apiEndpoint "/api/notes/harvest" ;
    icd:apiEndpoint "/api/notes/harvest/status" ;
```

## Step 2: ICD Service — Read Implementation Metadata

In `src/services/icd.service.ts`, add a method `getDomainImplementation(domainId: string)` that runs a SPARQL query against `gathering:icd/current` to fetch all `icd:harvesterScript`, `icd:fusekiGraph`, `icd:apiEndpoint`, etc. properties for a domain.

Return shape:
```typescript
interface IcdImplementation {
  harvesterScript: string[];
  harvesterTransform?: string[];
  sourcePath: string[];
  podPath: string;
  fusekiGraph: string;
  apiEndpoint: string[];
  pageRoute: string[];
  adminRoute?: string[];
  handlerClass: string;
  serviceClass: string[];
  validationGate?: string;
  syncCommand: string;
  manifestPath: string;
}
```

## Step 3: ICD Handler — Expose in Domain Response

In `src/handlers/icd.handler.ts`, extend the `GET /api/icd/domains/:id` response to include an `implementation` key with the data from Step 2.

## Step 4: SEMANTIC_MAPPER.html — Render Section

Add an "Implementation Contract" section to each domain accordion. Table layout (like Non-Functionals), with rows for each property. Section type: `"implementation-contract"`. Render after the existing provider/consumer sections.

## Step 5: SPARQL Verification Query

Write a test query that retrieves all implementation metadata across all domains — this proves #1553 topology can be generated from ICDs:

```sparql
PREFIX icd: <urn:gathering:icd#>
SELECT ?domain ?prop ?value
WHERE {
  GRAPH <gathering:icd/current> {
    ?domain a icd:Domain ;
      ?prop ?value .
    FILTER(STRSTARTS(STR(?prop), "urn:gathering:icd#harvester") ||
           STRSTARTS(STR(?prop), "urn:gathering:icd#source") ||
           STRSTARTS(STR(?prop), "urn:gathering:icd#fuseki") ||
           STRSTARTS(STR(?prop), "urn:gathering:icd#api") ||
           STRSTARTS(STR(?prop), "urn:gathering:icd#page") ||
           STRSTARTS(STR(?prop), "urn:gathering:icd#handler") ||
           STRSTARTS(STR(?prop), "urn:gathering:icd#service") ||
           STRSTARTS(STR(?prop), "urn:gathering:icd#sync") ||
           STRSTARTS(STR(?prop), "urn:gathering:icd#manifest") ||
           STRSTARTS(STR(?prop), "urn:gathering:icd#validation") ||
           STRSTARTS(STR(?prop), "urn:gathering:icd#pod") ||
           STRSTARTS(STR(?prop), "urn:gathering:icd#admin"))
  }
}
ORDER BY ?domain ?prop
```

## AC Checklist

1. Implementation Contract properties on domain instances in TTL (4 files)
2. ICD service reads them via SPARQL
3. ICD handler returns them in `/api/icd/domains/:id`
4. SEMANTIC_MAPPER renders the section
5. SPARQL topology query works
6. Tests pass, lint clean
