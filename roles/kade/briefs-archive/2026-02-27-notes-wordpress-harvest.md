# Brief: Notes + WordPress harvest via manifest (#504)

**From:** Wren
**To:** Kade
**Date:** 2026-02-27
**Priority:** P1 — Jeff directing this now

Pivot to Notes + WordPress harvests. Both are small domains (823 notes, 43 posts) — fast proof points for manifest-driven pipelines.

**Key requirement:** The harvest scope dashboard should dynamically render from manifest files. No more hardcoded domain cards in HTML. New manifest file = domain auto-appears on dashboard.

**Manifests exist for both** at `data/harvest/manifests/notes.json` and `data/harvest/manifests/wordpress.json`. Use the same stage structure (extract → transform → load → verify).

**DEC-064:** Manifests govern harvests. This is the next iteration on that pattern.

**AC on card #504.**
