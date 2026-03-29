# Domain Registry — Operational Topology

**Aligned with:** `product-manager/value-stream-and-domains.md` (value stream), `data/about/PRODUCT_TAXONOMY.md` (product taxonomy)

The product taxonomy defines *what* each domain is and where it sits in the value stream. This registry maps each domain to *how it runs* — harvesters, graphs, pages, metrics, dependencies.

Updated: 2026-03-12

---

## Sowing

### seeds
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Capture | SMS → seed pipeline | Twilio webhook → `src/handlers/capture.handler.ts` |
| Capture | Voice → seed pipeline | `/listen` skill → seed pipeline |
| Capture | Demo seed capture | Gemba observation → board pipeline (Wren-instrumented) |
| Store | Fuseki graph | `pods/jeff/seeds/*` |
| Page | Triage page | `/capture-triage` (auth-gated) |
| Spine events | `seed.received`, `seed.routed`, `seed.discarded` |
| Downstream | Routes to: glimmers, reading, watching, cooking, todo |
| Dependencies | Twilio API, Fuseki |

---

## Growing

### glimmers
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Store | Fuseki graph | `pods/jeff/glimmers/*` |
| Page | Glimmer list | `/glimmers` |
| Ontology | `Glimmer`, `GlimmerState` |
| Downstream | Ignites to → ideas |
| Status | Designed, not fully built |

### ideas
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Store | Fuseki graph | `pods/jeff/ideas/*` |
| Page | Ideas list | `/ideas` |
| Ontology | `Idea` |
| Downstream | Promotes to → projects (lifecycle transition) |
| Upstream | Ignited from → glimmers |
| Status | Built |

### projects
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Store | Fuseki graph | `pods/jeff/projects/*` |
| Page | Projects list | `/projects` |
| Ontology | `Project` |
| Upstream | Promoted from → ideas |
| Also | Team work items live on Vikunja board (Chorus domain, separate from Jeff's personal projects in Fuseki) |
| Status | Built |

---

## Practicing

### cooking
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Store | Fuseki graph | `pods/jeff/cooking/*` |
| Page | Cooking page | `/cooking` |
| Ontology | `CookingItem`, `Recipe` |
| Status | Manual capture |

### reading
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Store | Fuseki graph | `pods/jeff/reading/*` |
| Page | Reading page | `/reading` |
| Ontology | `ReadingItem` |
| Upstream | Routed from seeds |
| Status | Manual + routed |

### watching
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Store | Fuseki graph | `pods/jeff/watching/*` |
| Page | Watching page | `/watching` |
| Ontology | `WatchingItem` |
| Upstream | Routed from seeds |
| Status | Manual + routed |

### todo
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Store | Fuseki graph | `pods/jeff/todo/*` |
| Page | Todo page | `/todo` |
| Ontology | `TodoItem`, `Task` |
| Upstream | Routed from seeds |
| Status | Manual + routed |

### intentions
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Store | Fuseki graph | `pods/jeff/intentions/*` |
| Page | Intentions page | `/intentions` |
| Ontology | `Intention` |
| Harvest status | Manual — no automated harvest |

---

## Harvesting

### music
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Harvester | Apple Music XML parse | `scripts/harvest-music-xml-parse.js` |
| Harvester | Apple Music run | `scripts/harvest-music-run.js` |
| Harvester | Music extract (ffprobe) | `scripts/harvest-music-extract.sh`, `harvest-music-ffprobe.sh` |
| Harvester | Music ID3 parse | `scripts/harvest-music-id3-parse.js` |
| Harvester | Apple Music import | `scripts/music-import-apple.sh` |
| Dedup | `scripts/music-dedup-scan.js`, `music-dedup-execute.js` |
| Crossref | `scripts/music-crossref.sh`, `music-fuzzy-match.py` |
| Store | Fuseki graphs | `pods/jeff/music/*` (albums, artists, tracks) |
| Page | Music collection | `/music`, `/music/:id` |
| Page | Albums list | `/albums` |
| Streaming | Navidrome | `localhost:4533` |
| Ontology | `Album`, `Track`, `Artist`, `Genre` |
| Scale | 40+ years, ~15M triples (largest domain) |
| Source data | `~/Music/Music/Media/Music/`, `/Volumes/Gathering/Music/` |
| Harvest status | Complete — mature pipeline |

### photos
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Harvester | Apple Photos (SQLite) | `scripts/harvest-apple-photos.js` |
| Harvester | Google Photos (Takeout) | `scripts/harvest-google-photos.js` |
| Harvester | Batch pipeline | `scripts/batch-harvest-photos.sh` |
| Harvester | Photo thumbnails | `scripts/generate-google-thumbnails.sh`, `export-photo-thumbnails.js` |
| Store | Fuseki graphs | `pods/jeff/photos/*` |
| Page | Photos page | `/photos` (currently Apple Photos only via SQLite) |
| Ontology | `Photo`, `PhotoAlbum`, `PhotoLocation` |
| Scale | 200K+ photos |
| Source data | `~/Photos/`, `/Volumes/Gathering/Photos/GoogleTakeoutPhotos/` (157 zip files, ~628GB) |
| Known issues | #1350 — dual source (Apple SQLite + Google Fuseki), /photos only shows Apple. #1347 — sync manifest gap. #1351 — batch pipeline in progress |
| Harvest status | Active — Google Photos pipeline being built |

### books
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Store | Fuseki graphs | `pods/jeff/books/*` |
| Page | Books catalog | `/books`, `/books/:id` |
| Ontology | `Book`, `BookLocation` |
| Scale | 141+ |
| Harvest status | Manual + OCR upload |

### stories
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Store | Fuseki graphs | `pods/jeff/stories/*` |
| Page | Stories page | `/stories`, `/stories/:id` |
| Ontology | `Story`, `Narrative` |
| Scale | 135+ |
| Harvest status | Stale (302h) — session capture → TTL |

### notes
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Harvester | Notes extract | `scripts/harvest-apple-notes.js` |
| Harvester | Notes extract (shell) | `scripts/harvest-notes-extract.sh`, `harvest-notes-transform.sh` |
| Store | Fuseki graphs | `pods/jeff/notes/*` |
| Page | Notes page | `/notes` |
| Ontology | `Note`, `NoteFolder` |
| Harvest status | Partial (302h stale) |

### blog
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Harvester | WordPress transform | `scripts/harvest-wordpress-transform.sh` |
| Store | Fuseki graphs | `pods/jeff/blog/*` |
| Page | Blog page | `/blog` |
| Ontology | `BlogPost`, `WordPressSource` |
| Source | WordPress Docker (`../wordpress-blog/`) |
| Harvest status | Stale (289h) |

### social
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Harvester | Social exports | `scripts/harvest-social.sh` |
| Store | Fuseki graphs | `pods/jeff/social/*` |
| Page | Social page | `/social` |
| Ontology | `SocialPost` |
| Harvest status | Static import — one-time GDPR exports from Facebook and LinkedIn, not ongoing harvests |

### documents
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Harvester | Google Docs/Sheets | `scripts/harvest/` directory |
| Store | Fuseki graphs | `pods/jeff/documents/*` |
| Page | Documents page | `/documents` |
| Ontology | `Document` |
| Scale | 560K+ items total, 467 docs harvested so far (24 folders) |
| Harvest status | Active — #1330 shipped today. Pipeline proven, bulk harvest pending. |

### property
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Store | Fuseki graphs | `pods/jeff/property/*` |
| Page | Property pages | `/property`, `/garden` |
| Ontology | `Property`, `House`, `Garden`, `Room` |
| Harvest status | Manual CRUD |

### sexuality
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Harvester | Volume extract | `scripts/harvest-sexuality-extract.sh` |
| Harvester | Volume load | `scripts/harvest-sexuality-load.sh` |
| Store | Fuseki graphs | `pods/jeff/sexuality/*` |
| Ontology | `Model`, `Studio`, `MediaVolume` |
| Page | Gallery/models/studios | `/sexuality` (not in main nav — access-gated) |
| Source data | `/Volumes/Gathering/` (Bedroom Mac) |
| Harvest status | Partial (300h stale) |

### people
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Harvester | People harvest (LinkedIn/Facebook export) | `scripts/harvest-people.sh` |
| Store | Fuseki graphs | `pods/jeff/people/*` |
| Page | People page | `/people`, `/people/:id` |
| Ontology | `Person`, `Contact` |
| Scale | 2,259+ |
| Known issues | #1270 — relationship depth enrichment in WIP. #1269 — Apple Contacts harvest in Next (not built yet). |
| Harvest status | Partial — current harvester is export-based only, no live contacts sync |

---

## Reflecting

### values
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Store | Fuseki graphs | `pods/jeff/values/*` |
| Page | Values page | `/values` |
| Ontology | `Value` |
| Status | Manual |

### practices
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Store | Fuseki graphs | `pods/jeff/practices/*` |
| Page | Practices page | `/practices` |
| Ontology | `Practice` |
| Status | Manual |

### self
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Page | Self/Jeff page | `/jeff` |
| Queries | people, stories, values, practices, photos |
| Upstream | Convergence node — downstream of nearly all Harvesting + Reflecting domains |
| Status | AI-assisted reflection — semantic search + reasoning |

---

## Cross-Domain Infrastructure

### Search
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| FTS index | `scripts/semantic-index-build.js` |
| Page | Search | `/search` |
| API | `/api/search` |
| Spine events | Queries logged to Loki |

### Fuseki (Triplestore)
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Service | Apache Jena Fuseki | `localhost:3030` |
| Dataset | `pods` |
| Query endpoint | `/pods/query` |
| Update endpoint | `/pods/update` |
| Current scale | 15.7M triples, 37,621 graphs |
| Perf baseline | `scripts/perf-baseline.sh` (nightly via LaunchAgent) |
| Perf queries | `scripts/fuseki-perf.sh` (every 4h via LaunchAgent) |

### Sync
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Sync script | `scripts/harvest-sync-fuseki.sh` |
| Known issues | #1347 — `canSkipSync()` bypasses new TTL files |

### Enrichment (ADR-017, not yet built)
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Architecture | ADR-017 — separate fact from inference |
| Graph pattern | `pods/jeff/enrichment/<pass>-<date>` |
| Passes planned | Geo→Place, Date→Event, Person mention, Doc→Topic, Cross-domain |
| Status | Designed, not built. Depends on harvest completion. |

### Codebase Graph
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Builder | `scripts/harvest-codebase.sh`, `codebase-graph-watch.sh` |
| API | `/api/codebase/graph` |
| Page | System graph | `/gathering-chorus-system-graph` |

---

## Harvest Pipeline Status

Source: session-start health check. Updated each session.

| Domain | Status | Staleness | Gaps |
|--------|--------|-----------|------|
| blog | Stale | 289h | — |
| facebook | Stale | 193h | — |
| intentions | Active | — | — |
| linkedin | Stale | 196h | — |
| music | Complete | — | — |
| notes | Partial | 302h | — |
| people | Partial | — | — |
| photos | Active | 302h | 4 gaps |
| sexuality | Partial | 300h | — |
| stories | Partial | 302h | — |
| documents | Active | — | Just shipped #1330 |

---

## Spoke → Domain Mapping

From the system graph (codebase-graph API). **Note:** Spoke names predate PRODUCT_TAXONOMY and don't fully align with value stream section names above. A code alignment card is planned to reconcile these.

| Spoke | Domains |
|-------|---------|
| Gathering | core, collections, about |
| Sowing | capture, ideas |
| Growing | books, property |
| Practicing | solid, notes |
| Harvesting | music, photos, blog |
| Reflecting | stories, search, gallery |
| Infrastructure | auth, config, infra, ops, rdf, team |

---

## Infrastructure Services (Bedroom)

### nifi
| Layer | Component | Path/Pattern |
|-------|-----------|-------------|
| Service | Apache NiFi 2.8.0 | `https://192.168.86.242:8443/nifi` |
| LaunchAgent | `com.gathering.nifi` | Homebrew: `/opt/homebrew/Cellar/nifi/2.8.0/libexec` |
| Logs | nifi-app.log | `/opt/homebrew/Cellar/nifi/2.8.0/libexec/logs/` |
| Health | System diagnostics API | `https://192.168.86.242:8443/nifi-api/system-diagnostics` |
| Purpose | Governed data pipelines — ETL for harvest flows (#1662) |
| Dependencies | Java (Homebrew), Bedroom Mac |
