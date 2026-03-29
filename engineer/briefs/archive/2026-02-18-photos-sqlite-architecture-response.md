# Response: Photos SQLite Pipeline + Multi-Source Person Architecture

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-18
**Re**: Photos SQLite discovery — architecture review and decisions

---

## Summary: All Four Questions Answered

Good discovery work. SQLite is clearly the right extraction path. Here are your answers.

---

## Q1: SQLite Extraction — New Service or Replace JXA?

**Decision: New service (`PhotoSqliteService`).**

Kade's instinct is correct. Clean separation:

```
PhotoSqliteService   → metadata extraction (SQLite, seconds)
PhotoJxaService      → thumbnail export only (JXA, batched overnight)
PhotoHarvesterService → orchestrates both, writes Turtle
```

**Rationale:**
- SQLite schema is Apple's — it can change between macOS versions. Isolating it in one service means schema changes affect one file, not the whole pipeline.
- JXA stays for thumbnails because it's the only reliable way to get actual photo files out of Photos.app. The Photos Library filesystem is opaque.
- The orchestrator (`PhotoHarvesterService`) doesn't care where the data comes from — it receives normalized JSON and writes Turtle. This is the Pattern B abstraction preserved.

**Implementation note:** Read SQLite in read-only mode (`?mode=ro` in the connection string). Never write to Apple's database.

---

## Q2: PhotoPerson vs foaf:Person

**Decision: Use `foaf:Person` as the base class. Drop `jb:PhotoPerson` as a standalone class — make it a type annotation instead.**

```turtle
# A person entity (cross-domain capable)
:jeff-bridwell a foaf:Person ;
    foaf:name "Jeff Bridwell" ;
    jb:personSource <apple-photos>, <google-photos> ;
    jb:appleClusterId "6972" ;
    jb:googlePersonId "Jeff Bridwell" .

# Photo-to-person relationship carries the detection context
:photo-123 jb:depictsPerson :jeff-bridwell .

# Face detection metadata lives on a reified relationship, not the person
:detection-456 a jb:FaceDetection ;
    jb:inPhoto :photo-123 ;
    jb:ofPerson :jeff-bridwell ;
    jb:centerX 0.45 ;
    jb:centerY 0.32 ;
    jb:faceSize 0.15 ;
    jb:quality 0.87 .
```

**Why:**
- Persons span domains. Jeff Bridwell appears in Photos, potentially in Music (as listener/collector), potentially in Books. One entity, multiple type annotations.
- Face detection metadata (coordinates, quality, smile) belongs on the detection event, not the person. A person's face size varies across photos.
- `jb:PhotoPerson` from v0.8.0 can stay as an optional `rdf:type` for queries that only care about photo-related persons: `?person a jb:PhotoPerson` still works if you add that type.

---

## Q3: Cross-Source Person Merging

**Decision: Lazy merge with explicit linking. Don't auto-match.**

```turtle
# Apple Photos creates anonymous person from cluster
:apple-cluster-6972 a foaf:Person ;
    jb:personSource <apple-photos> ;
    jb:appleClusterId "6972" ;
    jb:faceCount 1051 .

# Google Takeout creates named person
:google-jeff-bridwell a foaf:Person ;
    foaf:name "Jeff Bridwell" ;
    jb:personSource <google-photos> .

# When confirmed (manually or by photo overlap):
:apple-cluster-6972 owl:sameAs :google-jeff-bridwell .

# Canonical person (created at merge time):
:jeff-bridwell a foaf:Person ;
    foaf:name "Jeff Bridwell" ;
    jb:mergedFrom :apple-cluster-6972, :google-jeff-bridwell ;
    jb:personSource <apple-photos>, <google-photos> .
```

**Why not auto-match:**
- Apple has 5,621 anonymous clusters. Google has named persons. Matching by face similarity requires ML we don't have.
- Matching by photo overlap (same photo in both libraries) is feasible but fragile (different filenames, different timestamps from timezone issues).
- Wrong merges are worse than no merges — you can always merge later, but splitting a wrongly-merged person corrupts the graph.

**Merge strategy (phased):**
1. **Phase 1 (now):** Import both sources as separate person entities. No merging.
2. **Phase 2 (when Google Takeout is ingested):** Identify overlap candidates by photo filename/date matching. Present to Jeff for confirmation.
3. **Phase 3 (future):** UI for manual person merging ("these two are the same person").

**For now, Kade builds Phase 1 only.** Separate entities per source. The ontology supports merging later without schema changes.

---

## Q4: Person Container Location

**Decision: `photos/people/{slug}.ttl` for now. Cross-domain `persons/` later.**

**Rationale:**
- Persons are currently only in the Photos domain. No Music artist → person linking yet.
- Putting them at `/persons/` top-level creates a container with no clear domain ownership.
- When we add cross-domain person linking (Music artist = Photo subject), we refactor the container. The RDF class (`foaf:Person`) is already correct — it's only the filesystem path that changes.
- Cross-graph SPARQL (ADR-008) doesn't care where the Turtle file lives — it cares about the graph URI.

**URI pattern:**
```
# Named persons (from Google)
/photos/people/jeff-bridwell.ttl

# Anonymous clusters (from Apple, until named)
/photos/people/apple-cluster-6972.ttl
```

---

## Q5: Reverse Geocoding for Locations

**Decision: No external API calls. Cluster by proximity, label manually.**

**Rationale:**
- External geocoding APIs add a dependency and rate-limit risk for 5,856 lookups.
- Kade already identified the top clusters (Boston, St. Louis, Cambridge, etc.) from the data.
- For v1: store raw lat/lng on the photo (already in ontology). Create `PhotoLocation` entities by clustering nearby coordinates (within ~1km). Use human-readable slugs from the obvious clusters.

**Implementation:**
```
# Cluster: group photos within ~0.01 degree (~1km)
# Round lat/lng to 2 decimal places for clustering

/photos/locations/boston-roslindale.ttl    → lat ~42.29, lng ~-71.14
/photos/locations/st-louis.ttl            → lat ~38.62, lng ~-90.21
/photos/locations/cambridge.ttl           → lat ~42.31, lng ~-71.11
/photos/locations/copenhagen.ttl          → lat ~55.69, lng ~12.58
```

Hardcode the top 10-15 cluster names from Kade's table. Everything else gets a slug like `loc-42-29-n-71-14-w`. Jeff can rename them later via UI.

---

## Q6: Thumbnail Approach

**Decision: Keep JXA export for thumbnails. Accept the batch run.**

Kade's batching approach (50 at a time, ~1GB peak disk) is sound. The 4.5-hour run is a one-time cost for initial harvest. Incremental runs only process new photos.

The Photos Library filesystem (`originals/`, `resources/derivatives/`) is opaque and version-dependent — don't try to read it directly. JXA's `Photos.export()` is the supported API.

---

## Ontology Impact: v0.8.0 → v0.8.1 (Additive)

No breaking changes. Add these properties:

```turtle
# Person source tracking
jb:personSource a owl:ObjectProperty ;
    rdfs:domain foaf:Person ;
    rdfs:range jb:HarvestSource ;
    rdfs:comment "Which source(s) identified this person" .

jb:appleClusterId a owl:DatatypeProperty ;
    rdfs:domain foaf:Person ;
    rdfs:range xsd:string ;
    rdfs:comment "Apple Photos anonymous cluster ID" .

jb:googlePersonId a owl:DatatypeProperty ;
    rdfs:domain foaf:Person ;
    rdfs:range xsd:string ;
    rdfs:comment "Google Photos person label" .

jb:mergedFrom a owl:ObjectProperty ;
    rdfs:domain foaf:Person ;
    rdfs:range foaf:Person ;
    rdfs:comment "Source person entities this was merged from" .

# Face detection (reified relationship)
jb:FaceDetection a owl:Class ;
    rdfs:label "Face Detection" ;
    rdfs:comment "A detected face in a specific photo" .

jb:inPhoto a owl:ObjectProperty ;
    rdfs:domain jb:FaceDetection ;
    rdfs:range jb:Photo .

jb:ofPerson a owl:ObjectProperty ;
    rdfs:domain jb:FaceDetection ;
    rdfs:range foaf:Person .

jb:centerX a owl:DatatypeProperty ;
    rdfs:domain jb:FaceDetection ;
    rdfs:range xsd:decimal .

jb:centerY a owl:DatatypeProperty ;
    rdfs:domain jb:FaceDetection ;
    rdfs:range xsd:decimal .

jb:faceSize a owl:DatatypeProperty ;
    rdfs:domain jb:FaceDetection ;
    rdfs:range xsd:decimal .

jb:faceQuality a owl:DatatypeProperty ;
    rdfs:domain jb:FaceDetection ;
    rdfs:range xsd:decimal .

# Depicts relationship (photo → person)
jb:depictsPerson a owl:ObjectProperty ;
    rdfs:domain jb:Photo ;
    rdfs:range foaf:Person ;
    rdfs:comment "A person depicted in this photo (from face detection)" .
```

---

## Build Order for Kade

1. **Now:** `PhotoSqliteService` — read-only metadata extraction (GPS, faces, all fields). Output: normalized JSON matching the existing harvest pipeline input format.
2. **Now:** Wire SQLite service into `PhotoHarvesterService` as the primary extraction path. JXA becomes thumbnail-only.
3. **Now:** Write face detection data to Turtle (using the `jb:FaceDetection` class above). Anonymous persons only — no merging yet.
4. **Now:** Write GPS data to `PhotoLocation` entities using proximity clustering.
5. **Later (separate card):** Google Takeout import service + person merging UI.

**Kade is unblocked on items 1-4.**

---

— Silas
