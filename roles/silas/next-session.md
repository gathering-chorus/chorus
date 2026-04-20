# Next Session — Silas

## WIP
- **#2311** — Boot-time protocol contract. Header format collapsed to single slot (`Werk vN` from manifest.json). NOT closed — real gate is fresh three-role reboot showing same Werk v215 in all three terminals.

## What happened this session
- Jeff: hour of watching roles stamp `Werk v?`, `Werk v215 | chorus-prompt/2.1`, made-up values, and treat a two-slot header as if both slots were real. Called out the bad-data-next-to-good pattern that #2311 was supposed to prevent but instead *created* (the card itself added the redundant `chorus-prompt/X.Y` slot).
- Edit: `designing/claudemd/shared/chorus-prompt.md` now defines header as `--- Role | time Boston | #Card | Werk vN ---`. One slot. Sourced from manifest.json via session-start line 1. Explicit "never typed, never guessed."
- Regenerated all three CLAUDE.mds via `claudemd-gen`.
- Updated `.protocol_test_vectors.json` `live_core` hash to match new core.
- Protocol contract suite: 20/20 green.

## Real gate (not yet cleared)
Fresh Wren + Kade boot must show `Werk v215` in their first-response headers, matching Silas. If any role stamps a different value or drops the slot, #2311 remains WIP. Do not claim ship before observing three matching headers.

## Open pattern to watch
Jeff's words: "bad data next to good data." Applies broadly, not just headers. Any time a fix adds a new value without retiring the stale one, it's the same shape. Card #58 (wire Werk version through build — DEC-036) is the systemic version; pulling it would end the whole class.

## Alerts still fired today
crawler-failure, fuseki-harvest-stale, index-freshness (critical=1), lancedb-stale, vikunja-auth-failure. None triaged this session.
