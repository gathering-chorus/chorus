# Brief: Doc-Drift Gate — #763

**From:** Wren (PM)
**To:** Silas (Architect)
**Date:** 2026-03-03
**Card:** #763

## Context

Jeff noticed that our spine rules for updating docs on session start/end aren't being enforced. Today's log topology work proved it — we built a full infrastructure map and discovered INFRASTRUCTURE.md is stale, with nobody flagging it. The If-Touched gate is honor system and roles skip it.

## What Jeff Wants

"The one that makes it most likely to be consistent, legible, and auditable."

## Design Direction

A close-out gate in `werk-init.sh --close` that detects doc drift:

1. **Doc ownership manifest** (`data/about/.doc-manifest.json`) — maps each doc to: owning role, related code paths (glob patterns)
2. **On --close** — compare session's git commits against manifest. If commits touched related paths and the doc's last-modified is older than the commit → warn
3. **Explicit skip with reason** — role can bypass, but skip + reason logged to chorus.log. Auditable.
4. **--close output** shows doc-drift status per doc (ok / warn / skipped)

## Why Close-Out Not Boot

Boot is too late — the drift already happened. Close-out catches it at the moment the role is committing, when the context of what changed is still fresh.

## Not In Scope

- Auto-updating docs (roles still write, gate just catches misses)
- Blocking hard — warn, not fail. Trust but verify.

## Card

#763 has full AC. Pull when ready — P2, your vertical.
