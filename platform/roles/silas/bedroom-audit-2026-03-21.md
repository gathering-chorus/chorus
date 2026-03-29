# Bedroom Mac Health Audit — 2026-03-21

**Machine:** Mac mini M2 Pro, 32GB RAM, 2TB internal SSD
**IP:** 192.168.86.242 | **Uptime:** 17h10m | **Load:** 2.50 / 2.72 / 2.83

## Findings

### CRITICAL — MacKeeper is burning CPU

| Process | CPU | RAM | Notes |
|---------|-----|-----|-------|
| MacKeeper AntiVirus | 21% | 2.0% (660MB) | Real-time scanning daemon |
| MacKeeper Endpoint Security | 2.1% | 0.1% | System extension |
| MacKeeperAgent | 0% | 0.3% | UI agent |
| MacKeeper PrivilegedHelper | 0% | 0.1% | Helper daemon |

**Total: ~23% CPU, ~830MB RAM** — constantly. This is a third-party antivirus running full-time on a machine that serves media files. macOS has built-in XProtect/MRT. MacKeeper adds no value here and actively hurts performance.

**Recommendation:** Uninstall MacKeeper. Use built-in macOS security.

### WARNING — Apple System Daemons High CPU

| Process | CPU | Notes |
|---------|-----|-------|
| ecosystemd | 48% | Apple ecosystem sync — likely iCloud/Handoff |
| trustd | 27% | Certificate validation — may be related to MacKeeper |
| ecosystemanalyticsd | 21.5% | Analytics collection |

**Total: ~97% CPU** from system daemons. The `trustd` load may drop after MacKeeper removal (AV triggers constant cert checks). `ecosystemd` at 48% is abnormal — may be stuck in an iCloud sync loop.

**Recommendation:** After MacKeeper removal, monitor for 24h. If ecosystemd stays high, check iCloud sync status and consider disabling unnecessary sync on this machine.

### Storage — 28 Volumes, Most at Capacity

| Volume | Size | Used | Free | Status |
|--------|------|------|------|--------|
| Internal (Data) | 1.8 TB | 326 GB | 1.5 TB | **OK (18%)** |
| VideosRilez-Ta | 15 TB | 15 TB | 29 GB | **CRITICAL (100%)** |
| VideosLeb-Luci | 9.1 TB | 9.1 TB | 19 GB | **CRITICAL (100%)** |
| VideosMaria-Mega | 3.6 TB | 3.6 TB | 42 GB | **WARNING (99%)** |
| VideosMega-Mia | 1.8 TB | 1.8 TB | 23 GB | **WARNING (99%)** |
| VideosLucj-Maria | 4.5 TB | 4.5 TB | 25 GB | **CRITICAL (100%)** |
| VideosUma-Zaa | 7.3 TB | 7.3 TB | 25 GB | **CRITICAL (100%)** |
| VideosCoco-Eliza | 9.1 TB | 9.0 TB | 52 GB | **CRITICAL (100%)** |
| VideosKey-Lea | 9.1 TB | 9.0 TB | 86 GB | **CRITICAL (100%)** |
| VideosNia-Rilex | 9.1 TB | 9.0 TB | 66 GB | **CRITICAL (100%)** |
| VideosTb-Uma | 2.7 TB | 2.7 TB | 15 GB | **CRITICAL (100%)** |
| VideosAlexa-Amb | 7.3 TB | 7.2 TB | 67 GB | **CRITICAL (100%)** |
| VideosHime-Jeff | 7.3 TB | 7.2 TB | 57 GB | **CRITICAL (100%)** |
| VideosNew | 9.1 TB | 5.6 TB | 3.5 TB | **OK (62%)** |
| VideosMulti | 9.1 TB | 8.5 TB | 628 GB | **OK (94%)** |
| VideosAme-Aria | 9.1 TB | 8.6 TB | 516 GB | **OK (95%)** |
| PhotosNew | 931 GB | 416 GB | 515 GB | **OK (45%)** |

**13 volumes at 99-100% capacity.** These are media archive volumes — likely intentionally full. But any write operation (thumbnail gen, index update) will fail on these volumes.

Internal SSD is healthy at 18% — plenty of headroom.

### Services — Running

| Service | Port | Status |
|---------|------|--------|
| MongoDB | 27017 | Running |
| images-api-video | 8082 | Running |
| images-api-server | 3001 | Running |
| Ollama | 11434 | Running |
| node-exporter | 9100 | Running |
| Navidrome | (loaded) | Running |
| volume-keepalive | - | Running |
| NFS export | /Volumes/VideosNew/Gathering | Active, serving to 192.168.86.36 |

**No failed services.** All LaunchAgents healthy.

### SMART Status

Internal SSD: **Verified** — healthy.
External drives: Not checked individually (would need per-disk SMART query).

### Memory

- 32 GB total, 0 swap used
- Active: ~13.5 GB, Inactive: ~13.5 GB, Wired: ~3.4 GB, Compressed: ~1.2 GB
- Memory pressure is manageable — no swapping

## Summary

| Severity | Issue | Action |
|----------|-------|--------|
| **CRITICAL** | MacKeeper burning 23% CPU + 830MB RAM | Uninstall |
| **WARNING** | ecosystemd at 48% CPU | Monitor after MacKeeper removal |
| **WARNING** | trustd at 27% CPU | Likely resolves with MacKeeper removal |
| **INFO** | 13 volumes at 99-100% | Expected for archive volumes — no action unless writes needed |
| **OK** | Internal SSD at 18% | Healthy |
| **OK** | All services running | No failures |
| **OK** | Memory — no swap | 32GB adequate |
| **OK** | Internal SSD SMART | Verified |

## Top Recommendation

**Uninstall MacKeeper.** It's consuming ~23% sustained CPU and 830MB RAM on a machine that should be running lean for media serving and harvest operations. macOS built-in security (XProtect, Gatekeeper, MRT) is sufficient for this use case. The `trustd` and possibly `ecosystemd` CPU spikes may also resolve — AV software triggers excessive certificate validation and system event monitoring.
