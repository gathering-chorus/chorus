# Ontology Status

Last updated: 2026-03-22

## Current Version: v1.5.2

**v1.5.2** (2026-03-22): Sexuality graph restructure (#1584). Split 30+ volume-based graphs into 5 type-specific named graphs: `sexuality/video/` (94K), `sexuality/image/` (1.7M), `sexuality/archive/` (40K), `sexuality/metadata/` (770 sidecars), `sexuality/models.ttl` (22.6K, unchanged). Purged 197.5K misplaced music files (m4a/mp3) confirmed as duplicates via cross-graph filename matching against music domain. Entity URIs preserved. Total sexuality: 1.86M items in clean type-specific graphs.

## Previous Version: v1.5.1

**v1.5.1** (2026-03-21): OWL reasoning spike (#1582). owlrl (OWL2 RL) runs against ontology + People + Stories (38K triples → 76K inferred). Three property characteristics tested: jb:knows symmetric, jb:partOf transitive, jb:dateOfBirth functional. Property chain axiom: jb:connectedToValue = jb:mentionedIn ∘ jb:hasTheme. Scale assessment: ~83 min projected for 23M triples (batch-practical). Reasoning score: 0/10 → testable. Cross-domain links needed before chain axioms produce inferences.

## Previous Version: v1.5.0

**v1.5.0** (2026-03-20): Namespace migration — four-namespace URN scheme. Graph URIs: `urn:jb:` (instance), `urn:gathering:` (product schema/ICD), `urn:chorus:` (team coordination), `urn:borg:` (observation). ICD graphs migrated from `https://jeffbridwell.com/icd/*` to `urn:gathering:icd/*`. Entity/class URIs inside triples still use old namespace pending bulk rewrite (#1557). All pod services read TTL prefixes from `config/namespaces.json`. Namespace enforcement via harvest-sync gate + sparql-guard hook. Cards #1540, #1555.

## Previous Version: v1.4.0

**v1.4.0** (2026-03-19): ICD as RDF ontology — Athena pattern. ICD domain classes (Domain, Provider, ConsumerType, Section, Paragraph, RiskItem, FieldMapping, CanonicalField, Diagram) with full tree structure. 7 domains migrated to RDF via migrate-icd-tree.py. Fidelity gate (--verify) validates HTML source vs graph. Canonical @base URI standardized. Cards #1527-#1545.

## Previous Version: v1.3.0

**v1.3.0** (2026-03-01): Values, Practices, People domains — three Self domain additions. Value (with cross-tradition mappings: Sanskrit, Buddhist, Taoist, Yogic + design test), ValueCollection, Practice, PracticeCollection, PracticeCadence enum (Daily/Weekly/Monthly/Quarterly/Annual/Enduring), Person (extends foaf:Person), PersonCollection, RelationshipType enum (Family/Friend/Mentor/Colleague/TeamMember/Companion). 13 new classes, 9 properties, 2 enumerations. 29 pod instance TTL files (10 values, 12 practices, 7 people). Cross-domain link: Practice → relatedValue → Value. People default jb:Private visibility. Card #591.

**v1.2.0** (2026-02-25): Home Cloud domain — Machine, PhysicalDrive, Volume, ManagedService, NetworkDevice, HomeCloudCollection. 6 new classes, 8 object properties (hasVolume, onMachine, physicalDrive, hasPartition, connectedTo, runsOn, dependsOn, readsFrom), 18 datatype properties (ipAddress, macAddress, hostname, chipModel, ramGB, diskIdentifier, interface, driveCapacityTB, filesystem, mountPoint, capacityGB, usedGB, freeGB, contentRange, servicePort, serviceType, launchdLabel, healthEndpoint, deviceType, manufacturer, deviceModel). Pod data: 8 TTL files covering 2 machines, 18 physical drives, 27 volumes, 25 services, 22 network devices. Card #357.

**v1.1.0** (2026-02-25): Added `jb:mediaUrl` (domain: MediaItem, range: xsd:anyURI) — HTTP URL linking RDF triples back to serveable media binaries on images-api. Enables collection pages to render actual images/videos from Fuseki data. Card #375. Total: 70 classes, 54 object properties, 117 datatype properties (241 declarations).

**v1.0.0** (2026-02-23): Sexuality domain — MediaItem (parent), Video, MediaPhoto, MediaArchive, Model (subClassOf foaf:Person), ContentSource, SexualityCollection. 7 new classes, 4 object properties (hasContent, featuresModel, fromSource, hasProfileImage), 12 datatype properties (filePath, fileSize, sourceVolume, mediaKind, sceneType, contentQuality, finderLabel, sourceName, modelName, modelChecksum, whereFrom, downloadedWith). Driven by images-api harvest architecture validation (card #224). Collection-graph persistence model validated at 1.85M items. Total: 70 classes, 54 object properties, 116 datatype properties (240 declarations).

**v0.9.0** (2026-02-22): Ontology sync sprint — 6 missing domains added to match app reality. Photos domain (Photo, PhotoAlbum, PhotoLocation, FaceDetection, 20+ properties). Notes domain (Note, noteFolder, noteSource). Five list domains (WatchlistItem, ReadingListItem, CookingListItem, TodoListItem, SocialPost). 7 new collection subclasses. Missing rdfs:domain added to hasBookcase, hasShelf, byArtist, duration, year, coverArt. Total: 63 classes, 50 object properties, 104 datatype properties (217 declarations, up from 159 in v0.7.0).

**v0.7.0** (2026-02-16): Music domain — Album, Track, Artist, Genre, MusicCollection, HarvestSource, HarvestRun. First real harvester ontology. Multi-source support with composite dedup key. Data provenance as first-class entities. Properties: hasAlbum, hasTrack, byArtist, albumArtist, playCount, skipCount, genre, releaseYear, discNumber, trackNumber, duration, artwork. Artist normalization rules defined.

**v0.6.0** (2026-02-15): Glimmer domain — Glimmer class (bidirectional with Idea: ignitedTo/sparkedFrom), GlimmerCollection, GlimmerStatus (Glowing/Ignited/Faded with reignition). Triage flow extended to route captures to glimmers.

**v0.5.1** (2026-02-14): Added v2 capture properties — `jb:capturedBy` (sender attribution for multi-sender support) and `jb:linkTitle` (fetched page title for link captures).

**v0.5.0** (2026-02-14): Added Capture Channel domain — CaptureItem class, CaptureCollection, status/type enumerations, routing provenance, media support. First manual intake channel (SMS). Supports future channels (email, web clip, voice, storefront).

**v0.4.0** (2026-02-13): Location model bridge (Book → Shelf → Bookcase → Room), flat location properties deprecated.

### Domains

| Domain | Key Classes | Status |
|--------|------------|--------|
| Property | Property, House, Room, Garden, GardenBed, Plant | Active |
| Books | Book (with location tracking) | Active |
| Blog | BlogPost, HarvestedSource, WordPressSource | Active |
| Gallery | ImageCollection, MovieCollection (stubs — see Sexuality) | Active |
| Sexuality | SexualityCollection, Model, MediaItem, Video, MediaPhoto, MediaArchive, ContentSource | Active (v1.1.0) |
| Profile | Identity, SOLID ACLs | Active |
| Ideas/Projects | Idea, Project, Collection | Active |
| Visibility | VisibilityLevel (Public/Private/Selective), WAC semantics | Active |
| Capture | CaptureItem, CaptureCollection, CaptureStatus, CaptureType | Active (v0.5.0) |
| Glimmer | Glimmer, GlimmerCollection, GlimmerStatus | Active (v0.6.0) |
| Music | Album, Track, Artist, Genre, MusicCollection, HarvestSource, HarvestRun | Active (v0.7.0) |
| Photos | Photo, PhotoAlbum, PhotoLocation, FaceDetection | Active (v0.9.0) |
| Notes | Note, NoteCollection | Active (v0.9.0) |
| Watchlist | WatchlistItem, WatchlistCollection | Active (v0.9.0) |
| Reading List | ReadingListItem, ReadingListCollection | Active (v0.9.0) |
| Cooking List | CookingListItem, CookingListCollection | Active (v0.9.0) |
| Todo List | TodoListItem, TodoListCollection | Active (v0.9.0) |
| Social Posts | SocialPost, SocialPostCollection | Active (v0.9.0) |
| Home Cloud | Machine, PhysicalDrive, Volume, ManagedService, NetworkDevice, HomeCloudCollection | Active (v1.2.0) |
| Values | Value, ValueCollection | Active (v1.3.0) |
| Practices | Practice, PracticeCollection, PracticeCadence | Active (v1.3.0) |
| People | Person, PersonCollection, RelationshipType | Active (v1.3.0) |

### Relationships

**Within-collection (safe for visibility):**
- Property → onProperty → House → inHouse → Room
- Property → onProperty → Garden → inGarden → GardenBed → inGardenBed → Plant
- Blog → hasCategory/hasTag → skos:Concept (taxonomy)
- Idea → mergedInto → Idea
- Album → hasTrack → Track, Album → byArtist → Artist
- Track → byArtist → Artist (may differ from album artist for compilations)
- Glimmer → ignitedTo → Idea / Idea → sparkedFrom → Glimmer

**Cross-collection (visibility boundary crossings):**
- Book → onShelf → Shelf → inBookcase → Bookcase → inRoom → Room (books → property)
- Idea → promotedTo → Project / Project → promotedFrom → Idea (ideas ↔ projects)
- CaptureItem → routedTo → any resource (capture → destination collection)
- Profile → hasCollection → Collection (profile → all collections)
- Any resource → relatedTo / mentions → any resource (generic cross-domain)
- HarvestRun → harvested albums/tracks with provenance chain back to source
- Practice → relatedValue → Value (practices express values — cross-domain query: "which practices express Balance?")

**Visibility note:** Cross-collection relationships are a structural concern for the visibility enforcement model. SPARQL queries that follow these relationships can traverse from a public collection into a private one. See ADR-003 section 7 for the phased approach: collection-scoped queries (Phase 1), opaque URIs (Phase 2), visibility-aware graph scoping (Phase 3/AI).

### Evolution Notes
- Ontology needs to grow as new domains are added
- Architect should review ontology impact for any new feature
- The ontology is the conceptual backbone — changes ripple to SPARQL queries, UI, and AI context

### Planned Domains
- Project management metadata (via GitHub Projects → RDF bridge)
- Cultivating domain (garden lifecycle: seasons, plant stages, harvest records — verbs not just nouns)

### Unified Ontology: Chorus — Shared Awareness v0.2.0 (C#40)
- **File**: `architect/ontology/chorus.ttl`
- **Namespace**: `https://jeffbridwell.com/chorus#`
- **Viz data**: `architect/ontology/chorus-system.json` (drives /chorus/system connections)
- **Merges**: former building.ttl (team protocol) + chorus.ttl v0.1.0 (pipeline execution)
- **10 sections**: Roles, Vertebrae (5-stage spine), Tools (7 instances), Products, Artifacts, Handoffs (C#43), Gates, Workflows, Trust+Fitness, Interactions
- **NEW entities**: Tool (sense/interaction/memory/orchestration/analysis), Vertebra (Capturing added), Product (Gathering feedback loop), Handoff (sent/received/stale)
- **5 vertebrae**: Capturing → Directing → Designing → Building → Proving
- **7 tools**: /look, /listen, /talk, /clearing, /chorus, /werk, Borg
- **4 gates**: DirectionGate, DesignGate, BuildGate, ProvingGate
- **4 trust metrics**: GatePassRate, BounceRate, OverrideRate, HandoffReceiptRate (new)
- **Key relationships**: `operates-at` (Role→Vertebra), `feeds` (Tool→Vertebra), `indexes` (Tool→memory), `outputOf`/`inputTo` (Product↔Vertebra feedback loop)
- **Prior art**: US Patent 9,552,400 B2 (Bridwell) — RDF/OWL + SPARQL + workflow gates

### DEPRECATED: Building Ontology
- **File**: `architect/ontology/building.ttl` (preserved for reference)
- **Deprecated**: 2026-02-22 — merged into chorus.ttl v0.2.0
- **Reason**: Both modeled "how the team operates" — artificial split. Jeff directed the merge.

### SHACL Status

- **Current shapes** (`jb-ontology-shapes.ttl`, 96 lines): Meta-level ontology quality — ensures classes have labels/comments, properties have labels/comments/ranges, ontology has version info.
- **Instance-level shapes planned** (ADR-010): Per-domain shape files in `architect/shapes/` — `photo-shapes.ttl`, `music-shapes.ttl`, `book-shapes.ttl`, `glimmer-shapes.ttl`, `capture-shapes.ttl`. Three severity levels: Violation (reject record), Warning (accept, track gap), Info (completeness metric).
- **Constraint**: Shapes must be collection-scoped. A Photo shape validates photo properties, not cross-collection references. Cross-collection references are optional annotations, not structural requirements (ADR-003).
- ~~Should SHACL instance shapes be created per-collection?~~ **Yes** — ADR-010 formalizes this. Per-domain shape files act as data quality contracts for the harvest pipeline.

### Open Questions
- ~~How should the ontology handle cross-domain relationships?~~ Answered in ADR-003 section 7.
- Versioning strategy as ontology evolves — migration path for existing pod data?
- ~~Should SHACL instance shapes be created per-collection to align with the visibility model?~~ **Yes** — ADR-010.
