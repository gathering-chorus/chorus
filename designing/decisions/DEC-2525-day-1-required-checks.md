# DEC-2525: Day-1 required-checks list for main (TS hermetic + Rust hermetic only)

**Date:** 2026-04-28
**Source:** Silas, on #2525. Plan precedent: `/docs/designing/ci-harness-disconnect-plan.html` (Phase 0 exit, declared after #2523 audit). Brief: `/docs/designing/ci-harness-research-brief.html` (sensor-vs-rule architecture).
**Status:** Active
**Landing note:** Filed as markdown pending the loom-decisions API (#2318). Migrate via API when shipped (per DEC-2311 landing pattern).
**Cards:** #2525 (this DEC), #2523 (audit that produced the verdict), #2524 (categorization that made hermetic addressable), #2526 (Phase 1 disconnect that consumes this list).

## Decision

The required-checks list on `main` branch protection is exactly two named jobs from the moment Phase 1 (#2526) ships:

| # | Required check | Source workflow | Filter |
|---|----------------|-----------------|--------|
| 1 | `jest — <package>` (hermetic only) | `.github/workflows/quality.yml` | `*.test.ts`, excluding `*.{integration,contract,smoke}.test.ts` |
| 2 | `cargo test — <crate>` (hermetic only) | `.github/workflows/quality.yml` | `tests/hermetic_*` + inline `#[test]` in `src/`; excluding `tests/integration_*` |

Every other CI job runs but does not block: lint-ratchet, type-check, MCP shape, catalog backstop, principle backstop, cargo clippy, MCP round-trip — all `continue-on-error: true` post-disconnect, surfaced via the nightly drift lane (#2527).

## Why

The CI substrate accumulated 8+ required checks because every quality concern got promoted to a blocking rule. Three concrete failure modes followed:

1. **Bypass-and-flake.** Required checks failed for non-hermetic reasons (network flake, env coupling). Agents learned `--no-verify` in days; humans context-switched.
2. **Cards-as-protection.** Card workflow accreted gates because the rules workflow couldn't be trusted.
3. **Architect-clean splits.** Multi-day work cycles to fix CI rules that were never load-bearing.

The harness brief reframed this: most "CI failures" we fought were rules failing to be useful, not sensors failing to detect. The fix is to keep most sensors and remove most rules. **Block on hermetic only.** Everything else surfaces as observability.

Two checks (jest + cargo, hermetic-only) is the minimum that:
- Catches "the build doesn't work."
- Catches contract regressions inside the package.
- Doesn't depend on Fuseki, Vikunja, network, time, or filesystem state.
- Has a 100/100 shuffled-runs verification window (per AC6 below) before going live.

## Criteria for adding a check to the required-list

A new required check must satisfy all of:

1. **Hermeticity-verified.** Passes the five hermeticity rules per the #2523 audit framework (no network / no fs outside tmp / no clock or random / no env coupling / order-independent). The audit must classify the check's underlying tests as `verdict=hermetic`.
2. **Consumer cited — named and current.** The PR that adds the check names a downstream consumer with a documented read pattern (role + cron / dashboard / workflow / alert). The consumer must already exist OR have an open card committing to land it within 30 days; "Wren will read this someday" is no consumer. Per Wren #2525 review: same shape as #2440's evidence move — data-of-truth lives where the consumer reads. Sensors without consumers are noise; the sensor consumer registry (#2528) is the enforcement surface.
3. **48h shuffled-runs window passed.** The check passes 100/100 shuffled-order runs across a 48-hour window before being added to branch protection. Mechanism: `jest --randomize` (#2532) for TS; `cargo nextest run --shuffle` for Rust once the workspace adopts nextest.
4. **DEC update.** Adding a required check requires updating this DEC (or filing a successor). Branch protection doesn't change without a named decision.

## Criteria for retiring a check from the required-list

A required check is retired when any of:

1. **Flake rate above 1%.** If the check fires red on PRs that subsequently merge green without code change at a rate exceeding 1%, it's retired. Flake = false-positive; false-positive blocks ≠ signal.
   - **Window tightens during disconnect tuning.** First 90 days post-Phase-1 land: 30-day rolling window. Math (per Wren #2525 review): at ~20 PRs/week, 1% = one false-positive every 5 weeks per check; during the disconnect's first window when we're actively tuning, a flaky check needs 30-day eviction to prevent the bypass-training failure mode the disconnect is meant to retire. After 90 days steady-state: 90-day rolling window.
2. **No active consumer.** Per the sensor consumer registry (#2528), if no consumer has read the check's signal in 90 days, retire.
3. **Hermeticity drift.** If the audit (re-run periodically) re-classifies the check's tests as non-hermetic, retire from required-list and route to integration tier.

Retirement is a DEC update — same governance as addition.

**Telemetry dependency.** The behavioral definition of "flake" — fires red on a PR that subsequently merges green without code change — requires per-check signal-tracking that does not exist today (Wren #2525 review). Until #2528 (sensor consumer registry) lands the necessary telemetry, retirement criterion #1 is unenforceable in the literal sense. Interim mechanism: manual review of the required-check job history at each retirement-decision moment, captured in the DEC update PR. #2528 is sequenced alongside #2526 so the data plane lights up when the disconnect lands; if #2528 slips, retirement criterion #1 becomes a deferred enforcement until the registry reads PR signals.

## Enforcement

- The Phase 1 disconnect script (#2526) cites this DEC in its commit message and in a comment on `.github/workflows/quality.yml` next to the required-check declarations.
- `gh api PUT /repos/<owner>/<repo>/branches/main/protection/required_status_checks` sets the contexts to exactly the two checks above.
- Future PRs proposing additions or retirements must cite this DEC and propose its update; bare workflow edits that add `required` without DEC change get rejected at code review.

## Out of scope

- The actual disconnect commands (#2526 owns).
- The 48h shuffled-runs verification window itself (AC6 of this card; runs after #2526 lands so the new required-checks set has something to verify).
- Adding clippy as a third required check (deferred per #2532 outcome — clippy ratchet currently advisory).
- Cross-language contract tests (#2200) — Phase 3 work.

## Related

- Plan: `/docs/designing/ci-harness-disconnect-plan.html` (Phase 0–3 sequencing)
- Brief: `/docs/designing/ci-harness-research-brief.html` (sensors-vs-rules architecture)
- #2523 — hermeticity audit (the verdict this DEC consumes)
- #2524 — test categorization convention (the suffix this DEC's filter uses)
- #2526 — Phase 1 disconnect (the consumer of this DEC)
- #2527 — nightly drift lane (where retired/non-required checks live)
- #2528 — sensor consumer registry (the "cite a consumer" enforcement surface)
- #2532 — clippy + shuffled runs (the detection mechanism for criterion #3 above)
- DEC-2311 — hook decomposition (vocabulary precedent: separate guard from sensor from observer)
- `chorus:principle-no-competing-implementations` — one implementation per concept (the disconnect's spirit)
