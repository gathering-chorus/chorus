# ADR-030: act + GitHub Actions YAML as the Orchestration Tool for Werk Pipelines

**Status:** Accepted
**Date:** 2026-05-08
**Author:** Silas

## Context

The build/deploy work surface in chorus is hand-operated. A developer commits, pushes, manually rebuilds binaries, manually deploys via app-state.sh / chorus-bin-install, manually verifies. We've been hit repeatedly by the resulting drift class — TS modules stale in running node, MCP client cached schemas, Rust cdhash churn from raw `cargo build`, bash helper shape drift (#2774 receipts).

The team needs a real pipeline: something fires on commit.landed, runs build → deploy → verify, emits typed events, leaves the previously-running binary in place if anything fails.

Constraints that shape the tool choice:

- **macOS-native required.** Every binary we ship is signed with `codesign` and bound to a TCC AppleEvents grant via cdhash. Both Library and Bedroom are Macs. There is no Linux deploy target.
- **Cost zero at our scale.** Per-PR GitHub Actions hosted-runner minutes were suspended 2026-04-29 ($225/mo cost stop). Whatever orchestrator we pick must run essentially free.
- **100% local execution.** No cloud orchestrator that bills per build minute or holds our build state.
- **Modern, OSS, maintained.** No abandonware, no vendor-lock to a closed product.
- **Minimum complexity.** Jeff explicit (2026-05-08): "i dont need an enterprise class orchestration system." A small loop is the right shape — not a multi-node CI cluster.
- **Don't reinvent the orchestrator.** Same principle applied to convergence-domain (NiFi for dataflow, not a hand-rolled engine): name the contract we want, lean on a strong domain-specific tool, write thin shims that translate tool events into our spine taxonomy.

## Decision

**Use [`act`](https://github.com/nektos/act) (nektos/act, MIT, ~70k stars, v0.2.88 May 2026) as the orchestration tool. Workflows are written in GitHub Actions YAML at `.github/workflows/*.yml`. act runs them locally on Library via macOS host-native execution (`-P macos-latest=-self-hosted`). A launchd plist watching canonical's `.git/refs/heads/main` is the trigger.**

Concretely:

1. Build/deploy logic lives in shell CLIs (`platform/scripts/chorus-build`, `platform/scripts/chorus-deploy`) — not in YAML.
2. The GHA workflow is thin: `runs-on: macos-latest`, steps shell out to the CLIs and emit one spine event per step.
3. act's `-P macos-latest=-self-hosted` mapping makes steps execute as host processes — codesign, TCC, AppleEvents, manifest writes all work natively.
4. A launchd plist (`com.chorus.building-pipeline`) watches canonical's main ref file; on update, fires `act push -P macos-latest=-self-hosted` from the chorus repo root.
5. Workflow output is logged to disk; spine events are tailed by the same Loki pipeline that observes the rest of the system. No separate orchestrator dashboard needed.

## Alternatives Considered

**Hosted GitHub Actions (paid runners).** Closest to "default modern CI." Rejected on cost (per-minute meter, suspended 2026-04-29) and macOS compute pricing being especially expensive on hosted runners.

**GitHub Actions self-hosted runner.** Mac-native, free per-minute, full GHA ergonomics, PR status checks. The strongest alternative. Rejected as the *primary* choice because it requires a long-running daemon to monitor (one more thing that can be down), tighter coupling to GitHub.com (the runner registration is GitHub-keyed), and lock-in to GitHub's vendor surface for the orchestration UI. Reserved as a clean upgrade path if act's CLI shape ever stops fitting — same workflow YAML moves over with one runner registration change.

**Woodpecker CI** (community fork of Drone). Modern, OSS, lightweight, well-governed. Rejected for our specific stack: Linux-container-first; codesign + TCC don't run inside its containers. Workarounds (separate Mac-native runner outside Woodpecker, manual signing step) reintroduce the hand-operated step we're trying to automate away. Strong tool, wrong fit for our macOS geometry.

**Jenkins with a Mac agent.** Most mature self-hosted CI tool ever; works on macOS. Rejected on weight (JVM, ~512MB+ memory, plugin sprawl, configuration drift in `~/.jenkins`) and feel (Groovy DSL, Blue Ocean UI, ceremony). Capable but mismatched to "small loop" framing.

**Tekton.** Kubernetes-native; would require running k8s on Library. Out of scope; we don't run k8s.

**Forgejo/Gitea Actions.** Both ship `act_runner` (a fork of nektos/act), so functionally identical orchestration with a Forgejo/Gitea repo server in front of it. Adds an extra service to maintain (the Git host) without buying us anything we don't already have via direct act invocation against canonical chorus.

**Hand-rolled bash + launchd WatchPaths only.** No orchestrator, just scripts firing on ref updates. Rejected because we'd be writing what act already gives us — step sequencing, conditional logic, env management, log capture. The Jeff principle from this morning: "we don't build a tool to handle orchestration if there are very strong domain-specific tools." act is the strong domain tool here.

**`PULSE_ALLOW_DIRECT_POST=1` style escape hatches as the orchestration model.** Not actually orchestration — that's a per-call bypass for a different gate (#2804 nudge migration). Mentioned only because it's a related shape.

## Consequences

### Positive

- **Cost: zero per build.** Local execution; only marginal electricity on a machine already running 24/7.
- **macOS-native.** `-P macos-latest=-self-hosted` runs steps as host processes; codesign + TCC + cdhash work the same as when a developer runs `build-signed.sh` by hand.
- **Workflow YAML is portable.** `.github/workflows/*.yml` files run unchanged on hosted GHA, self-hosted GHA runners, Forgejo Actions, Gitea Actions, and act. Tool migration cost is bounded.
- **Existing scripts reused.** Build and deploy logic stay in shell CLIs (`chorus-build`, `chorus-deploy`); workflow is a thin wrapper. Implementation churn from a future tool change is contained to YAML.
- **Single binary footprint.** act is one Go binary. No daemon, no UI process, no JVM, no dependent services beyond launchd (already running).
- **Active community.** ~70k stars, MIT licensed, 1300+ commits, used as upstream by Forgejo and Gitea — three independent communities depend on it staying healthy.

### Negative

- **act-specific extensions create some lock-in.** `-P <label>=-self-hosted` is an act feature; self-hosted GHA runners express the same idea differently (label-based runner registration). If we lean heavily on `-P` mappings inside YAML, migrating to self-hosted GHA runner is YAML rework. Mitigation: keep the `-P` invocation in the launchd plist (one place), keep workflows clean of act-specific syntax.
- **GHA Marketplace actions create tighter coupling.** Most JavaScript actions work in act; some don't. Heavy marketplace use makes future tool migration expensive. Mitigation: prefer `run:` calling our own scripts over `uses: third-party/action@vN`. Workflows become "shell out to our CLIs" — very portable.
- **Cross-stage event flow is awkward in GHA YAML.** The pipelines pattern doc names cross-stage events (designing-pipeline → building-pipeline → proving-pipeline). GHA YAML's `repository_dispatch` / `workflow_dispatch` is workable but not the natural shape. If our cross-stage orchestration becomes complex, we may outgrow GHA YAML. Mitigation: today we have one pipeline (building); cross-stage doesn't exist as live work.
- **act tracks GHA syntax with lag.** New GHA contexts and expression functions land in act weeks after GitHub releases them. Mitigation: stick to stable subset; we own when to upgrade.
- **No native PR status checks.** act runs locally; there's no "green checkmark" on a GitHub PR before merge unless we wire it ourselves via the GitHub API. Acceptable today — the gate chain (gate-product / gate-code / gate-quality / gate-arch / gate-ops) is the team's quality bar, not GHA's.
- **Launchd WatchPaths reliability is now ours to own.** If the trigger misses an event (event coalescing, path resolution edge cases), the pipeline silently skips a build. Mitigation: poll fallback if reliability becomes an issue; spine event for every fire so misses are detectable.

### Reversibility

If act ever stops fitting: the workflow YAML files are the load-bearing artifact, and they're standard GHA syntax. Migration paths in increasing distance:

1. **act → self-hosted GHA runner on Library.** Same YAML, register a runner against a GitHub repo, swap the launchd invocation. Days of work.
2. **act → Forgejo Actions.** Same YAML, run a Forgejo server (which uses act_runner under the hood) and point it at a Forgejo mirror of chorus. Weeks of work for the server stand-up.
3. **act → Woodpecker.** Different YAML (similar shape), Linux-container-first so signing moves to a Mac-side step outside the orchestrator. Several weeks; loses macOS-native simplicity.
4. **act → custom orchestrator.** Rejected by this ADR's premise. Don't.

The exit doors are wide. We are not boxed in.

## References

- 2026-05-08 Jeff ↔ Silas — discovery + spike of act on Library; host-native macOS via `-P` confirmed working with codesign + TCC + chorus-log spine emit.
- [`nektos/act`](https://github.com/nektos/act) — upstream project.
- [`act` user guide — Runners](https://nektosact.com/usage/runners.html) — `-P` host mapping.
- #2774 — Building-pipeline implementation card (the work this ADR's decision shapes).
- #2734 — `~/.chorus/bin/` single deploy location for cdhash stability (the artifact location that `chorus-deploy` will install to).
- #2791 — Manifest schema + writer (the artifact inventory `chorus-build` will write to).
- ADR-026 — CI architecture and lock-file policy (predecessor framing for CI; this ADR sits underneath it as the orchestration-tool layer).
- DEC-022 — Silas owns ops; deploy lifecycle is in scope.

## Boundary

This ADR decides the **orchestration tool** for werk pipelines. It does NOT decide:

- Which subdomains live under werk (covered in `pipelines-service-design.html` — pattern, not domain).
- The contract shape of individual CLIs and MCPs (covered in `build-and-deploy-service-design.html` and #2774's AC).
- How #2774 is sequenced or scoped (Jeff's call per card).
- Whether Bedroom ever runs builds (out of scope; today Library-only).
- The proving / designing / operating / heralding pipeline implementations (separate ADRs when those stages get implementation work).
