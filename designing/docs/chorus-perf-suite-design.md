# Chorus Perf Regression Suite — Design

**Silas, 2026-04-20. Card #2287. Sibling: `chorus-contracts.md`.**

## Why this exists

Today's nudge work (#2283) surfaced that a 20s latency regression lived in the code for 3 weeks with no alarm. Correctness tests passed — the functions returned the right values. But nobody was timing the functions, so an `lsof` call slipped into the hot path and nobody noticed.

The gate:code suite enforces test pass + warning budget. It does not enforce latency budgets. Add that, with a small number of tests and clear budgets, so a performance regression fails the gate the same way a broken test does.

## Principles

1. **Measure only what's deterministic in cargo test.** No osascript, no live Terminal, no network to remote services. The in-process parts of each chorus service are measurable and reproducible.
2. **Set budgets from measured baselines with 2-3x headroom.** Tight enough to catch real regressions, loose enough to avoid flakes.
3. **One test per service, not twenty.** Signal over coverage. A flaky perf test is worse than no perf test.
4. **Fail with the number.** `"nudge dry-run took 2500ms, budget 500ms — 5x regression"` is self-documenting.
5. **Env is pinned.** Every test runs with `DEPLOY_ROLE=silas` set. Per Jeff's direction: `DEPLOY_ROLE=unset` is a contract violation, not a test condition.

## Services covered

| Service | Budget | Measured baseline | What's timed |
|---------|--------|-------------------|--------------|
| nudge dry-run | 500ms | ~150ms | Full nudge binary with `CHORUS_INJECT_DRY_RUN=1` — persist + detect_sender + queue decision, no osascript |
| nudge persist | 100ms | ~30ms | curl POST to `/api/nudge` on Bridge (localhost:3475) |
| chat.sh say | 200ms | TBD | chat.sh say with `CHAT_DRY_RUN=1` — persist + tick marker, no nudge delivery |
| pulse assembly | 100ms | 628-761ms today (#2172) | chorus-hook-shim pulse subcommand — reads JSONL, writes /tmp/pulse-latest.json |
| spine emit | 50ms | ~90ms (chorus-log script) | chorus-log invocation — one event |

## Non-goals

- **Not measuring osascript inject.** ~1.3s is the macOS Accessibility floor. Tests can't shrink that. Live-session nudge latency (~1.5-1.9s) is that floor plus persist (~30ms). Budget covers the parts we control.
- **Not measuring end-to-end user experience.** A role can sleep-poll in a Bash tool call and make the chat feel slow (today's lesson). That's observability, not gate:code.
- **Not measuring chat polling cadence.** Cron tick frequency is a harness choice, not a code latency.
- **Not running on real-session state.** Tests provide their own env; they don't depend on `/tmp/pulse-latest.json` being populated by a live session.

## Test pattern

```rust
#[test]
fn <service>_under_<budget>ms() {
    let t = Instant::now();
    let out = Command::new("bash")
        .arg(SCRIPT_OR_BINARY)
        .args([...])
        .env("DEPLOY_ROLE", "silas")
        .env("CHORUS_INJECT_DRY_RUN", "1") // or CHAT_DRY_RUN, as appropriate
        .output()
        .expect("<service> must run");
    let elapsed = t.elapsed().as_millis();

    assert!(
        <expected-output-present>,
        "<service> must complete the expected action"
    );
    assert!(
        elapsed < <BUDGET>,
        "<service> took {}ms — must be <{}ms. (#2287)",
        elapsed, <BUDGET>
    );
}
```

Adding a new service: copy the pattern, set the budget, document the baseline. No framework plumbing.

## Contract enforcement (folded from C1)

`nudge.rs` currently defaults `detect_sender()` to `"jeff"` when `DEPLOY_ROLE` is unset. That's a silent compensation for a broken contract. Per `chorus-contracts.md` C1:

**Change:** `detect_sender()` returns `Err` (or nudge binary exits non-zero) if `DEPLOY_ROLE` is unset. Remove the `"jeff"` fallback. Remove the `nudge_without_deploy_role_does_not_hang` test — it tests a condition that shouldn't exist.

**Error message:** `"CONTRACT VIOLATION: DEPLOY_ROLE not set. Check session-start for the caller."`

## Integration with gate:code

`gate-code-tests.sh` runs `cargo test`. The perf suite is a regular cargo test file — it ships in the same run. No new tool, no new script.

Failure output names the service and the delta, so the builder can see which service regressed without scrolling the test list.

## Rollout

1. Write the design doc (this file).
2. Remove `nudge_without_deploy_role_does_not_hang` from `tests/nudge_suite.rs` — tests a deprecated condition.
3. Enforce DEPLOY_ROLE contract in `nudge.rs` — fail-loud.
4. Write `perf_suite.rs` with the five tests.
5. Verify gate:code runs them (cargo test picks them up automatically).
6. Measure live baselines for chat say, pulse assembly, spine emit — set budgets from measured data.

## References

- `chorus-contracts.md` — contract C1 (DEPLOY_ROLE)
- #2283 — nudge consolidation, where the lsof regression was caught
- #2172 — pulse hot-path latency card (target sub-100ms)
- #2286 — gate architecture (no-signature exemption, precedent for fail-loud pattern)
