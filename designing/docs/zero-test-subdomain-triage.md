# Zero-Test Subdomain Triage (#2531)

**Date:** 2026-04-27
**Triggered by:** Kade's #2515 wave-1 test inventory audit — 14 of 48 Athena subdomains return zero tests across all four tested patterns (filename, owner-prefix, domain label, alias).

This document classifies each of the 14 into one of three buckets so Phase 0 of the CI Harness Disconnect plan can exit cleanly with the gap accounted for.

## Buckets

- **Knowledge surface, no tests by design** — content/data domains, not code. Coverage gap is correct; "tests" don't apply.
- **Tests-needed (future card)** — code surface that should have tests; not yet first-class. Not blocking now, but the gap is real.
- **Scope error** — subdomain shouldn't exist, or should merge. Handled separately (retire in graph or open conversation with Jeff).

## Classification

### Knowledge surface — no tests by design (8)

These are content/data domains. The "test" question doesn't apply: a doc can't fail; data quality is enforced by harvest discipline + drift detection (#2520, #2522), not unit tests.

| Subdomain | Why no tests |
|---|---|
| `books-domain` | Jeff's reading list. Content. |
| `documents-domain` | Document harvest from Drive/local. Data quality is the harvester's concern (separate cards). |
| `video-domain` | Video corpus. Content. |
| `blog-domain` | Blog posts. Content + WP integration tested separately. |
| `loom-rcas` | Incident records. Content; no behavioral surface to test. |
| `loom-analytics` | Aggregated analytics outputs. Numbers we collect, not code we run. |
| `loom-metrics` | Metric values. Same shape as analytics — emitted by code that lives in `chorus-domain` / `observability-domain`, both of which DO have tests. |
| `loom-practices` | Practice documentation. Content. |

**Action:** Accept as known-coverage-gap. No follow-on card required. Phase 0 exit math should treat these 8 as "no tests by design" rather than "missing tests."

### Tests-needed — future card (6)

These are code surfaces with no test coverage today. Filing follow-on cards is the right move.

| Subdomain | Owner | Note |
|---|---|---|
| `commits-domain` | Kade | Git/commit observability code. Should have tests. |
| `toolchain-domain` | Silas | Infrastructure tooling. Should have tests on critical paths. |
| `alerts-monitors-domain` | Silas | Alert/monitor pipeline. Tests on alert routing + dedup logic. |
| `messages-domain` | Silas | Clearing/Bridge messaging service. Tests on persist+deliver contract. |
| `properties-domain` | Silas | System properties tracking (not Jeff's property-domain). Code; tests-needed. |
| `heralds-domain` | Kade | Herald reflection pipeline. Tests on the herald contract. |

**Action:** File 6 follow-on cards (one per subdomain) tagged `chunk:tests`, `sequence:werk`, owned per the table above. Lower priority — these don't block current work, but they belong on the coverage roadmap.

### Scope error (0)

None of the 14 are scope errors. Each is a real subdomain that should exist; the question was just whether tests apply.

## Phase 0 Exit Math

Of the 48 Athena subdomains:
- 34 have tests (per Kade's #2515 audit)
- 8 are knowledge surfaces — accept as correct gap (this triage)
- 6 are tests-needed — filed as follow-on cards
- 0 scope errors

**Effective coverage gap closure: 100%** — every subdomain is either tested, accepted as no-tests-by-design, or carded for future test work.

## Connects to

- **#2515** (Kade) — surfaced the 14 via test inventory audit
- **#2523** (Silas) — hermeticity audit; Phase 0 exit can cite this triage
- **`ci-harness-disconnect-plan.html`** — Phase 0 exit AC for Kade's track
