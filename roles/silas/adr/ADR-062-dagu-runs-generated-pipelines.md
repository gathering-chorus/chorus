# ADR-062: dagu runs the pipelines chorus-make generates (supersedes ADR-030 for werk v2)

**Status:** Proposed — draft by Kade for Silas's review (#4465). Not accepted until Silas signs it.
**Date:** 2026-10-09
**Author:** Kade (draft), Silas (owner of ADR-030 and of the ops lane)
**Number:** 062, not 061. ADR-041 §1 cites an "ADR-061, 2026-09-24" (the repo tree as a build-time projection) that has no file and no graph row; 061 is left for Silas to write or retire rather than reused.

## Context

ADR-030 (2026-05-08) chose act running GitHub Actions YAML as the werk orchestration tool. Five months later, measured on 2026-10-09:

- **The YAML is not thin.** ADR-030 decision 2 said "the GHA workflow is thin; steps shell out to the CLIs." `.github/workflows/werk.yml` is 702 lines with about 500 lines of inline bash: proven-round 108, demo 75, prove-live 74, test 52, deploy-canonical 49, outcome 49.
- **We built three workflow-engine pieces on top of act.** Jeff's go is a second act run with `go=true`. Resume after a failure is our own JSON run pin. Step phase comes from regexes over act's log (`werk-phase.ts`). Each is a recurring source of reds and stuck runs.
- **The model does not drive what runs.** The graph names 9 werk steps and 5 cicd pipeline steps. act runs 20 steps that call about 35 verb operations. Nothing generates werk.yml from the graph or checks the two agree.
- **act ignores fields we would want:** `concurrency`, `timeout-minutes`, `continue-on-error`, `permissions`, `environment` (nektosact.com/not_supported.html).

Jeff, 2026-10-09: "1 - we design, code, test chorus-make 2 - we use chorus-make to generate a new werk pipeline that is a new major version 3 - we run cards against werk v1 and also against werk v2 so we have a fallback path when v2 breaks." chorus-make goes first, before athena-make generates compiled services, because a failure there costs only v2.

ADR-030's constraints still hold and still decide: macOS-native (codesign, TCC, cdhash), zero cost, 100% local, modern OSS, minimum complexity, don't reinvent the orchestrator.

## Decision

**werk v2 runs on [dagu](https://github.com/dagu-org/dagu). chorus-make reads a pipeline's steps and their ordered skills from the graph and writes the dagu workflow file; the file is a build output with a "generated, do not edit" header and a drift check. werk v1 (act + werk.yml) stays, unchanged, as the fallback until v2 has proven itself on real cards.**

1. One dagu step per skill (verb operation), in PipelineStep order then skill order. Every step is a `command:` calling a verb; no inline logic.

   **This is the biggest job in the decision.** About 500 lines of inline bash in werk.yml (proven-round 108, demo 75, prove-live 74, test 52, deploy-canonical 49, outcome 49, plus the three `gh api` status posts) must move into verb modes, with tests, before chorus-make can generate those steps. Until a block has moved, v2 cannot run that step, and v1 carries it.
2. Jeff's go is a dagu `human.task` step between demo and land, inside the same run.
3. Resume is `dagu retry --run-id <id> --step <failed> --downstream`. Step status comes from dagu, not log regexes.
4. Verbs keep writing their own spine events (dagu has no per-step event hook).
5. GitHub stays reached the way it is today: verbs call `gh` (PRs, merge); the three inline commit-status posts move into verbs. The token is passed as an environment variable, as act passes it now.
6. The dagu scheduler runs under launchd (Silas's lane: LaunchAgent changes go through Silas, `com.chorus.*`), loopback only, auth mode set explicitly.
7. Run and card state transitions are dagu's (Jeff 2026-10-09: "can dagu handle state transitions so we dont have to?"). The run pins in `~/.chorus/werk-runs` and their reconcile-on-poll retire in v2; card moves are generated steps.
8. **dagu is the product orchestration layer, not only werk's runner** (Silas with Jeff, 2026-10-09 14:48). The graph's value streams are the plan; chorus-make generates one workflow per pipeline; dagu runs athena-* → werk-* → borg-*, each handing the next the commit sha it proved. A failure routes to a named step in the stream that owns it, not back to the start. werk v2 (cicd) is the first pipeline generated; athena and borg follow on the same generator.
9. **MCP and dagu wrap the same verb list** (Jeff via Silas, 2026-10-09 14:49). MCP exposes one verb to an agent; dagu runs them in order. Both are generated from the model's deterministic Skill rows, and neither has a verb the other lacks; a check compares the two lists. Today they differ: MCP has werk-pull and werk-unpull (no Skill row), and the Skill rows have werk-test, werk-demo and werk-sync (no MCP tool).

## Evidence

Trial on Library, 2026-10-09 10:43, dagu 2.18.2 (Homebrew core), isolated DAGU_HOME, not under launchd:

```
run 1        real verb (werk-deploy env-port chorus-api kade) ok · a step that fails once: red · rest aborted
retry        --step <failed> --downstream: the verb step kept, not rerun · failed step green · stopped at the go [waiting], no process held
go           dagu human-task complete → queued → scheduler resumed → land succeeded
```

chorus-make (#4465 werk, fixtures only until the Skill shape lands on #4467): the generated 13-step cicd file loads and dry-runs in dagu; dagu's own step-id rule (`^[a-zA-Z][a-zA-Z0-9_]*$`) caught a generator bug on first load. 12 tests, negative proofs for a hand edit (drift), a deterministic skill with no binary, a duplicate order, and zero rows from the graph.

## Alternatives considered

From Wren's 2026-09-26 survey ("Value streams that run"), ranked on: definition as data the graph writes, a native wait for Jeff's go, resume from a failed step, light on one Mac, events out.

- **Keep act, generate werk.yml.** Fixes model drift, keeps all three home-built engine pieces. Rejected as the end state; it remains v1.
- **Restate.** One Rust workflow reads the graph at run start, the most literal "runtime reads the model". Rejected for now: a 0.x Rust SDK and an interpreter we would own.
- **Temporal.** Best-in-class resume; production needs Postgres. Too heavy.
- **Kestra, Conductor OSS.** Java services; Kestra's human task is paid.
- **Argo, Tekton, Camunda 8.** Kubernetes or paid. Ruled out, as in ADR-030.

## Consequences

### Positive
- The go, resume and step status come from the engine. The second act run, most of the run pin, and the log regexes retire with v1.
- A step added to the graph runs in the next v2 run with no one editing YAML; a hand edit to the file goes red.
- v1 stays as the fallback, so a v2 failure never blocks a land.

### Negative
- A long-running scheduler under launchd: one more service that can be down (ADR-030 rejected the self-hosted GHA runner for the same reason). Mitigation: it is loopback-only, and v1 still lands if it is down.
- dagu's human-task code is young (fixes landing this month, per the 09-26 survey).
- GPLv3. We run it; we do not distribute it.
- GitHub Actions YAML portability (ADR-030's main reversibility argument) is given up for v2.
- Two pipelines run side by side for a while; how one card runs both with only one land is open (below).

### Reversibility
v1 is still there. If dagu stops fitting, chorus-make changes what it writes (act YAML, or a Restate workflow); the graph rows do not change.

## Open (Silas to decide or carry)
- The launchd unit for the scheduler, auth mode, and where DAGU_HOME lives.
- How one card runs v1 and v2 with one land.
- Whether ADR-030 is marked superseded now or when v1 retires.

## Boundary
Decides the orchestration tool for pipelines chorus-make generates. Does not decide the Skill shape (#4467, Wren), athena-make's generated services, or when v1 retires (Jeff).

## References
- ADR-030 — act as the orchestration tool (superseded for v2 by this ADR).
- #4465 — chorus-make; design: `designing/docs/chorus-make-service-design.html`.
- #4467 — the Skill / StepSkill shape (Wren).
- Wren, 2026-09-26, "Value streams that run: chorus-model and chorus-make" (claude.ai artifact KS1WJ9PyKAr18GwXMs3ZpM).
- DEC-022 rule 2 — Silas owns infrastructure stability. DEC-048 — Jeff's go is the land authority.
