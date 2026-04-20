# Chorus Contracts — Fail-Loud Pattern

**Silas, 2026-04-20. Copy-paste template linked from `chorus-contracts.md`.**

## The pattern

A contract violation at the binary entry point exits non-zero with an explicit error. The error names the contract, names the likely upstream cause, and tells the caller how to fix it.

```rust
// At the top of the binary's run() or handler:
match std::env::var("DEPLOY_ROLE") {
    Ok(role) if matches!(role.as_str(), "silas" | "wren" | "kade") => role,
    Ok(role) => {
        eprintln!(
            "CONTRACT VIOLATION: DEPLOY_ROLE={} is not a valid role \
             (expected wren/silas/kade). Check session-start or use --from.",
            role
        );
        return ExitCode::from(1);
    }
    Err(_) => {
        eprintln!(
            "CONTRACT VIOLATION: DEPLOY_ROLE unset. \
             Check session-start for the caller."
        );
        return ExitCode::from(1);
    }
}
```

## Rules for writing a fail-loud check

1. **At the entry point.** Don't compensate mid-function. Check the contract where the call enters your binary.
2. **Exit non-zero.** Calling scripts must be able to detect the failure via exit code.
3. **Stderr, not stdout.** Error messages go to stderr so they don't contaminate tool output piped elsewhere.
4. **Name the contract.** The error says "CONTRACT VIOLATION: X". The name is stable, searchable, and recognizable in logs.
5. **Name the likely cause.** "Check session-start" or "Use --from". Give the caller the next action.
6. **No silent defaults.** Don't invent a reasonable-looking value. A bad attribution in the log is worse than a hard stop.

## Anti-patterns

```rust
// WRONG: silent fallback masks the violation
let role = std::env::var("DEPLOY_ROLE").unwrap_or_else(|_| "jeff".into());
```

```rust
// WRONG: the wrapper compensates for a missing contract
// (in bash)
export DEPLOY_ROLE="${DEPLOY_ROLE:-jeff}"
```

Both of these produce "jeff" attribution when upstream is broken. The fix was to remove both.

## Testing the pattern

```rust
#[test]
fn fails_loud_when_contract_violated() {
    let out = Command::new("bash")
        .arg(SCRIPT)
        .arg(VALID_ARGS)
        .env_remove("DEPLOY_ROLE")
        .output()
        .expect("script must run");

    assert!(!out.status.success(), "must exit non-zero on contract violation");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("CONTRACT VIOLATION"),
        "stderr must name the violation: {}", stderr
    );
}
```

## Where this pattern applies

Per `chorus-contracts.md`:

- **C1** — DEPLOY_ROLE must be set (entry point: `nudge.rs`)
- **C2** — role must be wren/silas/kade (entry points: role-accepting binaries)
- **C3** — session_id required on hook input (entry point: hook `check()` functions that depend on session state)

C4 (cross-role filesystem reads) uses a different mechanism — a PreToolUse hook on Read/Glob/Grep — because the boundary isn't at a single binary entry point. See #2290.

## References

- `chorus-contracts.md` — the ledger
- #2287 — where C1 was enforced (nudge binary)
- #2290 — C4 enforcement (PreToolUse hook, Wren)
