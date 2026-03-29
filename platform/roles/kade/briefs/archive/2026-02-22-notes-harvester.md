# Brief: Notes Harvester — Apple Notes / iCloud Ingestion

**From:** Wren
**To:** Kade
**Date:** 2026-02-22
**Card:** #95

## Context

Jeff's primary capture flow is texting himself ideas, which land in Apple Notes. Today during meditation he texted 6 ideas and had to manually screenshot and paste them into our session. That friction needs to go.

The Slack bridge just got cut (#123, you finished today). The intake pipeline needs a new source — Notes is the most important one.

## What's Needed

Build a Notes harvester that:

1. **Reads Apple Notes** — via AppleScript/JXA or the Notes SQLite database at `~/Library/Group Containers/group.com.apple.notes/NoteStore.sqlite`
2. **Incremental** — only fetch new/modified notes since last run, like the existing photo harvester pattern
3. **Outputs structured data** — title, body text, creation date, modification date, any attachments
4. **Integrable** — should be callable from session-start or a `/seed` skill so new notes surface automatically

## Constraints

- Local only — no iCloud API, read from the Mac directly
- Apple Notes SQLite is compressed (protobuf). May need to decode. Check what's available.
- If SQLite is too complex, AppleScript/JXA via `osascript` is the simpler path
- Don't harvest everything — Jeff will need a way to tag or folder notes meant for Gathering (vs personal notes)

## Coordination

- Silas is designing the overall capture flow architecture (#126) — your harvester plugs into that
- Start with the simplest thing that works: read notes from a specific Notes folder, output JSON

## Deliverable

Working script that reads Notes from a designated folder and outputs JSON. Tests. Brief back to Wren with what you found about Notes access on macOS.
