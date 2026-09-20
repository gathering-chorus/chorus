# Agent migration validation record

Source baseline: `90488c29`; implementation branch: `codex/agent-runtime-migration`.
Validation ran on macOS against an isolated checkout and temporary state. New
supervisor verification used the repository-pinned Rust **1.97.1**; TypeScript
dependencies came from committed lockfiles. No role deployment, live model call,
installed runtime conformance attestation, or multi-day canary was performed.

| Area | Verified result |
| --- | --- |
| `chorus-agent` | 23 tests: bounded workers/jobs, private provenance, process-group cancellation, shared concurrency locks, identity, leases, message dedupe, journal replay, handoff recovery, UDS/Pulse claim/ack, managed ingress, human approval refusal |
| MCP | 286 tests across 16 suites; TypeScript build |
| Pulse | 209 tests across 20 suites; TypeScript build |
| API | 2,045 passed / 14 skipped, 218 passed suites / 2 skipped; TypeScript build |
| Clearing | 190 targeted provider, canonical-history, transcript, participant, reply and router tests; TypeScript build; targeted ESLint |
| Runtime workers | 10 deterministic HTTP/ACP/provider tests; TypeScript build; targeted ESLint |
| Native policy/runtime unit tests | 14 passed |
| Native hook CLI acceptance | 6 passed, including multi-path denials, Unicode delivery, missing enrollment and broken-output/no-ack |
| SessionCache / memory / log gates | 8 / 16 / 8 passed |
| Session response debt | 21 passed, including sibling-session separation and missing evidence |
| `chorus-awake` | 32 passed, including configured dispatch and no fallback after supervisor failure |
| `werk-demo` | 77 library tests, including profile routing, pipe saturation and descendant timeout |
| Ops analysis | 147 passed when run serially |
| Configuration / build fixtures / baseline | 13 Python tests with Node available; no skips |
| Repository hygiene | `git diff --check`, new-crate rustfmt check, JSON parsing, workflow YAML parsing |

The ops suite has process-global `CHORUS_OPS_LOCK` fixtures that interfere when
run in parallel; the full serial run passed. The API integration skips are the
repository's existing opt-in/live-service tests, not hidden test failures.

## Reproduce core checks

Use the repository's normal toolchain setup and run from this checkout:

```sh
cargo test --locked --manifest-path platform/services/chorus-agent/Cargo.toml
python3 -m unittest discover -s platform/scripts/tests -p 'test_agent_*.py'
```

Run `npm ci`, `npm run build`, and each package's test command in MCP, Pulse,
Clearing and `platform/agent-adapters`. Build Clearing before testing the API-mode
worker because it imports the shared text-generation module. The new CI contract
job records the focused provider/cursor/worker/supervisor sequence. Existing
scheduled/manual CI triggers remain unchanged.

Native hook tests use the `runtime`, `session_cache`, `nudge_drain`, `memory_gate`
and `log_first_gate` filters plus `--test runtime_hook_cli` in `chorus-hooks`.
Run `ops::tests` with `--test-threads=1`. Tests launch fake temporary processes and
local sockets; a restrictive sandbox must permit those local fixtures.

## Isolated API regression run

The API's default global setup backs up the user's live index. For this run, a
temporary Jest config imported `platform/api/jest.config.js`, removed only
`globalSetup` and `globalTeardown`, and retained its project selection while
setting rootDir explicitly to the isolated API checkout. `RUN_INTEGRATION` was
unset. No deployment database backup/restore was invoked.

`CHORUS_ROOT` pointed at the checkout. `CHORUS_DB_PATH`, `CHORUS_LOG_FILE`,
`CHORUS_PULSE_PATH`, and `CHORUS_AGENT_SOCKET` pointed to dedicated temporary
paths. API, Fuseki, Loki and JWKS URLs pointed to unused loopback port 1. The run
used `node node_modules/jest/bin/jest.js --config <temporary-config> --maxWorkers=2
--forceExit`. The local `better-sqlite3` 12.9.0 native dependency was the same
version already built for Pulse; no source or lockfile was changed for it.

## What these checks do not establish

Fake-peer tests verify Chorus's parsing, state transitions, routing, authorization,
failure handling and wire contracts. They do not prove an upstream runtime loads
the configured hooks, an endpoint supports a useful coding loop, local permissions
provide OS isolation, or a deployed model meets review quality requirements.

Complete the deployment baseline, strict-MCP migration, credentialed card workflow,
provider capability probes, native/app surface checks, signed install, latency
comparison and five-day/ten-card canary in the [runbook](agent-runtime-migration.md)
before production promotion. Profiles deliberately remain opt-in and uncertified.
