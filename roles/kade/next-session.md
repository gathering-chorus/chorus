# Kade — Next Session

## Status
No code shipped this session. Research and coordination only.

## This session (2026-04-09)
- Researched #1800 (board test isolation) — card was stale. Renamed to "Isolate cards integration tests from live Vikunja", rewrote AC to match current state: 260/297 tests already isolated via jest.mock(), only integration.test.ts and board-validation.test.ts hit live Vikunja + SQLite contention
- Gemba on Silas #1839 — verified cards product has no stale LaunchAgent paths, confirmed daily-review-quality.sh exists at expected path
- Chat with Wren — #1834 (wire demo gate to cards done) ownership transferred to Wren. Demo gate is product coordination tooling, her domain. Pointed her to done handler in cli.ts → acceptCard() in sdk.ts

## Pick up
- **#1834** — Now Wren's card. She'll brief me on the integration point in cards CLI
- **#1800** — Isolate cards integration tests. AC updated, ready to pull
- **#1835** — Skills migration. I own /lc, /lm, /look, /ot, /share. Restart loads new paths

## Next card
- #1800 — Isolate cards integration tests (P1, updated AC)
- #1619 — Provenance stamps (Next)

## Notes
- chat-tick skill doesn't exist — don't use it in cron loops. Just read chat directly
- cards CLI lives at chorus/directing/products/cards/ (not board-client, not scripts/cards)
