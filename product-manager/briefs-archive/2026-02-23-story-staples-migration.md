# Story: Staples IBM-to-Modern Migration

**From:** Silas (routing to Wren)
**Date:** 2026-02-23
**Type:** story capture

## The Story

At Staples, Jeff was part of a massive migration — converting everything from IBM products on staples.com and staplesadvantage.com to Java backends and React front ends. The effort took 6-7 years and involved roughly 30-40 domain teams overall. The first deploys on the new platform were "legendarily bad" — 200 people on a bridge call with the person who is now CTO of Conoco-Phillips.

## Key Quotes

- "the first deploys on the new site were legendarily bad"
- "like 200 people on a bridge w/ the now CTO of Conoco-Phillips"
- "talk about coordination issues"

## Context

- Shared while building a deploy freeze feature for app-state.sh — direct lineage from those coordination failures to this kill switch
- This is the Staples domain/integration/information architect tenure referenced in memory
- IBM → Java/React is a full platform rewrite, not a lift-and-shift
- 30-40 domain teams = massive cross-team coordination surface
- The 200-person bridge call is the anti-pattern that Chorus is designed to prevent at small scale
- "Legendarily bad" first deploys = the formative experience behind Jeff's deploy discipline, health gates, rollback mechanisms, and the "no manual PID killing" rule

## Signal for Wren

- **Career arc update**: EXE Technologies (build engineer, 2M LOC Makefiles) → Staples (domain/integration architect, 30-40 team migration, 6-7 years) → FastX Partners (fractional CTO) → Gathering
- **Coordination at scale**: Jeff has firsthand experience with coordination failure at enterprise scale. Chorus isn't theoretical — it's informed by watching 200 people fail to coordinate on a bridge call.
- **The CTO of Conoco-Phillips was on that bridge**: Jeff operated at a level where the people in the room went on to become Fortune 500 C-suite. This is relevant for the revenue/consulting narrative.
- **Deploy trauma → deploy discipline**: The legendarily bad deploys explain why app-state.sh exists, why there's a health gate, why there's a rollback mechanism, why there's now a freeze. Every one of those features has a scar behind it.
- **Pattern**: Jeff builds small what he saw fail big. Chorus = the 3-person version of what 200 people on a bridge couldn't do.
