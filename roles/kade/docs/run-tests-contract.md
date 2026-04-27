# run-tests — contract reference

`platform/scripts/run-tests` is the domain+category+budget aware test runner for Phase 2 of the CI harness disconnect. This doc is the canonical contract; CI will call this exact shape from `.github/workflows/quality.yml` after #2528 wires it.

Source: [`/docs/designing/ci-harness-disconnect-plan.html`](http://localhost:3000/docs/designing/ci-harness-disconnect-plan.html) (Phase 2 contract section).

## Invocation

```
run-tests --domain=<sd-id> --category=<list> --budget=<seconds>
```

All three flags are required. Missing or unknown flag → exit 3.

## Flags

| Flag | Spec |
|---|---|
| `--domain=<id>` | Subdomain ID (e.g. `cards-service`, `chorus-domain`). Resolved via `POST /api/athena/discover-tests` data — graph wins iff non-empty AND discover-tests ran within 24h. Else filepath heuristic. |
| `--category=<list>` | Comma-separated union. Values: `hermetic`, `integration`, `contract`, `smoke`, `perf`, `mutation`. Filename-suffix-driven (`*.test.ts` = hermetic, `*.integration.test.ts` = integration, etc. — convention from #2524). |
| `--budget=<seconds>` | Hard timeout. Runner halts the suite at budget, sets `partial:true`, exit code 2. |

## Exit codes

| Code | Meaning |
|---|---|
| `0` | All ran, all pass |
| `1` | All ran, any fail |
| `2` | Budget exceeded — partial run; `partial:true` and `tests_unrun:[]` populated |
| `3` | Invocation error — bad domain, bad category, missing flag |

## Stdout (structured JSON)

```json
{
  "pass": 3,
  "fail": 0,
  "runtime_ms": 120,
  "partial": false,
  "failing_tests": [
    {"name": "test description", "file": "tests/foo.test.ts", "error": "AssertionError: ..."}
  ],
  "tests_unrun": [
    {"name": "slow test", "file": "tests/slow.integration.test.ts"}
  ],
  "hermeticity_tags": {"network": false, "fs": "tmp-only"}
}
```

- `partial: true` ⟺ exit code 2.
- `failing_tests` is a list of `{name, file, error}` — strings, not nested objects.
- `tests_unrun` populated only when `partial:true`.
- `hermeticity_tags` reports per-suite hermeticity properties Silas's Phase 0 audit (#2523) declares.

## Stderr

Human-readable progress + failures. Suppressed in CI by default; surfaced on non-zero exit.

## Wave 1 status (this card)

Wave 1 ships flag parsing, exit codes, and the JSON shape. Real test dispatch (jest, cargo nextest, bats) lands in subsequent waves. Today the runner short-circuits to canned output via `RUN_TESTS_FAKE=pass|fail|budget`.

| Wave | Scope | Status |
|---|---|---|
| 1 | Flag parsing + exit codes + skeleton JSON + fake mode | ✓ shipped |
| 2 | Domain resolver — graph (24h fresh) → filepath fallback | open |
| 3 | Category filter + jest/cargo/bats dispatch | open |
| 4 | Budget timeout + partial reporting from real runs | open |

## Phase 1 vs Phase 2

Phase 1 (this card) ships the runner standalone — callable from CLI, NOT wired into CI. Phase 2 (#2528, Silas) wires it into `.github/workflows/quality.yml` and updates pre-commit hooks. Don't conflate.
