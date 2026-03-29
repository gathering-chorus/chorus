# Brief: Priority Stack for Kanban

**From**: Silas (Architect)
**To**: Wren (PM)
**Date**: 2026-02-13
**Priority**: High — these are the agreed architectural priorities, need to be reflected on the board

## Context

Jeff and I aligned on the architectural priority stack today. He's asked that these be layered into the project kanban (https://github.com/users/WJeffBridwell/projects/1). Sequencing matters — these are ordered by risk and dependency, not just value.

## Priority Stack (agreed with Jeff)

### 1. Visibility Enforcement (ADR-003) — IN PROGRESS
Kade is already running. Steps 1-3 (ACL coverage, write path audit, migration audit) are prerequisites. Steps 4-8 are the build. ADR-003 is accepted with full test strategy.

**Kanban items needed:**
- ACL service test coverage to 90%+ (Kade, in progress)
- Write path audit for .acl files (Kade, next)
- Migration audit of existing .acl files (Kade, next)
- Build collectionVisibilityMiddleware (Kade, blocked by above)
- Middleware unit + integration tests (Kade, with middleware)
- Wire into routes + E2E tests (Kade, after middleware)

### 2. Pod Data Backup — NOT YET ASSIGNED
Highest-risk non-functional gap. Turtle files are the source of truth with no backup. Disk failure = permanent loss of the knowledge graph. Infrastructure work — can run in parallel with Kade's ADR-003 build.

**Kanban items needed:**
- Automated pod backup (cron + tar to second location as minimum viable)
- Backup verification (restore test)

### 3. Fuseki TDB2 Verification — SILAS, NEXT SESSION
Quick investigation: confirm Fuseki is using TDB2 persistent storage, not in-memory. If in-memory, everything we're planning for scale (15-25M triples) is on a foundation that won't hold.

**Kanban items needed:**
- Verify Fuseki storage configuration (Silas, quick)
- If needed: migrate to TDB2 persistent storage (Kade, sized after investigation)
- Baseline benchmark at current triple count (Silas)

### 4. CI Pipeline Enforcement — AFTER ADR-003 SHIPS
Test failures should block the pipeline, not pass with `|| echo`. Security-critical middleware is being built; the pipeline should catch problems.

**Kanban items needed:**
- Remove permissive test execution from CI/CD
- Verify coverage thresholds are enforced
- Evaluate whether coverage threshold should increase given new security code

### 5. Visualization Tooling (ADR-004) — AFTER FOUNDATION IS SOLID
Jeff needs to see the shape of his data. Three layers: YASGUI in dashboard, WebVOWL for ontology, vis.js for data exploration. ADR-004 is drafted.

**Kanban items needed:**
- Embed YASGUI in admin dashboard (replaces SPARQL textarea)
- Self-host WebVOWL for ontology browsing
- vis.js graph renderer on collection pages (larger effort)

### 6. First External Harvester (beyond WordPress) — BLOCKED
Proves the harvest pipeline pattern at scale. Blocked by Jeff filling in ingestion depths on the content ingestion matrix.

**Kanban items needed:**
- Jeff: decide ingestion depths for Google Photos, music, social (content-ingestion-matrix.md)
- Design harvest pipeline adapter pattern (Silas)
- Build first harvester (Kade, after above)

### 7. Conversational AI Layer — FUTURE
Architecturally necessary for curation at scale, but the graph needs more content to curate first. Right thing, not right time.

**Kanban items needed:**
- Scope and design (Silas, when graph has sufficient content)
- Build (Kade, after design)

## Notes for Wren

- Items 1-4 are foundation work. They strengthen what's built. Resist pressure to skip ahead to features until these are solid.
- Item 2 (pod backup) is the one I'm most concerned about. It's not exciting but it's the highest-risk gap. If you agree, please make sure it gets on the board and doesn't drift.
- Item 6 is blocked by a Jeff decision (ingestion depths). You may want to schedule that conversation.
- Jeff's meta-note: next session should tilt toward building, not documenting. We laid a lot of foundation docs today (capability map, ingestion matrix, conceptual model, glossary). The map is drawn — time to move.

## Also Pending

You have a second brief from me today: `2026-02-13-conceptual-model-glossary-review.md`. That's a review request for the conceptual model and glossary — lower priority than getting the kanban updated but important for shared language across roles.
