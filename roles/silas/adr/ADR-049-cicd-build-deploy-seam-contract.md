# ADR-049: The cicd ↔ build/deploy Seam Contract — "merged == this exact verified commit runs everywhere"

**Status:** Accepted (design ratified by Jeff 2026-06-20; Silas + Kade pair on #3517)
**Date:** 2026-06-20
**Deciders:** Jeff Bridwell · Silas (SA + build/deploy/toolchain) · Kade (cicd/version-control)
**Cards:** #3517 (the SA build) · governed-from-above by #3530 (toolchain governance) / #3531 (pipeline-runner standard) / #3532 (dependency-coupling map)
**Relates:** ADR-030 (act for werk pipelines) · ADR-036 (one build/deploy execution path)

## Context
The build/deploy engine fails at the **seam** between cicd + version-control (Kade) and build + deploy + toolchain (Silas): *"merged ≠ deployed."* Every facet-fix (the six folded into #3517) patched one domain's leg and the contract *between* them drifted, because no one owned the whole. A split designs the interface twice → two assumed contracts → the exact drift. So this ADR adopts **one contract**, designed once by the pair; each half builds to it.

The unifying frame: **every failure here is "claimed state ≠ verified state"** — cdhash="unknown", the run-store "running"-while-landed lie, deploy-from-slot-not-main. The contract makes **claimed == verified at each handoff, on ONE sha end-to-end.**

## Decision
Adopt the cicd↔build/deploy seam contract. The loop, by domain:
`merge→origin/main` (vc·Kade) → `trigger(landedCommit)` (cicd·Kade) → `build+deploy(landedCommit)` (Silas) → `verify(running==built for landedCommit)` (Silas) → `gate(landedCommit)` (cicd·Kade). **Same `landedCommit` throughout → merged≠deployed cannot exist.** All of it runs on the toolchain (Silas) — the floor.

### Handoff 1 — TRIGGER (cicd → build/deploy) · Kade
- EVENT = `werk-merge` SUCCESS landing a card on origin/main (not a local werk commit).
- PAYLOAD = `{ landedCommit: <origin/main HEAD post-merge>, card_id, changedUnits: [...] }`. `landedCommit` resolved from origin/main, never the role's slot (the snowflake fix). `changedUnits` = the dependency-driven deploy set.
- IDEMPOTENT: re-firing for the same `landedCommit` is a no-op when running==built.
- **Fires INLINE from werk-merge** (resolved fork), not an external watcher — a watcher is a second runner that drifts and violates the runner-minimization standard (#3531).

### Handoff 1b — ANCHOR (version-control) · Kade
- Canonical `git ff-only` to origin/main BEFORE build. `target/release/` is the build artifact; `~/.chorus/bin` is the deploy artifact (#2734) — never deploy-from-target.

### Handoff 2 — VERIFY-UP (deploy → cicd) · Silas
- deploy emits `{ commit, binary, builtCdhash, runningCdhash, equal }`.
- builtCdhash = codesign cdhash of the freshly built+signed binary (#2734).
- runningCdhash = cdhash of the **actually-running** binary, resolved from the live process/launchctl — not the install record (closes the "cdhash=unknown / deployed-equals-running=WARN" gap).
- deploy is idempotent + atomic (install only after sign+verify, atomic move).
- **SYNCHRONOUS** (resolved fork): deploy blocks until running==built, returns verified. "Deployed" can never mean "merged-but-unverified." Bounded wait; timeout = RED.

### Handoff 2b — role-slot lifecycle (the snowflake) · Silas
- role-bin slots are build-time-only, torn down on cw close. prod = `~/.chorus/bin`, the single persistent truth; PATH resolves prod.
- dependency-driven deploy set: build emits `changedUnits` → deploy installs+verifies exactly that set; the hardcoded allowlist dies.

### Handoff 3 — GATE (cicd reads verify-up) · Kade
- Gate (deployed-equals-running floor; werk-test #3190 end-state) consumes the verify record and is **RED when `equal != true` OR `runningCdhash` is unresolvable. "unknown" is RED, not a pass.** Keyed on `landedCommit`. Emitted to the spine on the inherited trace — a red is legible, not "unknown."

### Handoff 4 — BOOTSTRAP (#3197, shared)
- A card modifying the toolchain (werk-build/werk-deploy/werk.yml/the test step) runs the **werk's own werk.yml**, not `-W canonical` (Kade). Build NEW toolchain → staging → verify → atomic-swap, so a broken deployer ships its own fix (Silas). Named exception, not an afterthought.

## Conformance (the live proof, not a test)
A real change merged to main is observed LIVE via the spine — `binary.deployed { binary, cdhash, commit }` — within one deploy cycle. "What commit is live?" = query the spine, never guess.

## Build split (same contract → no handoff drift)
- **Kade:** trigger (inline, landedCommit) · anchor (ff-only) · gate (verify-up, "unknown"=RED) · bootstrap-cicd (werk's werk.yml).
- **Silas:** build-anchored-to-landedCommit · deploy + verify-up (cdhash real, running-resolved, SYNC) · role-slot lifecycle · dependency-driven deploy set · bootstrap-staging (atomic-swap).

## Consequences
- merged≠deployed is closed by construction (one sha, claimed==verified at each handoff).
- The hardcoded deploy allowlist is retired (dependency-driven set).
- Pipeline-fixing cards can land (named bootstrap exception, #3197 path-to-close).
- Implementation proceeds as slices against this ADR; full source spec preserved at the pair's origin (the seam-contract draft).
