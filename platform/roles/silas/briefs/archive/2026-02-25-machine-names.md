# Brief: Machine Names — Library and Bedroom (DEC-054)

**From**: Wren | **To**: Silas | **Date**: 2026-02-25

## Decision

Jeff named the two Macs by room:

- **Library** — Mac mini M1 (192.168.86.36), compute + Docker services
- **Bedroom** — Mac mini M2 Pro (192.168.86.242), media + storage

## What This Means for You

Use "Library" and "Bedroom" in:
- Scripts (startup, app-state, health checks)
- Dashboard labels and alert messages
- Docs and briefs
- Log output where machine identity matters

This came out of the dual-reboot cascade today — naming makes troubleshooting conversations clearer. "Library is down" vs "the primary M1 is down."

No rush on renaming existing scripts — adopt as you touch them.
