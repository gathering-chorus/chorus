# Pipeline Manifest

Central inventory of photo, music, and convergence pipeline scripts.
Each entry maps to: location, owner, run mode, and external dependencies.

## Live Services (LaunchAgent — always running)

| Script | Location | Owner | Run Mode | Dependencies |
|--------|----------|-------|----------|-------------|
| Navidrome | `platform/scripts/launchagents-canonical/com.gathering.navidrome.plist` | Silas | LaunchAgent (KeepAlive) | `/Volumes/VideosNew/Gathering/Music/Music`, port 4533 |
| harvest-exporter | `~/Library/LaunchAgents/com.chorus.harvest-exporter.plist` | Silas | LaunchAgent (15 min) | shared-observability, Prometheus |
| deep-health | `platform/scripts/deep-health.sh` | Silas | LaunchAgent (5 min) | fswatch, chorus-hooks binary, nudge |
| chorus-ops | `platform/scripts/chorus-ops.sh` | Silas | LaunchAgent (5 min) | Vikunja, Fuseki, Clearing |

## Photo Pipeline — Repeatable Workflows

| Script | Location | Owner | Run Mode | Dependencies |
|--------|----------|-------|----------|-------------|
| photo-pipeline.py | `roles/kade/scripts/` | Kade | Manual, idempotent | Fuseki (localhost:3030), iPhone backup, canonical graph |
| run-canonical-rebuild.py | `roles/kade/scripts/` | Kade | Manual, idempotent | Fuseki (localhost:3030), 4 era source graphs |
| build-nifi-photos-flow.py | `roles/kade/scripts/` | Kade | Manual | NiFi (jeffs-mac-mini.lan:8443), Fuseki (192.168.86.36:3030) |
| generate-thumbnails.sh | `building/products/convergence/` | Silas | Manual, batch | Fuseki canonical graph, sips, `/Volumes/VideosNew/Gathering/Photos/generated/thumbnails/` |
| generate-thumbnails-v2.sh | `building/products/convergence/` | Silas | Manual, batch | Same as above |
| generate-thumbnails-library.py | `building/products/convergence/` | Silas | Manual, batch | Library machine photos |
| gen-thumbs-bedroom.py | `roles/kade/scripts/` | Kade | Manual, batch | Bedroom machine (SSH), sips, Takeout source files |
| gen-video-thumbs-bedroom.py | `roles/kade/scripts/` | Kade | Manual, batch | Bedroom machine (SSH), ffmpeg |

## Photo Pipeline — NiFi Infrastructure

| Script | Location | Owner | Run Mode | Dependencies |
|--------|----------|-------|----------|-------------|
| nifi-dsl.sh | `platform/scripts/` | Silas | IaC control plane | NiFi (jeffs-mac-mini.lan:8443), Fuseki |
| configure-nifi-pipeline.sh | `building/products/convergence/one-shots/` | Silas | One-shot setup | NiFi API |
| nifi-apple-to-fuseki.py | `building/products/convergence/` | Silas | Phase 1 standalone | Apple Photos SQLite, Fuseki, sips |
| build-nifi-iphone-native.py | `building/products/convergence/one-shots/` | Silas | One-shot setup | NiFi API, SQLite JDBC driver |
| generate-thumb-nifi.sh | `jeff-bridwell-personal-site/scripts/nifi/` (moved #3599) | Kade | NiFi processor | sips, source photo files |

## Convergence / Harvest

| Script | Location | Owner | Run Mode | Dependencies |
|--------|----------|-------|----------|-------------|
| harvest-media.sh | `building/products/convergence/` | Silas | On-demand | MongoDB, Fuseki (192.168.86.36:3030), `/harvest` endpoint |
| harvest-media-export.js | `building/products/convergence/` | Silas | On-demand | MongoDB (mongosh), N-Triples output |
| fuseki-maintenance.sh | `building/products/convergence/` | Silas | On-demand DBA + LaunchAgent (com.chorus.fuseki-compact, Sat 1am compact) | Fuseki (localhost:3030/pods), TDB2 |
| fuseki-baseline.sh | `building/products/convergence/` | Silas | On-demand | Fuseki |
| graph-lint.sh | `building/products/convergence/` | Silas | On-demand | Fuseki, SPARQL |
| owl-linter.py | `building/products/convergence/` | Silas | On-demand | OWL files |
| test-pipeline-ac.py | `building/products/convergence/` | Silas | Test harness | Pipeline scripts |

## Music

| Script | Location | Owner | Run Mode | Dependencies |
|--------|----------|-------|----------|-------------|
| Navidrome | See Live Services above | Silas | LaunchAgent | `/Volumes/VideosNew/Gathering/Music/Music`, port 4533 |

## Quality Gates

| Script | Location | Owner | Run Mode | Dependencies |
|--------|----------|-------|----------|-------------|
| nifi_discipline.rs | `platform/services/chorus-hooks/src/hooks/` | Kade | PreToolUse hook | NiFi API, demo/accept events |

## Utilities

| Script | Location | Owner | Run Mode | Dependencies |
|--------|----------|-------|----------|-------------|
| wm-schema-extract.py | `roles/kade/scripts/` | Kade | One-shot | Schema source |
| backfill-domain-tags.sh | `platform/scripts/` | Silas | On-demand | Vikunja API, card titles |
| smoke-check.sh | `platform/scripts/` | Silas | On-demand | App endpoints (localhost:3000) |
