# Brief: Data Migration Sequencing Plan — Review Requested

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-03-01 19:15 Boston
**Priority:** P1 — read before starting work tomorrow

## Context

Jeff asked for a sequenced, repeatable plan for data migrations. Tonight I validated all harvest pipeline claims against live systems — several were wrong (see earlier brief). Jeff is skeptical of unverified assertions and wants a gated approach.

## Full Plan

Read: `architect/briefs/2026-03-01-data-migration-sequencing-plan.md`

## Summary

Three phases, gated:
1. **Complete Music** (1 session) — Jeff re-exports XML → re-harvest → artwork backfill → verify
2. **Photos Pipeline Maturity** (2 sessions) — verify existing, script the pipeline, diff Bedroom sources, import one-at-a-time
3. **Remaining Domains** (notes, stories, facebook, linkedin, sexuality) — lower priority, incremental

Operating rules: one domain at a time, local rsync only (no SMB), verify before next step, manifest is truth.

## What I Need From You

**Answer these 5 questions** (comment on the brief or write a response brief to `architect/briefs/`):

1. **Photos extract** — is `harvest run photos extract` fully scripted end-to-end, or does it require manual steps?
2. **Artwork backfill** — does `backfill-artwork.ts` pick up where it left off (resume from album 2,001), or does it re-scan all 4,965?
3. **Photos thumbnails** — is there a thumbnail generation step for the photos pipeline? Music has cover art; what does photos browse use?
4. **150 failed music imports** — have you investigated the failures in `/tmp/music-import-remote.log`? Format issues, missing files, or API errors?
5. **Photos source diff** — can we reuse the `music-source-diff.js` pattern for photos, or is the data shape too different (EXIF vs XML)?

## What Changed Tonight

- Redundant SMB rsyncs killed (serial rsync was already done — 148GB, all 3 steps exit 0)
- Harvest pipeline now observable: Prometheus exporter, 3 alert rules, boot hook summary, `harvest sync-board`
- Your scope cards (#436, #437) have auto-synced status comments
- Verified artwork count: 1,746 missing (not 2,965)

Don't start new migration work until we align on this plan.

---

## PM Addendum (Wren, 2026-03-02)

Silas's plan and observability work are solid. Adding one process rule that makes this stick:

### Harvest WIP Discipline

**When a harvest card is in WIP, no feature cards for that role until the current stage gates.** This is the forcing function Jeff asked for. The pattern that keeps breaking harvest flow: Kade starts a pipeline stage, a feature card arrives, harvest loses context, nobody notices for days.

Silas built the 48-hour staleness alert — but if Kade has already context-switched to a feature card, the alert fires to the wrong attention. The fix is upstream: don't let the context switch happen mid-stage.

**Concretely:**
- Harvest card in WIP = that role's WIP slot is occupied (DEC-051, DEC-056)
- Feature cards queue in Next until the current harvest stage completes or explicitly blocks
- "Blocks" means a real dependency (Jeff needs to re-export XML, Bedroom drive offline) — not "I'll come back to it"
- If a harvest stage will take multiple sessions, note expected duration in the card description

### Boot Visibility

The `werk-init.sh` harvest summary should call out **your** stale domains specifically, not just show a general health line. When Kade boots and sees "music: extract stage stale 3 days" that's a direct signal. Ask Silas to scope the boot summary to role ownership.

### Answer Silas's 5 Questions

These are specific and answerable. Write a response brief to `architect/briefs/` before starting any harvest work. Alignment first, then execute.
