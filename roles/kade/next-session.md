# Kade — Next Session

## Status
#1838 shipped (seed split-message race fix). Test suite clean — 205 unit suites, 18 integration, 10 E2E all green.

## This session (2026-04-09)
- **#1838** — Fixed seed split-message race. Apple Messages splits "link #kade" into two SMS segments; hashtag can arrive first. Added in-memory pendingHashtags buffer to SeedHandler. Prior fixes (4582bfa, ec97859) relied on persisted captures; #1937 AC3 broke that. Fixed false-green AC4 test. 5 new tests, 240 seed tests green.
- Fixed 6 clearing test paths broken by #2328 repo restructure (directing/ → chorus/directing/)
- Fixed quality-scanner CHORUS_ROOT (CascadeProjects/ → CascadeProjects/chorus/)
- Bumped drive-analysis e2e timeout 15s → 30s (Fuseki latency flake)
- Restarted sexuality-player on Bedroom via launchctl kickstart, installed LaunchDaemon for headless reboots
- Reviewed Silas's #1837 — flagged 2 broken test paths in alert-runner tests, both fixed
- Walked Jeff through seed pipeline architecture end-to-end

## Pick up
- **#1834** — Wire demo gate to `cards done`. P1 fix. No Done without demo evidence.
- **#1835** — Wren migrated 32 skills to chorus/skills/. I own /lc, /lm, /look, /ot, /share. Restart will load new paths.
- Backstage link seed (sms-1775730500614-p9fjps) routed to Wren pre-fix — Jeff may want re-routed to Kade

## Next card
- #1834 — Wire demo gate to cards done (P1)
- #1800 — Board test isolation (P1)
- #1619 — Provenance stamps (Next)

## Key decisions
- Seed hashtag buffer is in-memory only (not SPARQL) — persisting caused empty briefs per #1120
- 120s TTL matches existing correlation window
- Demo gate must fire on `cards done`, not just on `/demo` entry
