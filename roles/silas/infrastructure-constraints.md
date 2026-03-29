# Infrastructure Constraints

Last updated: 2026-03-22
Status: Living document — all roles reference this before designing features or infrastructure changes.

---

## Two-Machine Topology (DEC-054)

| | Library (M1) | Bedroom (M2 Pro) |
|---|---|---|
| **Hostname** | Jeffs-Mac-Mini-M1-3 | Jeffs-Mac-mini |
| **Chip** | Apple M1 (8 cores) | Apple M2 Pro (12 cores) |
| **RAM** | 16 GB | 32 GB |
| **Internal SSD** | 1.8 TB (83% used) | 1.8 TB (1.6 TB free) |
| **IP** | 192.168.86.36 | 192.168.86.242 |
| **External storage** | None | 18 drives, ~178 TB |
| **Network** | Wired Gigabit Ethernet | Wired Gigabit Ethernet |
| **Role** | Compute + native services + development | Media server (images-api) + storage |
| **SSH** | — | Enabled |

**Network**: Both machines on same LAN (192.168.86.0/24). Library: wired Gigabit Ethernet (1.4ms). Bedroom: WiFi (3.4ms). 22 total devices on network.

**Security (ADR-012)**: Non-app services bound to 127.0.0.1 (localhost only). Only the app on port 3000 is LAN-accessible (has SOLID auth).

**Cross-machine operations (ADR-016)**: Read is free (any role, any machine). Write/mutate requires a card + managed tooling. No raw process killing. LaunchAgent changes go through Silas.

---

## Library Mac — What Runs Here

All Gathering services run on the Library Mac:

### Native Services (all LaunchAgents — Docker retired)

Docker was fully retired in early March 2026. All services run as native LaunchAgents. Total: ~30 agents, ~2.5GB RSS.

#### Core Application

| Label | Type | Function | RSS |
|-------|------|----------|-----|
| `com.gathering.app` | KeepAlive | Express app (port 3000) | ~708MB |
| `com.gathering.fuseki` | KeepAlive | Apache Fuseki SPARQL (port 3030, 1.5GB heap) | ~889MB |
| `com.gathering.css` | KeepAlive | Community Solid Server (port 3001) | ~42MB |
| `com.gathering.vikunja` | KeepAlive | Kanban board (port 3456) | ~50MB |

#### Content (WordPress)

| Label | Type | Function | RSS |
|-------|------|----------|-----|
| `com.gathering.wordpress` | KeepAlive | WordPress (port 8081) | ~58MB |
| `com.gathering.mysql` | KeepAlive | MySQL (port 3306) | ~55MB |

#### Observability

| Label | Type | Function | RSS |
|-------|------|----------|-----|
| `com.gathering.prometheus` | KeepAlive | Metrics (port 9090) | ~147MB |
| `com.gathering.grafana` | KeepAlive | Dashboards (port 3100) | ~187MB |
| `com.gathering.loki` | KeepAlive | Log aggregation (port 3102) | ~121MB |
| `com.gathering.promtail` | KeepAlive | Log shipping | ~65MB |
| `com.gathering.alertmanager` | KeepAlive | Alerts (port 9093) | ~27MB |
| `com.gathering.blackbox-exporter` | KeepAlive | ICMP/HTTP probes (port 9115) | ~34MB |
| `com.gathering.mysqld-exporter` | KeepAlive | MySQL metrics (port 9104) | ~19MB |

#### Chorus (coordination)

| Label | Type | Function |
|-------|------|----------|
| `com.chorus.api` | KeepAlive | Chorus context index HTTP API (port 3340) |
| `com.chorus.hooks` | KeepAlive | Rust hook service — PreToolUse/PostToolUse/UserPromptSubmit on unix socket (/tmp/chorus-hooks.sock) |
| `com.chorus.alert-notifier` | KeepAlive | macOS desktop alert notifications |
| `com.chorus.session-watcher` | KeepAlive | Ambient chorus index daemon (fswatch) |
| `com.chorus.fuseki-perf` | KeepAlive | RDF store performance monitoring |
| `com.chorus.fuseki-compact` | StartCalendarInterval | TDB2 weekly compact, Saturday 1am |
| `com.chorus.andon-light` | KeepAlive | Floating menubar role status display |
| `com.chorus.andon-enrich` | StartInterval (30s) | Slow-path enrichment for andon light |
| `com.chorus.ops` | run-once | Operational health checks |
| `com.chorus.perf-baseline` | run-once | Nightly performance baseline |
| `com.chorus.harvest-exporter` | run-once | Harvest metrics export |
| `com.chorus.launchagent-metrics` | run-once | LaunchAgent health metrics |
| `com.chorus.posture-capture` | run-once | Posture photo capture |
| `com.chorus.jeff-input-monitor` | KeepAlive | Jeff activity detection |
| `com.chorus.clearing` | KeepAlive | Multi-role clearing session server |
| `com.gathering.codebase-graph-watcher` | KeepAlive | Codebase graph index updates |

### Disk Budget (1.8 TB SSD)

| Category | Current (2026-03-16) | Notes |
|----------|---------------------|-------|
| Source media (Music, Videos) | ~900 GB | Music rescue (#311) freed ~1 TB |
| ~/Library (Messages, Mail, caches) | ~141 GB | Mostly irreducible |
| ~/CascadeProjects (all code + pods) | ~6 GB | Grows slowly |
| OS + system | ~15 GB | Fixed |
| Fuseki TDB2 (23.3M triples, 82K graphs) | ~3 GB | Grew with VideosNew + photos harvest |
| **Used / Available** | **~71% used** | Healthy — well below 95% (C2) |

---

## Bedroom Mac — What Runs Here

### Application Services (bare Node.js, NOT Docker)

| Service | Port | Process | Log |
|---------|------|---------|-----|
| images-api server | 3001 | `node server.js` (gallery UI) | `/tmp/images-api-server.log` |
| images-api video | 8082 | `node video-server.js` (media serving) | `/tmp/images-api-video.log` |

Code at `CascadeProjects/personal-website/` on both machines (GitHub: WJeffBridwell/personal-website).

### LaunchAgent Services (4)

| Label | Type | Function |
|-------|------|----------|
| `com.gathering.images-api-server` | KeepAlive | Gallery UI (port 3001) |
| `com.gathering.images-api-video` | KeepAlive | Media serving (port 8082) |
| `com.gathering.volume-keepalive` | run-once (4min) | USB enclosure idle prevention |
| `com.gathering.ollama` | KeepAlive | Ollama inference server (port 11434, 0.0.0.0) — nomic-embed-text (#782) |
| `com.gathering.nifi` | KeepAlive | Apache NiFi 2.8.0 (HTTPS port 8443) — governed data pipelines (#1662) |

### Storage

- **Internal SSD**: 1.8 TB (1.6 TB free)
- **External**: 18 drives, ~178 TB total, ~10 TB free (concentrated on VideosNew)
- **Gathering folder**: `/Volumes/VideosNew/Gathering` — 7 TB free, backup destination for Library pods

---

## Canonical Storage Convention (CSC)

All domain source files, harvested data, and generated artifacts follow a single directory convention rooted at `/Volumes/VideosNew/Gathering/` on Bedroom (SMB-mounted on Library at `/Volumes/Gathering-1/`).

### Convention

```
/Volumes/Gathering/
├── Photos/
│   ├── source/
│   │   ├── apple/              ← Apple Photos derivatives (ZUUID-keyed)
│   │   ├── google-takeout/     ← Google Takeout extraction (filename-keyed)
│   │   └── iphone/             ← iPhone backup extraction
│   └── generated/
│       ├── thumbnails/         ← 200x200 JPEG (date-bucketed: YYYY-MM/)
│       └── derivatives/        ← Other generated artifacts
├── Music/
│   ├── source/
│   │   └── apple-music/        ← Apple Music library files
│   └── generated/
│       └── thumbnails/         ← Album art
├── Social/
│   ├── source/
│   │   ├── facebook/           ← Facebook archive extraction
│   │   └── linkedin/           ← LinkedIn data export
│   └── generated/
│       └── thumbnails/
├── Video/
│   ├── source/                 ← Video files by volume
│   └── generated/
│       └── thumbnails/         ← Video poster frames
├── Documents/
│   ├── source/
│   │   ├── notes/              ← Apple Notes export
│   │   └── stories/            ← Story artifacts
│   └── generated/
└── Pipeline/
    ├── staging/                ← NiFi intermediate outputs
    ├── dead-letter/            ← Rejected/unmatched records
    └── manifests/              ← Harvest run manifests
```

### Rules

1. **One root**: `/Volumes/Gathering/` — no domain files outside this tree
2. **source/ vs generated/**: source files from providers go in `source/<provider>/`. Pipeline-produced artifacts (thumbnails, derivatives) go in `generated/`. Never mix them.
3. **Never /tmp/**: generated artifacts are persistent data, not temporary. `/tmp/` gets wiped on reboot. If NiFi or a script produces a file the app needs later, it goes in `generated/`, not `/tmp/`.
4. **NiFi writes here**: all pipeline outputs go to `Pipeline/` or domain `generated/` subdirectories
5. **App reads here**: `servingPath` and `thumbnailPath` in canonical records point into this tree
6. **ICD documents it**: every provider section includes `icd:storagePath` pointing to its `source/` directory
7. **Bedroom is authoritative**: Library accesses via NFS mount. If NFS is down, files are unavailable — that's acceptable (C5)
8. **Idempotent**: re-running a pipeline overwrites `generated/` artifacts in place. Source files are append-only.

### Migration

Existing files in ad-hoc locations (e.g., `~/Pictures/Photos Library.photoslibrary/`, `/Volumes/VideosNew/Gathering/Photos/GoogleTakeoutPhotos/extracted/`) are **symlinked** into the convention tree, not moved. Move happens per-domain as each harvester is rebuilt through NiFi. This is a gradual normalization, not a big-bang migration.

### What this replaces

Before CSC, every harvester independently decided where to store files:
- Apple Photos: `~/Pictures/Photos Library.photoslibrary/resources/derivatives/`
- Takeout: `/Volumes/VideosNew/Gathering/Photos/GoogleTakeoutPhotos/extracted/`
- iPhone: `~/Library/Application Support/MobileSync/Backup/<UDID>/`
- Music: `/Volumes/Gathering/Music/`
- Thumbnails: `public/thumbnails/photos/`

CSC makes storage locations discoverable from the ICD — no more "where are the files?" discovery per domain.

---

## Hard Constraints

These are non-negotiable. Every feature and infrastructure change must respect them.

### C1: All application I/O stays on local SSD
SOLID pods, Fuseki TDB2, and application code live on Library's internal SSD. No network storage for anything the app reads/writes at runtime. Network latency and SMB overhead are unacceptable for pod random I/O and database operations.

### C2: Library disk must stay below 95% utilization
APFS performance degrades sharply above 97%. Warning at 90%, critical at 95%. ~900 GB of the current usage is media — known and intentional, not runaway growth. New features that generate significant disk usage (harvesters, caches, media) must include a disk impact estimate.

### C3: LaunchAgent count is bounded
~30 LaunchAgents on an M1 with 16 GB RAM (~2.5GB total RSS). Each new service costs memory. Justify any new agent against what it replaces or what it adds. Prefer combining functions over new agents.

### C4: Bedroom is storage and media serving
Don't run Gathering application services on Bedroom. It serves media (images-api) and stores data. It's the backup destination, not a compute node. If we ever need more compute, that's a separate architectural decision.

### C5: Network storage is for bulk/backup only
SMB over Gigabit Ethernet is fine for: large file transfers, backups, media serving. It is NOT fine for: pod operations, database I/O, anything latency-sensitive. The Gathering folder on Bedroom is for off-machine backups and overflow, not runtime data.

### C6: No cloud dependencies for core functionality
The system runs entirely on local hardware. Cloud services (if added) are for optional features like public sharing or remote access. Core functionality (pods, Fuseki, browse, capture) must work with no internet connection.

### C7: Harvesters must estimate disk impact before running
The music harvester generated 71 MB of Turtle files + Fuseki indexing overhead. Future harvesters (photos: potentially millions of records) must estimate: Turtle file size, Fuseki triple count, any cached/generated artifacts (thumbnails, etc.). Report this in the harvester brief.

---

## Home Network Topology (192.168.86.0/24)

22 devices, monitored via ICMP probes (blackbox-exporter). Dashboard: `localhost:3100/d/home-network`.

| Category | Count | Devices | Typical Latency |
|----------|-------|---------|-----------------|
| Compute | 2 | Mac Mini Primary (office), Mac Mini Secondary (3rd floor) | 1-3ms |
| Network | 3 | Nest WiFi Router + 2 mesh points | 3-9ms |
| Apple TVs | 3 | Living room, Bedroom, Kitchen | 3-7ms |
| HomePods | 5 | Kitchen (x2), Library, Office, Attic | 6-10ms |
| Entertainment | 3 | Pioneer AV, Roku Ultra, LG OLED TV (all living room) | 4-7ms |
| Smart Speakers | 2 | Bedroom (stereo pair) | 10-12ms |
| Mobile | 3 | Jeff's iPhone, Kathy's iPhone, Apple Watch | 20-25ms (when on WiFi) |
| Other | 1 | HP Printer (office) | 2ms |

**Note**: Mobile devices and some speakers may show DOWN when off-WiFi or sleeping — expected behavior, not an alert condition.

Full inventory: `architect/network-inventory.md`.

---

## Observability Stack

**Dashboards** (Grafana at localhost:3100):

| Dashboard | UID | Purpose |
|-----------|-----|---------|
| Home Cloud | `home-cloud` | Infrastructure overview — CPU, memory, disk, service health, constraints |
| Home Network | `home-network` | All 22 LAN devices — ICMP ping status, latency, room-grouped view |
| App Operations | `app-operations` | Application health — service probes, endpoint routing, log streams |
| Chorus Activity | `chorus-activity` | Team coordination — events, card-commit linking, pipeline flow |
| Node Metrics | `node-metrics` | Host-level metrics |
| Logs Explorer | `logs-explorer` | Full-text log search via Loki |
| Service Overview | `service-overview` | High-level service status |

**Operational scripts** (in `messages/scripts/`):

| Script | Purpose |
|--------|---------|
| `system-state.sh` | Unified lifecycle: status, start, stop, restart, health, verify (ADR-012) |
| `cost-report.sh` | Claude usage + Twilio SMS cost visibility |
| `chorus-audit.sh` | Gate registry + fitness function runner |
| `chorus-log.sh` | Event pipeline emission |
| `board.sh` / `chorus-board.sh` | Kanban board management (Gathering / Chorus) |

---

## Soft Constraints

Preferences, not absolutes. Can be overridden with justification.

### S1: Prefer fewer, well-configured services over many simple ones
Consolidate where it makes sense. The observability stack (Prometheus + Grafana + Loki + Promtail + exporters) is 7 LaunchAgents — that's a lot for monitoring, but at ~600MB total RSS it's acceptable.

### S2: Photo Backup Strategy (#1580, 2026-03-21)

**Inventory (54,121 items on iPhone):**

| Source | Items | Size | Syncs to |
|--------|-------|------|----------|
| iPhone camera roll | 54,121 | 261GB | iCloud Photos + Google Photos |
| iCloud Photos | 24,494 (21,322 photos + 3,172 videos) | 219GB | Library Mac + Bedroom Mac |
| Google Photos (cloud) | Unknown (continuous upload) | 3.1GB on phone | Google Takeout export |
| Google Takeout (on disk) | 68,126 | 1.2TB | Bedroom external drive |
| Library Mac (Apple Photos) | 24,592 | 264GB | iCloud sync |
| Bedroom Mac (Apple Photos) | 24,543 | 16GB (optimized/thumbnails) | iCloud sync |

**Redundancy:**
- Camera roll photos: **4x** (phone + iCloud + Google Photos + Library Mac)
- App media (~30K screenshots, WhatsApp, etc.): **2x** (phone + iCloud Backup)
- Google Takeout originals: **1x** (Bedroom external drive only — risk)

**Decision: Turn off iCloud Photos on Bedroom.**
- It syncs only thumbnails (16GB vs 264GB on Library = "Optimize Storage")
- Costs 40% CPU via ecosystemd for data that exists in 3 other places
- Bedroom's role is storage/serving (C4), not sync target
- Harvest pipeline reads Apple Photos from Library Mac, not Bedroom

**iPhone 54K vs iCloud 24K gap:** ~30K items are app-generated media (screenshots, WhatsApp saves, Messages attachments) that live in the phone's photo library but aren't "camera roll" items synced by iCloud Photos. They're protected by iCloud Backup (311GB).

**Source provenance (Jeff, 2026-03-22):**
- Mac (Apple Photos) library is the **archive** — 14,512 photos (1972-2026), includes Julian's childhood. 11,553 are Apple-only (no Google match by filename). These are irreplaceable.
- iPhone + Google Photos are the **live stream** — synced for years, largely identical. All iPhone photos are likely also in Google.
- Google Takeout on Bedroom is a one-time snapshot of Google cloud — 49,842 photos with dates (1998-2026).
- Both sources span decades with heavy overlap. No clean year cutoff between "archive" and "live."
- Canonical reconciler prefers Apple metadata — correct for the 11.5K Apple-only records that have no other copy.

**Risk: Google Takeout on Bedroom (1.2TB) has no backup.** If the external drive fails, 68K source records lose their originals. Consider a second copy on another Bedroom drive.

**iCloud plan:** 2TB family plan, 608GB used (546.6GB Jeff + 61.3GB family). 1.4TB free.

### S3: Source media stays on Library (post-cleanup)
After Jeff frees ~1 TB, source media (Music, Videos) fits on the local SSD with headroom. No need to move it to SMB unless disk pressure returns. Simpler architecture = fewer failure modes.

### S3: Off-machine backups are still valuable
Even with local disk headroom, copying backups to Bedroom protects against Library disk failure. This is insurance, not a storage necessity.

---

## Implications for Current Work

| Item | Impact |
|------|--------|
| ADR-007 (storage migration) | **Revised** — Phase 2 (media migration to SMB) no longer urgent. Phase 1 (off-machine backups) active — Library pods backed up to Bedroom. |
| Music harvester | **COMPLETE** — 71 MB Turtle, 5,844 albums / 54,331 tracks in Fuseki. |
| Photos harvester | **COMPLETE** — ~1.7M items via collection-graph model. Validated at scale. |
| Sexuality collection | **In progress** — 1.85M items. Collection-graph model proven. |
| Disk headroom | 441 GB free (76%). Comfortable margin above 95% (C2) threshold. |
| Boot-order orchestration (#382) | **SHIPPED** — LaunchAgents with `RunAtLoad` handle reboot recovery natively. |
| Docker retirement | **COMPLETE** — all 18 containers migrated to native LaunchAgents (March 2026). |
| Service redistribution spike (#1432) | **COMPLETE** — recommendation: don't redistribute. Memory headroom is healthy at ~9.5GB free. |

---

## Related Documents

- `service-manifest.md` — Full service inventory with ports, health checks, LaunchAgents
- `system-architecture.md` — System-wide architecture, ADR index
- `operations-assessment.md` — Ops health scorecard, priority actions
- `network-inventory.md` — All 22 LAN devices with IPs and MACs
- `adr/ADR-007-two-machine-storage-topology.md` — Storage topology decisions
- `adr/ADR-012-network-bind-security.md` — Docker port binding security
- `adr/ADR-016-cross-machine-operations.md` — Cross-machine SSH protocol

---

— Silas
