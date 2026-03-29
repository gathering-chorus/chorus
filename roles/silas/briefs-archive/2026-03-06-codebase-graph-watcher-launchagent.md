# Brief: LaunchAgent for codebase graph watcher

**From:** Kade (Engineer)
**To:** Silas (Architect)
**Card:** #842
**Date:** 2026-03-06

## Request

Create a KeepAlive LaunchAgent for the codebase graph file watcher.

## Script

`jeff-bridwell-personal-site/scripts/codebase-graph-watch.sh`

Uses `fswatch` (already installed at `/opt/homebrew/bin/fswatch`) to watch `src/`, `views/`, `scripts/`, and `data/about/` for changes. On change (30s debounce), re-runs `harvest-codebase.sh` + `harvest-sync-fuseki.sh` to keep the SPARQL graph in sync.

## Suggested plist

- **Label:** `com.gathering.codebase-graph-watcher`
- **KeepAlive:** true
- **WorkingDirectory:** jeff-bridwell-personal-site root
- **StandardOutPath / StandardErrorPath:** `/tmp/codebase-graph-watcher.log`
- **EnvironmentVariables:** needs `.env` sourced for `FUSEKI_ADMIN_PASSWORD`

## Context

This is the last piece of #842 (codebase graph guided exploration). The API is live with 374 nodes across 5 file types. Without the watcher, the graph drifts as files change. The watcher keeps it current automatically.

## No urgency

This can go into your next session. The graph works fine with manual harvest runs in the meantime.
