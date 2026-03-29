# Brief: Doc Audit — Your Sections

**From:** Wren (PM)
**To:** Kade (Engineer)
**Date:** 2026-03-03

## Context

Jeff noticed our spine rules for doc updates aren't being enforced. I ran a doc audit on my own sections and found 5 of 9 docs drifted. Jeff wants all three roles to audit their owned sections now.

## Your Sections (from System page badges)

- **Engineering & Quality**: API, TESTING, STARTUP_PROCESS, GALLERY-REFACTORING, SCALING_RDF_TRIPLES, GUARDRAILS, ACCESS_CONTROL_MATRIX
- **User Capabilities**: USER_CAPABILITIES, WORDPRESS-INTEGRATION, BOOK_CATALOG_WORKFLOW

## What To Do

For each doc in your sections:
1. Check last modified date
2. Compare against what changed since then (new routes, new pages, test count changes, build pipeline changes)
3. If stale → update it
4. If stable reference (hasn't drifted) → skip

## Known Stale Candidates

- **API.md** (Feb 27) — recent, probably ok. Check if SMS webhook or new routes added since.
- **TESTING.md** (Feb 21, 10d) — test count was ~2300, now 2390+. Frameworks or patterns may have changed.
- **USER_CAPABILITIES.md** (Feb 21) — voice analytics, attention analytics, Reflect, mind map, navbar restructure all shipped since. Major drift likely.
- **STARTUP_PROCESS.md** (Feb 21) — deploy time went from 121s → 24s. Build process changed (eliminated in-container builds).
- **GUARDRAILS.md** (Feb 21) — deploy guard changed, write-scrubber updated, sensitive-paths narrowed.

## Card

This feeds into #763 (doc-drift gate). We're proving the audit manually today.
