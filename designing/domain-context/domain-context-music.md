# Domain Context: Music

Last updated: 2026-03-26 by Wren (#1688)

## ICD

| File | What it defines |
|------|----------------|
| `src/ontology/icd-instance-music.ttl` | Music domain ICD — Apple Music as single source, field mappings |
| `src/ontology/jb-ontology.ttl` | Album, Track, Artist, Genre, MusicCollection classes (v0.7.0) |

## Tests

No music-specific test files found. Harvester validation is manual (record counts, field coverage).

## Persistence

| Type | Location | Details |
|------|----------|---------|
| Fuseki — Music graph | `urn:jb:music/` | Albums, tracks, artists |
| Source files | `/Volumes/Gathering/Music/` on Bedroom (NFS) | 71MB Turtle files |
| Apple Music library | `~/Music/Music/Media/Music/` on Library | Source audio files |
| Harvester | `scripts/harvest-apple-music.js` | Apple Music → TTL |
| Navidrome | `http://192.168.86.242:4533` on Bedroom | Music streaming server (Subsonic API) |
| Shuffle endpoint | `GET /api/music/shuffle?mode=archive|collection&count=N&v=3` | Graph-aware shuffle with genre threading, play count weighting, time-of-day profiles, provenance clustering |
| Stats endpoint | `GET /api/music/stats` | Library statistics |
| Playlist endpoint | `POST /api/music/shuffle/playlist` | Creates Navidrome playlist from shuffle output |
| Fuseki — Tracks | 115,112 tracks, 13,613 albums | `jb:Track`, `jb:Album` types |
| Fuseki — Properties | `jb:byArtist`, `jb:hasGenre`, `jb:playCount`, `dc:title`, `jb:sourceFilePath` | Key queryable fields |

## Key Decisions

| Decision | Summary |
|----------|---------|
| DEC-094 | Harvest pause — no new data loading until ops tightened |
| Single source | Apple Music is the only source. No Spotify, no YouTube Music. |
| Jeff 2026-03-26 | Graph-aware shuffle shipped — archive mode (115K, weighted unplayed) + collection mode (played tracks). V1-V3 all live. |
| Jeff 2026-03-26 | Navidrome integration — shuffle creates Subsonic playlists. Apple Music bypassed for playback. |
| Jeff 2026-03-26 | 77K tracks (65%) have zero plays. Apple Music never surfaces them. Shuffle's archive mode does. |

## Constraints

- **Apple Music is single-source.** No multi-source merge logic needed (unlike photos). The harvester reads the library directly.
- **71MB of Turtle files.** Music was the first domain harvested. The TTL files are large — Fuseki handles them but they take minutes to load.
- **Navidrome on Bedroom.** Music streaming is a Bedroom service, not Library. Files served from NFS mount.
- **Use Finder names, not terminal names.** Jeff sees "Media" in Finder, not "Media.localized". Use `~/Music/Music/Media/Music/` in conversation.
- **Library is a family archive, not just Jeff's taste.** Provenance matters: Kirby's Dr. Demento (~914 tracks), Bobee Sweet's Chumbawamba (~308 tracks), Jeff's parents' vinyl, Aubrey's mom's 78s. Treat with respect.
- **AirPlay from Mac to HomePod is unreliable.** Stale session bug — HomePod goes silent, needs power cycle. Use Navidrome/Subsonic as the playback path instead of Apple Music.
- **Navidrome creds needed for playlist creation.** Subsonic API requires username/password. Check env or config for NAVIDROME_USER/NAVIDROME_PASS.
