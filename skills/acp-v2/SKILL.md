---
name: acp-v2
description: v2 accept-commit-push — act sequences the leaf verb-binaries. Sibling of /acp; both live until team-decided cutover.
user-invocable: true
---

# /acp-v2 — accept-commit-push via act + verb binaries

Drives `act` over the `acp` workflow, which sequences six leaf verb-binaries:
`werk-commit → werk-push → werk-build → werk-deploy → werk-verify → werk-accept`.
**Sibling of `/acp` (not a replacement)** — both paths coexist on the substrate; the
cutover (retiring chorus_acp + redirecting `/acp` to act) is a separate, future card.

```bash
# /acp-v2 <card-id> — the call. DEPLOY_ROLE is the accepter (authorizing identity),
# distinct from ROLE (the builder whose werk is being accepted).
# -P macos-latest=-self-hosted: runs host-native, NOT in Docker — critical because
# werk-* are macOS-native binaries (codesign / TCC / AppleEvents only work host-side).
# Mirrors building-pipeline.yml's pattern (ADR-030).
act -P macos-latest=-self-hosted \
    -W .github/workflows/acp.yml -j acp \
    --input card_id=<card-id> \
    --input role=<builder-role> \
    --input accepter="${DEPLOY_ROLE}"
```

The workflow mints ONE `CHORUS_TRACE_ID` at entry, exports it via `$GITHUB_ENV` so
every verb subprocess inherits it. Each verb writes its OWN `chorus/<verb>/<card>`
gh status with the shared trace_id in the description (ADR-032 §3.3 — leaf-verb
autonomy, no orchestrator pre-seeding). The orchestrator writes a per-step jsonl
witness to `$CHORUS_HOME/ops/logs/werk-acp.jsonl` carrying the same trace — that
gives the pipeline-health view its composed data per card.

**Authority gate (DEC-048)** is enforced in `werk-accept` itself, not in the skill
or workflow. The workflow rebinds `DEPLOY_ROLE` from `ROLE` (builder) to `accepter`
only for the accept step.

**On failure** GHA halts the job on the first non-zero exit. The failing verb's
own all-or-nothing rollback fires internally; the workflow stops sequencing; card
stays WIP. Re-run after the fix — verbs are idempotent.

**If `act` is unreachable** (binary missing, workflow file not in werk): use `/acp`
(v1, chorus_acp MCP) which stays live until cutover. The two paths are independent.

**Bootstrap:** `platform/scripts/install-werk-verbs.sh` builds + installs the six
verb binaries to `~/.chorus/bin/`. CI build-on-merge keeps that current.
