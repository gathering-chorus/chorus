# Brief: Switch Gathering Music mount from SMB to NFS

**From:** Kade | **To:** Silas | **Date:** 2026-03-06 | **Priority:** P1
**Context:** #1110 Music canonical matching

## Problem

Apple Music created a library pointing at `/Volumes/Gathering/Music/` but only indexed 249 albums / 2,758 tracks out of 187,829 tracks on disk (6,718 album dirs, 2,788 artist dirs). It's been 24+ hours with no further progress.

Root cause: the mount is **SMB** (`smbfs`), not NFS. Apple Music silently fails to scan large libraries over SMB — no Spotlight indexing on network volumes, scanner chokes on the directory tree.

```
//...@Jeff's Mac mini._smb._tcp.local/Gathering on /Volumes/Gathering (smbfs)
```

## Request

Reconfigure the Bedroom Mac share so `/Volumes/Gathering/` (or at minimum `/Volumes/Gathering/Music/`) is mounted via **NFS** on Library. Jeff wants his local Apple Music library intact — just the mount transport changed.

## Constraints

- Jeff's Apple Music library (`~/Music/jeff-music/`) points at this mount. After switching, the mount path must remain `/Volumes/Gathering/Music/` or Apple Music will lose its references.
- Bedroom Mac (192.168.86.242) currently exports via SMB. May need NFS export added.
- 187K files, ~1TB. NFS should handle the sustained metadata reads Apple Music needs.
- Don't disrupt other services using the Gathering volume (volume-keepalive, images-api).

## Disk stats (for context)

| Metric | Count |
|--------|-------|
| Artist dirs | 2,788 (with audio) |
| Album dirs | 6,718 |
| Track files | 187,829 |
| Duplicate " 1" files | 54,081 (~29%) |
| File formats | 125K m4a, 27K mp3, 242 m4p, misc |
