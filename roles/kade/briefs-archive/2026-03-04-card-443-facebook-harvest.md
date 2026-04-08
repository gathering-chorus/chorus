# Brief: #443 Harvest Facebook + LinkedIn Exports to RDF

**From:** Wren | **To:** Kade | **Priority:** P2 | **Status:** Next

## What

Parse Jeff's Facebook and LinkedIn data exports and ingest into the Gathering RDF pipeline. Both exports are downloaded and ready.

## Data Sources

### Facebook
- **Path**: `/Users/jeffbridwell/Downloads/facebook-jeffbridwell169465-2026-03-03-34FwtHTO/`
- **Posts**: `your_facebook_activity/posts/your_posts__check_ins__photos_and_videos_1.json`
- **2,033 posts** (Dec 2010 – Nov 2025) — 1,606 with text, 801 with attachments
- **Format**: JSON array — `{ timestamp (unix), data[].post (text), attachments[].data[].external_context.url, title }`

### LinkedIn
- **Path**: `/Users/jeffbridwell/Desktop/Complete_LinkedInDataExport_03-01-2026.zip.zip`
- **Extract to**: `/tmp/linkedin-export/` or `data/harvest/linkedin/`
- **Key files** (CSV):
  - `Shares.csv` (~195 rows) — posts/shares. Columns: `Date, ShareLink, ShareCommentary, SharedUrl, MediaUrl, Visibility`
  - `Positions.csv` (10 rows) — career history with rich descriptions
  - `Connections.csv` (~2,103 rows) — professional network
  - `Comments.csv` (~116 rows) — comments on others' posts
  - `Articles/` — 3 HTML articles

## Pipeline

Follow existing harvest pattern (wordpress-harvester is closest analog):

1. **Extract** — parse JSON (Facebook) and CSV (LinkedIn)
2. **Transform** — normalize → TTL at `data/pods/jeff/facebook/posts/{slug}.ttl` and `data/pods/jeff/linkedin/posts/{slug}.ttl`
3. **Load** — `harvest-sync-fuseki.sh facebook/` + `linkedin/`
4. **Verify** — searchable, counts match, platform filter works

## Pattern Files
- `src/services/wordpress-harvester.service.ts` — closest analog
- `src/services/notes-pod.service.ts` — simple pod writer
- `data/harvest/manifests/facebook.json` + `linkedin.json` — stubs exist
- Graph URI: `http://localhost:3000/pods/jeff/{facebook,linkedin}/posts/{slug}.ttl`

## RDF Mapping
- Type: `jb:SocialPost`
- `dcterms:created` — timestamp
- `jb:postBody` — text / share commentary
- `jb:platform` — `"facebook"` or `"linkedin"`
- `jb:hasAttachment` — links, media refs
- `jb:slug` — date + content hash
- `jb:harvestedIn` — harvest run

## LinkedIn Bonus (if time)
- `jb:Position` from Positions.csv — Jeff's career history (high Self domain value)
- `jb:Connection` from Connections.csv — professional network

## Acceptance Criteria
1. Facebook posts ingested (~2,033 ± dedup)
2. LinkedIn shares ingested (~195 ± dedup)
3. Both synced to Fuseki with correct named graphs
4. Posts appear in `/search` results
5. Platform filter — user can filter by "facebook" or "linkedin"
6. Both manifests updated with completed stages

## Out of Scope
- Media file downloads (just capture URL refs)
- Facebook messages (privacy)
- Facebook comments/reactions (future card)
