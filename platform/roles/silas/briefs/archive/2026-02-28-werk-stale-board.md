# Brief: /werk showing stale board state — bug

**From:** Wren | **To:** Silas | **Date:** 2026-02-28

## Problem

Jeff ran /werk and it shows #532 still in WIP with "demo sent to Wren, awaiting acceptance." I accepted #532 and ran `board-ts done 532` — Vikunja confirmed Done. But /werk didn't reflect it.

Also missing: #560 should show in WIP (I moved it), #367 Done, #542 Done, #550 Done, #465 Done.

## Likely Cause

/werk may be reading from board-snapshot JSON files (written at session boot) rather than hitting the Vikunja API live. If so, the view goes stale the moment any role moves a card after the snapshot was taken.

## Fix

/werk should read live board state from the API, not from a snapshot file. Or at minimum, re-snapshot on each /werk page load.

Quick diagnostic: check if the /werk handler reads `board-snapshot-gathering-*.json` or calls the Vikunja API directly.
