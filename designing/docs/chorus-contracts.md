# Chorus Contracts

**Silas + Wren, 2026-04-20. Living doc.**

## Principle

A contract is a condition the system relies on. If it's violated, fail loud at the entry point — don't silently default, don't compensate, don't work around it. Silent fallbacks mask real bugs. Today's example: the nudge binary defaulted to `sender=jeff` when `DEPLOY_ROLE` was unset — which hid a 3-week-old 20s lsof regression because nobody noticed the "jeff" attribution was wrong.

The goal: make the right path the only path, not the preferred one.

## Contracts

### C1 — `DEPLOY_ROLE` must be set for any nudge or role-scoped call

**Owner:** Silas
**Enforced at:** `nudge.rs`, any hook that attributes a role
**Failure mode:** Non-zero exit with explicit error. No "jeff" fallback.
**Rationale:** A missing `DEPLOY_ROLE` means the calling context is broken — session-start didn't fire, or a script forgot to export. Silently attributing to "jeff" makes the broken state invisible.
**Status:** Pending implementation in #2287 AC.

### C2 — Role must be `wren`, `silas`, or `kade`

**Owner:** Silas
**Enforced at:** `role_dir()` in `nudge.rs`, `detect_sender()`, any spine event with a role field
**Failure mode:** Already enforced in `nudge.rs` via `role_dir()` returning `None` → "Unknown role" error. Audit other surfaces for same treatment.
**Rationale:** A stray role name in logs or state files corrupts search, gemba, and attribution.
**Status:** Partial — nudge covers it. Need audit of spine event emitters, role-state, inferred.json writers.

### C3 — Session ID required on every hook input

**Owner:** Silas
**Enforced at:** hook pipeline entry (shim.rs dispatch, or individual hook `check` functions)
**Failure mode:** Currently most hooks treat missing `session_id` as "skip gate" (return `allow()`). That's a silent compensation. Option: require it, or explicitly document which hooks are session-independent.
**Rationale:** Session-scoped checks (TDD gate, test-quality gate, pair gate) all depend on reading session JSONL. Missing session ID → checks are no-ops — the gate is pretending to work.
**Status:** Audit needed.

### C4 — Cross-role reads go through Chorus API (Bezos memo)

**Owner:** Wren
**Enforced at:** All role code, skills, scripts
**Failure mode:** No runtime enforcement today. Proposed: a lint/gate that flags direct filesystem reads of `roles/<other-role>/**` outside approved patterns.
**Rationale:** Roles reading each other's files directly bypasses the Chorus API, breaks the interface contract, and makes changes brittle. Every cross-role read should be an endpoint.
**Status:** Not enforced. Proposed for a follow-on card.

## Anti-contracts (things we explicitly don't enforce)

- **`sender=jeff` as default** — removed per Jeff 2026-04-20. Ambiguous attribution is worse than failure.
- **Silent inject fallback to queue on unknown role** — inject to an unknown role already errors; queue fallback is only for real inject failures (Terminal window missing).

## Pattern: how to enforce a contract in a hook

```rust
// Hook entry point
if std::env::var("DEPLOY_ROLE").is_err() {
    eprintln!("CONTRACT VIOLATION: DEPLOY_ROLE must be set. \
               Check session-start for the caller.");
    return ExitCode::from(1);
}
```

Fail-loud wrapper: exit non-zero, print to stderr with the contract name, name the likely upstream cause. The error message is documentation.

## References

- #2287 — chorus perf regression suite (includes C1 enforcement)
- DEC-107 — nudge force-always (related: no preferred path, only one path)
- DEC-093 — domain endpoints via Chorus API (related to C4)
