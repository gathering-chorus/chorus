# ADR-007: Two-Machine Storage Topology

**Date**: 2026-02-17
**Status**: Accepted (revised same day)
**Decider**: Jeff
**References**: infrastructure-constraints.md, infrastructure.md (memory file)

## Context

The music harvester (#47) ingested 54,331 tracks / 5,844 albums / 7,327 artists from Apple Music. Combined with existing source media (~1.9TB), Docker containers, and system files, the primary Mac mini M1's 2TB SSD hit 100% utilization. Docker's metadata DB corrupted overnight, taking down Fuseki, Grafana, and the bridge.

Jeff has a second Mac mini (M2 Pro, 32GB RAM) on the 3rd floor connected via wired Gigabit Ethernet with 18 external drives (~178TB) and a Gathering folder on VideosNew with 7TB free.

**Revision (same day):** Jeff identified ~1TB of data he can free from the primary Mac immediately. This changes the calculus — with ~800GB+ free on the local SSD, the urgent need to migrate media to SMB disappears. The simpler architecture is to keep everything local.

## Decision

**All application and source data stays on the primary Mac's local SSD.** The secondary Mac serves as backup destination and media server (images-api) only.

### Primary Mac (all runtime I/O)
Everything the Gathering system reads or writes lives here:
- **SOLID pods** (Turtle files)
- **Fuseki TDB2** (SPARQL database)
- **Docker containers and images**
- **Application source code**
- **Source media** (music files, videos — stays local after Jeff's cleanup)

### Secondary Mac (backup + media serving)
- **Off-machine backups** — daily copy of pods + ontology + Fuseki to `/Volumes/VideosNew/Gathering/backups/`
- **images-api** — serves the ~178TB video/photo library (existing, unchanged)
- **Overflow** — if primary disk fills again in the future, media migration to SMB is a documented fallback (see Fallback Plan below)

## Why Local-Only is Better

| Factor | Local SSD | SMB over Gigabit |
|--------|-----------|------------------|
| Random I/O latency | ~0.01ms | ~1-5ms |
| Sequential throughput | ~3 GB/s | ~120 MB/s |
| Reliability | No network dependency | Mount can disconnect |
| Complexity | Zero | SMB config, mount management, fstab |
| Failure modes | Disk failure | Disk failure + network failure + SMB failure |

With ~800GB free after cleanup, there's no performance or capacity reason to introduce network storage for runtime data. Simpler architecture = fewer failure modes.

## Off-Machine Backups (Still Valuable)

Even with local headroom, copying backups to the M2 Pro protects against primary disk failure. This is insurance, not a storage necessity.

- **What**: Daily pod + ontology + Fuseki TDB2 backups
- **Where**: `/Volumes/VideosNew/Gathering/backups/` on Mac mini
- **How**: rsync over SSH (simpler than SMB mount)
- **When**: After local backup completes, if remote is reachable
- **Resilience**: If remote is unreachable, local backup continues unaffected

## Disk Budget (Post-Cleanup)

| Category | Size | Notes |
|----------|------|-------|
| Source media (after ~1TB freed) | ~900 GB | Music, Videos, iTunes |
| Docker (VM + images + volumes) | ~48 GB | |
| ~/Library | ~141 GB | Messages, Docker, Mail, caches |
| ~/CascadeProjects | ~6 GB | Code + pods + Turtle |
| OS + system | ~15 GB | |
| **Used** | **~1.1 TB** | |
| **Available** | **~800+ GB** | Healthy headroom |

**Target**: Keep utilization below 85% (~1.5TB used). Above 90%, APFS performance degrades and Docker becomes unstable.

## Fallback Plan (If Disk Fills Again)

If a future harvester (photos: potentially millions of records) pushes the primary Mac past 85%:

1. Share `/Volumes/VideosNew/Gathering` via SMB on the Mac mini
2. Mount on primary Mac
3. Move the **largest non-runtime data** (source media, not pods/Fuseki) to SMB
4. The constraints from the original version of this ADR apply: pods and Fuseki never go on SMB

This fallback is documented but not active. It's a known path, not a current plan.

## Constraints

See `infrastructure-constraints.md` for the full constraint set. Key ones for this decision:

- **C1**: All application I/O stays on local SSD
- **C2**: Primary Mac disk stays below 85% utilization
- **C5**: Network storage is for bulk/backup only
- **C7**: Harvesters must estimate disk impact before running

## Consequences

### Positive
- Simplest possible architecture — one machine, local disk, no network dependencies
- No SMB configuration, mount management, or network failure modes
- All I/O is fast (NVMe SSD)
- Off-machine backups provide disk failure protection

### Negative
- Primary Mac is still a single point of failure for runtime (mitigated by backups)
- Source media consumes ~900GB of the 2TB SSD (but 800GB free is sufficient headroom)
- If disk fills again, we'll need to revisit (but now we have a documented fallback)
