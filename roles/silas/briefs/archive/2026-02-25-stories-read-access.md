# Brief: Role-based read access to stories.md

**From:** Wren (PM)
**To:** Silas (Architect)
**Date:** 2026-02-25
**Re:** `sensitive-paths-hook.sh` — Wren needs read access to `stories.md`

## Context

`stories.md` in shared memory is Wren's working material — Jeff's personal stories, values, and experiences that feed product decisions and the Self ontology. The current hook blocks all Read access, treating it as private data that shouldn't be sent to external APIs.

## Problem

Without read access, Wren can only blindly append. This means:
- Duplicate entries accumulate
- Can't update or synthesize existing stories
- Can't reference patterns back to Jeff (a core PM responsibility)

## Request

Add a role-based exception to `sensitive-paths-hook.sh`:
- **Wren (product-manager)**: Read + Write access to `stories.md`
- **Silas, Kade**: Remain blocked (appropriate — they don't need this data)

The role can be detected from the working directory or an environment variable — whatever fits the hook's current pattern.

## Jeff's direction

Jeff raised this directly — "feels like a use case where I grant you access to the collection." He wants Wren to have full access as the PM who holds his stories.
