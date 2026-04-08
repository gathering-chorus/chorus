# /demo — Product Template Conformance

Second conformance test of the Chorus product template (first: Cards #1807).

## Template Checklist
- [x] gates/ — preflight.sh, done-gate.sh, provenance.sh (gate logic owned by product)
- [x] tests/ — gates.bats, 6 scenarios, all green
- [x] alerts/ — demo-health.md (gate failure rate, skip rate, provenance gap)
- [x] domain-context.md — owner, gates, spine events, dependencies, contract
- [x] RUNBOOK.md — health check, test, common failures, rebuild
- [ ] logs/ — not applicable (demo has no runtime log output)
- [ ] src/ — gate logic is shell scripts in gates/, not compiled source

## What This Proved
1. Gate logic can live with the product owner (shell scripts) while infrastructure (chorus-hooks) dispatches
2. Product owner can fix their own gates without a Rust compile cycle
3. The dispatch contract (exit 0/1 + stderr) is simple enough for any language
4. 658 lines of Rust gate logic → 240 lines dispatch + ~180 lines shell = same behavior, clear ownership

## Pattern for Other Products
Any product with gates: write gate logic as scripts in `<product>/gates/`, chorus-hooks dispatches via exit code contract. Product owner tests with bats. No coupling to hook binary internals.
