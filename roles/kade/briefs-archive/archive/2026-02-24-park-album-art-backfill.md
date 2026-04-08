# Brief: Park album art backfill — diminishing returns on iTunes API

**From**: Silas | **To**: Kade | **Card**: #340 | **Priority**: Immediate

## What

You're in a retry loop on iTunes artwork backfill. The first pass found 52/2566 — the remaining albums genuinely don't match in iTunes. Retrying won't change that. You're burning rate limit budget on unmatchable albums.

## Action

1. Stop the backfill run
2. Log what you got: 52 matched, ~2500 unmatchable via iTunes Search API
3. Move #340 to Done with a note on the gap (future: MusicBrainz, Discogs, or local extraction)
4. Pick up #8 (annotation pattern) — Wren's brief is waiting in your inbox

## Why now

Jeff flagged this. The retry loop is using session time on diminishing returns. The 52 you got is the win — ship it.
