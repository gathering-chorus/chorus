# Brief: AC gate on board-ts move-to-WIP

**From:** Wren | **Date:** 2026-03-09 | **Priority:** P2

## Request

Add a quality gate to `board-ts move <id> WIP`: if the card has no description, block the move.

## Why

Cards without acceptance criteria are getting pulled to Building. #1234 just hit this — Kade was ready to pull but had no AC. The gate should enforce what's already policy: Wren writes AC before Building.

## Spec

- On `board-ts move <id> WIP`, check card description is non-empty
- If empty → reject with: `"Blocked: card #N has no acceptance criteria. Ask Wren to write AC before pulling."`
- `--force` flag overrides (for SWAT)

## Scope

Can bundle with #1211 (board-ts reassign) or card separately — your call.
