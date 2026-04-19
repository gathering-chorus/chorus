# Cards CLI Test Patterns

## Permutation Matrix (#2245)

`cards-api-permutation.test.ts` is a data-driven suite. It iterates `fixtures/cards-api-matrix.json` and runs each case against a `FixtureClient` seeded from `fixtures/cards-api-state.json`.

**Adding a new test case:** Edit `fixtures/cards-api-matrix.json` only. No test code changes needed.

Each matrix row:
```json
{
  "id": "move-valid",
  "args": ["move", "2", "WIP"],
  "stdoutMatch": "Moved",
  "expectCall": "move"
}
```

- `id` — unique name (shown in Jest output)
- `args` — CLI args after `cards`
- `stdoutMatch` — regex/string the joined stdout must contain (omit or null to skip)
- `expectCall` — BoardClient method that must have been called (omit to skip)

**Fixture state:** `fixtures/cards-api-state.json` — 3 tasks (WIP/Next/Done) representing a known board state. Reused across all matrix cases.

**Hermetic:** `FixtureClient` records calls, returns canned data. No real Vikunja connection, no network.

## Other test patterns

| File | Pattern |
|------|---------|
| `cli-runCli.test.ts` | MockClient + captureConsole, per-command dispatch |
| `client.test.ts` | Mocked HTTP, BoardClient parsing |
| `sdk-*.test.ts` | SDK function behavior, hermetic |
| `*-flow.test.ts` | Multi-step narrative scenarios |
| `*-bdd.test.ts` | Gate enforcement, BDD-style |
| `integration.test.ts` | Live Vikunja (RUN_INTEGRATION=true only) |
