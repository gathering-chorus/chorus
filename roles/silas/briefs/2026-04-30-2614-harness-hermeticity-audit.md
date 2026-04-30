# #2614 — Harness Hermeticity Audit (silas-half of #2613)

**Card:** #2614 (silas-half of split from #2612 nightly regression triage)
**Started:** 2026-04-30
**Owner:** Silas
**Status:** WIP

## Inventory (AC1)

**39 test files, 187 occurrences** of hardcoded `silas`/`wren`/`kade` role identifiers across:
- `platform/tests/` (bats/sh/ts test fixtures)
- `platform/services/chorus-hooks/tests/` (Rust integration tests)
- `platform/services/chorus-inject/tests/` (Rust unit + integration tests)

Top concentrations:
- `chorus-inject/tests/hermetic_runner_dispatch.rs` — 19 (hermetic FakeRunner pattern, classified `(c)`)
- `chorus-hooks/tests/chat_suite.rs` — 16 (writes real /tmp/chorus-chat/ files, `(b)`)
- `chorus-hooks/tests/perf_suite.rs` — 10 (curl POST to messaging API, `(b)`)
- `chorus-hooks/tests/spine_events.rs` — 2 (writes /tmp/session-context-kade.md + /tmp/voice-inbox/kade, `(b)`)

Full per-file count at `/tmp/2614-inventory.txt`.

## Classification (AC2) — heuristic-first pass

Classification grid per the card's three categories:
- **(a) intentional axis-4 integration test** — gate behind `RUN_INTEGRATION`
- **(b) accidental live-role leak** — tests writes to real role paths/APIs while running
- **(c) test of role-routing logic, legitimate** — uses synthetic state (FakeRunner, tempdir), no real-state mutation; role-name strings are inert data

### (b) Confirmed live-role-leak (8 files) — MUST gate or fix

Heuristic: file references real role state paths (`/tmp/voice-inbox/`, `/tmp/session-context-`, `/tmp/chorus-chat`) OR real APIs (`localhost:3475` messaging, `localhost:3470` bridge):

1. `chorus-hooks/tests/chat_suite.rs` — `run_chat(["start", "silas", "wren", ...])` creates real `/tmp/chorus-chat/silas-wren-*.md` files
2. `chorus-hooks/tests/spine_events.rs` — writes `/tmp/session-context-kade.md` + `/tmp/voice-inbox/kade/pending-inject.txt`
3. `chorus-hooks/tests/nudge_suite.rs` — exercises real nudge path
4. `chorus-hooks/tests/perf_suite.rs` — curl POST against real messaging API
5. `tests/features/chat/chat-flow.feature` — Cucumber chat flow
6. `chorus-hooks/tests/werk_version_single_source.rs` — needs read; flagged by heuristic
7. `chorus-hooks/tests/session_opening_narrative.rs` — needs read; flagged by heuristic
8. `chorus-hooks/tests/integration.rs` — needs read; flagged by heuristic

### (c) Likely hermetic (4 files) — document and leave

Heuristic: uses `FakeRunner`, `tempdir`, `TempDir`, `env::temp_dir`, AND no real-state references:

1. `chorus-hooks/tests/session_principle_inject.rs`
2. `chorus-inject/tests/hermetic_runner_dispatch.rs` — confirmed via read; FakeRunner exclusively
3. `chorus-hooks/tests/nudge_origin_tag.rs`
4. `chorus-hooks/tests/session_init_gate_binary.rs`

### Unclassified (27 files) — closer read needed

Don't fit either heuristic; need per-file inspection. List:
```
platform/tests/git-queue-push.bats
platform/tests/session-indexer-incremental.bats
platform/tests/mcp-nudge.test.sh
platform/tests/spine-e2e.sh
platform/tests/bridge-subscriber-watchdog.bats
platform/tests/catalog-curation-api.test.sh
platform/tests/close-out.test.sh
platform/tests/git-queue-branch-check.bats
platform/tests/doc-inventory.test.sh
platform/tests/features/memory/conversation-recall.feature
chorus-hooks/tests/session_start_additional_context.rs
chorus-hooks/tests/deploy_role_settings.rs
chorus-hooks/tests/context_cache_slim.rs
chorus-hooks/tests/escaping_and_git.rs
chorus-hooks/tests/tdd_gate_acceptance.rs
chorus-hooks/tests/ops_awareness_timeout.rs
chorus-hooks/tests/session_init_gate_recovery.rs
chorus-hooks/tests/demo_gate_removal.rs
chorus-hooks/tests/demo_gate_env.rs
chorus-hooks/tests/demo_gate_proven.rs
chorus-hooks/tests/role_state_writer_invariants.rs
chorus-hooks/tests/session_close_event.rs
chorus-hooks/tests/session_start_pulse.rs
chorus-hooks/tests/role_state_card_clear.rs
chorus-hooks/tests/mcp_client_session.rs
chorus-hooks/tests/pulse_snapshot.rs
chorus-inject/tests/inject_integration.rs
chorus-hooks/tests/pulse_service.rs
chorus-hooks/tests/manifest_envelope.rs
chorus-hooks/tests/hook_false_positives.rs
```

## Architectural finding (worth carrying)

Production code paths like `/tmp/voice-inbox/<role>/`, `/tmp/session-context-<role>.md`, and `/tmp/claude-team-scan/<role>-declared.json` are **hardcoded constants** in `chorus-hooks/src/shared/state_paths.rs` — not env-overridable. This means even synthetic-role-name strategies fail if the test mutates these paths, because the *same path constant* is used by the real daemon.

Per memory `feedback_eliminate_runtime_dep_dont_manage_it`: the long-term fix is to parameterize these paths (env var or constructor argument) so tests can redirect to a tempdir while production uses `/tmp/voice-inbox/`. Audit-card scope is to gate the leaks; parameterization is a follow-on (filed inline as a note here, NOT as a separate card per drowning-in-cards discipline).

## Fix strategy (AC3 + AC4)

**This session:**
1. Add `RUN_INTEGRATION` gate helper in `chorus-hooks/tests/common/` (new) or as a top-of-file helper in each leak test. Tests that hit real role-state skip with explicit message unless `RUN_INTEGRATION=1`.
2. Apply to `spine_events.rs` (the named leak from #2612) as proof-of-pattern.
3. Mark the helper-skipped tests with `eprintln!("SKIP: axis-4 ...")` so the skip is visible in cargo output.
4. Run `cargo test -p chorus-hooks` without `RUN_INTEGRATION` → tests skip cleanly, no real-state mutation.
5. Run `cargo test -p chorus-hooks` with `RUN_INTEGRATION=1` → tests run, must pass.

**Follow-on commits on this branch (still card #2614):**
6. Apply gate to remaining 7 confirmed-leak files.
7. Inspect + classify the 27 unclassified files; apply gate where needed.
8. Wire `RUN_INTEGRATION` into pre-commit hook (Check 4.66 or similar) so axis-4 tests don't fire by default — explicit opt-in for integration runs.

## What stays out of scope (per drowning-in-cards)

- Path parameterization (constants → env-overridable). Notable architectural debt; document here, file later when there's a forcing function.
- Cucumber step-definition audit — Kade-half (#2613). Pair on straddlers per AC5.

## Demo plan

When AC complete: show `cargo test -p chorus-hooks` output before/after the gate is wired. Before: 8 tests racing live state, occasional flake. After: 8 tests cleanly skipping with `axis-4` reason, zero real-state mutation. Then `RUN_INTEGRATION=1 cargo test -p chorus-hooks` runs the full set with deterministic pass.
