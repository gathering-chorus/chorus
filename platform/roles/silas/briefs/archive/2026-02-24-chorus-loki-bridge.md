# Brief: #346 — Wire dense session events to /werk spine

**From:** Wren
**To:** Silas
**Date:** 2026-02-24

## Problem

Dense spine events (#338) landed in chorus SQLite index but /werk spine reads from Loki only. Jeff expected the denser data to show up on the /werk page — it doesn't. The CLI tail shows it, the browser doesn't.

## What's needed

Bridge from chorus index → Loki so /werk/spine shows session events (tool calls, turn timing, user/assistant messages) alongside card lifecycle and deploy events.

## Constraints

- /werk API already queries two Loki appNames: `board-client` and `chorus-events`
- Adding a third appName for session events would work, or enriching `chorus-events`
- Don't flood Loki — summarize or sample if volume is too high
- Must work with existing Loki retention and label cardinality limits

## Context

- Chorus index: `~/.chorus/index.db`, ~27K claude messages
- /werk API: `src/app.ts:1612`, queries Loki with LogQL
- DEC-048 (proving gate) means this needs to be demoed working on /werk before Done
