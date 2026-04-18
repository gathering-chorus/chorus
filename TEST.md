# Chorus Test Modes

Every test in chorus runs in one of **two modes**. Both modes hit **zero errors, zero skips** as the standard of "good." Any deviation is a bug to fix or a test to delete — not a state to live in.

## The Two Modes

### Hermetic (default)

```
cd <project> && npx jest
cargo test        # Rust side
```

Hermetic tests:
- Execute against fakes, fixtures, or in-process engines (e.g. [oxigraph](https://crates.io/crates/oxigraph) WASM for SPARQL).
- Never contact the network, the filesystem outside the fixture, or a running service.
- Complete in **single-digit seconds** for any project.
- Report **0 failures and 0 skips** on every invocation.

The default `jest` / `cargo test` command runs this mode only. It is the per-commit, per-pair, per-gate feedback loop.

### Integration-gated

```
RUN_INTEGRATION=true npx jest
```

Integration tests:
- Contact real services (Fuseki on `:3030`, chorus-api on `:3340`, etc.).
- Verify end-to-end behavior that fakes can't cover.
- Opt-in via `RUN_INTEGRATION=true`. Never run by default.
- Run in a dedicated pre-merge CI lane, also to **0 failures and 0 skips**.

## Binary Rule — No Third Path

**Every test file is one mode, at the file level:**

- Hermetic suite → always runs, no env-var conditionals, no `describe.skip` inside it.
- Integration suite → gated by `RUN_INTEGRATION=true`, runs only in integration mode.

There is no **mixed file**. No `const d = process.env.RUN_INTEGRATION === 'true' ? describe : describe.skip` pattern. No `if (process.env.X)` branching between assertion sets. If a file contains both hermetic and integration concerns, split it into two files before committing.

Rationale: mixed files hide skipped tests from the default run. A reader of the test file can't tell which blocks ran without inspecting env-var state. The binary rule forces the split to the filesystem, where `grep -l "describe"` tells the truth.

Enforcement:
- `tdd_gate.rs` (hook): rejects new test files that violate the binary rule ([#2215](../../)).
- `test_quality_gate.rs` (hook): rejects tests without both an imported-production-symbol call and an assertion mechanism ([#2196](../../), [#2210](../../)).

## Fixture Pattern for Migration

The migration target for integration tests that can become hermetic is **[#2208's fixture pattern](../../)**:

- Checked-in TTL fixture (e.g. `tests/fixtures/athena-minimal.ttl`).
- In-process oxigraph SPARQL engine (`tests/fixtures/oxigraph-sparql.ts`).
- Golden JSON response files for assertions (`UPDATE_GOLDEN=true` regenerates).

A test that contacted Fuseki over HTTP can often be rewritten to hit oxigraph in-process against the fixture TTL, with the same assertions. Same coverage, hermetic shape.

## Honesty Metric

The integration-mode test count **shrinks over time** as hermetic rewrites land. Today's baseline (2026-04-18):

| Project           | Hermetic (default) | Integration-mode | Failing |
|-------------------|-------------------:|-----------------:|--------:|
| platform/api      | 778                | 346              | 50      |
| directing/clearing| 390                | 9                | 2       |

Target: integration-mode count → 0 for any project where the integration tests have hermetic equivalents. Projects with legitimately non-hermetic concerns (e.g., network reachability probes) keep a small integration suite; that count is the "real" integration surface.

When a card lands that migrates an integration test to hermetic, the integration count drops. That drop is the measurable delta — not a vibe, not a claim.

## "Quarantined" is Not a State

A failing or skipped test is never "quarantined until someone gets to it." It is either:

- **Fixed** — assertion passes, runs in the right mode.
- **Deleted** — with an explicit commit message stating what it was asserting and why the assertion no longer matters (coverage moved elsewhere, behavior changed, test was never real).

`jest.config.js` `testPathIgnorePatterns` and `describe.skip` are **not** places for dead tests to live. They are configuration for the binary-rule split — nothing else. A test file in `testPathIgnorePatterns` behind `RUN_INTEGRATION=true` is running in integration mode; it had better pass when that mode runs.

## Card Lineage

This document was shaped in chats `silas-kade-1776550938` and `silas-wren-1776551473` on 2026-04-18 and filed as [#2213](../../). Downstream work:

- [#2214](../../) — Integration triage spike: classify the 48 failures + 346 skips into fix / toss / migrate-to-hermetic; produce a table with recommendation per file.
- [#2215](../../) — Binary-rule gate extension of `test_quality_gate.rs`: reject env-var conditionals inside a `.test.ts` file.
- [#2216](../../) — `test.each` parser coverage for the quality gate.

## When This Document Changes

- A new mode is introduced (currently unlikely; the goal is fewer modes, not more).
- The binary rule is amended (requires explicit discussion and gate update).
- The honesty metric's baseline numbers shift materially (re-run and update the table).

This document does not change when:
- A single test migrates hermetic — that's just running the migration.
- A project adopts the pattern — the mode contract already applies by default.
- Someone wants to "quarantine" something temporarily. That path is not available; fix or delete.
