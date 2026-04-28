# Wren — Next Session

## Last session (2026-04-27)

**Shipped 10 KM cards in one session.** Curation surface stands up:
- #2511 link audit · #2510 inventory cleanup · #2445 catalog relocation · #2518 catalog retirement · #2519 ADR reconciliation · #2517 test gaps · #2520 tagging pass · #2521 hierarchy tree UI · #2522 coverage trend · #2531 zero-test triage

**Key outcome:** doc-catalog moved to chorus-api at :3340; flat 300-row list became Athena-driven tree; 99% product / 16% subdomain coverage with drift detector live; misfiled count 57 → 0 (43 docs relocated, 14 stale ADR dupes deleted).

## Open follow-ons (filed during session)

- **#2533–#2538** — 6 tests-needed cards (commits/toolchain/alerts-monitors/messages/properties/heralds) — Silas/Kade
- **#2539** — daily LaunchAgent for doc-tag-coverage-snapshot — Silas
- **#2541** — Clearing Domains panel empties when gathering restarts (Vikunja LaunchAgent under com.gathering, should be com.chorus) — Silas
- **#2543** — flaky fitness-summary test blocks unrelated commits — Kade
- **#2544** — chorus-api wraps Vikunja behind /api/cards (front door for cards data, same pattern as #2445 for catalog) — Wren
- **#2545** — doc-catalog curation workflow (untagged + drift findings become fixes from the page) — Wren ← **biggest next leverage**

## Next-up sequence (KM)

1. **#2545** curation workflow — turns the catalog from viewing surface to working surface
2. **#2544** chorus-api /api/cards front door — substrate fix
3. **#2541** Clearing resilience — folds into #2544
4. **#2469 / #2314 / #2318 / #2316 / #2152** — old Wave 2, now feeds #2545

## Demo / acp queue

- **#2118** (Kade) — gates green, awaiting Jeff's /acp

## What next session should know

- Curation workflow (#2545) is the lever Jeff named: "this becomes a tool you use, not just one you admire." Sequence it first.
- Vikunja LaunchAgent rename (com.gathering.vikunja → com.chorus.vikunja) is structural — folds into #2544.
- KM Wave 1 + Wave 2 both done. The corpus is honest. The next leverage is workflow, not data.
