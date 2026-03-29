# Brief: Storage Migration — Off-Machine Backups + Media Offload

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-17
**Priority**: P1 (Phase 1), P2 (Phase 2)
**References**: ADR-007, infrastructure.md (memory file)

---

## Context

The music harvester pushed the primary Mac's 2TB SSD to 100%. Docker corrupted overnight. We need to offload large media files to the Mac mini on the 3rd floor (192.168.86.242) and set up off-machine backups.

See ADR-007 (`architect/adr/ADR-007-two-machine-storage-topology.md`) for the full architectural decision.

## Two-Machine Topology (Summary)

| Machine | Role | SSD | Free |
|---------|------|-----|------|
| Mac mini M1 (192.168.86.36) | Primary — compute + services | 2.0 TB | ~13 GB (critical) |
| Mac mini M2 Pro (192.168.86.242) | Storage — media + backups | 1.8 TB internal + 178 TB external | 7.0 TB on VideosNew |

Network: Wired Gigabit Ethernet, same LAN.

## What You Need to Do

### Phase 1: SMB Share + Off-Machine Backups (P1)

**1. Set up SMB share on Mac mini (requires SSH)**

SSH into `192.168.86.242` and share the Gathering folder:

```bash
# On Mac mini M2 Pro (192.168.86.242)
sudo sharing -a /Volumes/VideosNew/Gathering -n "Gathering" -g 001 -s 001
```

Verify the share is visible:
```bash
# On primary Mac (192.168.86.36)
smbutil view //192.168.86.242
```

**2. Create mount point on primary Mac**

```bash
sudo mkdir -p /Volumes/Gathering
```

**3. Mount the share**

```bash
# Test mount (interactive)
mount -t smbfs //jeffbridwell@192.168.86.242/Gathering /Volumes/Gathering

# For persistent mount, add to /etc/fstab or use Login Items
```

**4. Create directory structure**

```bash
mkdir -p /Volumes/Gathering/backups/daily
mkdir -p /Volumes/Gathering/backups/weekly
mkdir -p /Volumes/Gathering/music
mkdir -p /Volumes/Gathering/video
```

**5. Update backup script**

The existing backup script (`jeff-bridwell-personal-site/backup-pods.sh`) writes to a local directory. Add a Phase 3 copy step that rsyncs the latest backup to `/Volumes/Gathering/backups/`:

```bash
# After existing backup completes:
REMOTE_BACKUP_DIR="/Volumes/Gathering/backups"
if [ -d "$REMOTE_BACKUP_DIR" ]; then
    rsync -av "$BACKUP_DIR/latest/" "$REMOTE_BACKUP_DIR/daily/"
    echo "Off-machine backup copied to $REMOTE_BACKUP_DIR"
else
    echo "WARNING: Remote backup mount not available at $REMOTE_BACKUP_DIR"
fi
```

**Important**: The backup should continue to work if the SMB mount is unavailable. Don't let a network issue break local backups.

### Phase 2: Media Migration (P2 — after Phase 1 validated)

**Do not start Phase 2 until Phase 1 is working reliably for at least a few days.**

Phase 2 moves ~1.9TB of source media from the primary Mac to the Gathering mount:

| Source | Destination | Size |
|--------|-------------|------|
| ~/Downloads/Music | /Volumes/Gathering/music/downloads/ | ~481 GB |
| ~/Downloads/iTunes | /Volumes/Gathering/music/itunes/ | ~462 GB |
| ~/Videos | /Volumes/Gathering/video/ | ~549 GB |
| ~/Music | /Volumes/Gathering/music/library/ | ~484 GB |

**Apple Music library is the hardest part.** The library database has absolute file paths. Moving files without updating the library database will break playback. Research the Apple-supported way to relocate the library before touching ~/Music.

**Migration protocol:**
1. `rsync -av --progress <source>/ <destination>/` (copy, preserve metadata)
2. Verify file counts and sizes match
3. Spot-check a few files (can they be opened from the new location?)
4. Only after verification: remove originals from primary SSD
5. Never delete originals and copy in the same step

**Expected outcome**: Primary SSD drops from ~100% to ~10% used.

## Constraints

- **Pods stay local.** Never put SOLID pods on SMB.
- **Fuseki stays local.** Database needs local disk.
- **Don't break Apple Music.** The ~/Music move needs research first.
- **Backup script must be resilient.** If SMB mount is down, local backups continue.
- **Jeff wants zero impact to running services.** The app, Fuseki, Docker — all unchanged.

## Acceptance Criteria

### Phase 1
- [ ] Gathering folder shared via SMB from Mac mini
- [ ] Mount accessible on primary Mac at /Volumes/Gathering
- [ ] Directory structure created
- [ ] Backup script copies to remote after local backup completes
- [ ] Backup script handles SMB unavailability gracefully
- [ ] At least one successful off-machine backup cycle verified

### Phase 2
- [ ] ~/Downloads/Music migrated and verified
- [ ] ~/Downloads/iTunes migrated and verified
- [ ] ~/Videos migrated and verified
- [ ] ~/Music migrated (with Apple Music library relocation research done first)
- [ ] Primary SSD has significant free space recovered
- [ ] All migrated content accessible from new location

## Questions for Jeff (before Phase 2)

1. **Apple Music**: Do you actively use Apple Music on this Mac? If not, we can just move the files without worrying about the library database.
2. **Videos**: Are these referenced by any application, or are they standalone files we can move freely?
3. **Timeline**: Phase 1 can start immediately. Phase 2 when you're comfortable.

---

— Silas
