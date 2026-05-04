---
generated: 2026-05-04 09:18 Boston
session_arc: ADR-028 reconcile + decisions canonical write path
---

# Next session — silas

## First action: close #2716 live-verification AC + /demo + /acp

After Jeff reboots me, the new session loads the rebuilt MCP binary (with the slug-matcher fix). First three calls:

```
chorus_decisions_get adr-028
chorus_decisions_get ADR-028
chorus_decisions_get dec-095
```

All three should return the right body. Once verified, mark the last AC ✓ on #2716, then run /demo, then /acp.

## What landed this session

**#2716 (WIP, 7 ACs filed, 6/7 done in this session, 1 awaiting reboot-verify):**
- Wired `GET /api/athena/subdomains/:id/decisions` (server.ts) — handler had been on disk since #2485 but never imported/mounted; the 308 redirect at server.ts:1709 had been pointing at a non-existent route
- 286 decisions now reachable via canonical path
- MCP `chorus_decisions_list` works
- MCP `chorus_decisions_get <id>` matcher rewritten (mcp/server.ts:946) — case-insensitive, falls back URI-slug → id-field → label-field. Logic verified against live data via node script (adr-028 / ADR-028 / DEC-095 / "XSD Model..." all resolve). Awaiting live MCP verify on next session reload.
- Added `subdomain-principles` + `subdomain-decisions` to athena health endpoint list (handlers/athena-health.ts + server.ts ATHENA_QUERIES)
- New file `platform/api/src/handlers/subdomain-decisions-write.ts` — Zod `DecisionInputSchema` + `createSubdomainDecision` + `updateSubdomainDecision`. Per ADR-028 Addendum 2 (single declarative schema). Bad-input → 400 with structured Zod issues.
- `POST` + `PUT /api/athena/subdomains/:id/decisions[/:entityId]` wired in server.ts
- ADR-028 reconciled to graph via PUT — Status: Accepted, Wren's three patches landed in graph (meta-invariants explainer, Class B MUST 4 paraphrase clause, Addendum 2 fifth-place enumeration), (reconstructed) caveats removed

**ADR-028 collaboration arc closed (with Wren):**
- Wren's 07:16 nudge gave three verdicts on reconstructed sections; all three patches applied to disk
- Disk → graph reconcile via the new Zod-validated PUT path (the ADR commands its own persistence — it's the proof point that the new write path works)
- Status: Proposed → Accepted (Silas + Wren co-author 2026-05-04)
- Pending: Jeff acceptance

**Wren's #2664 (nudge cleanup) — gemba'd, gate:arch-pass logged:**
- Q1 (signal loss): zero — old alert was reading retired writer state. Architecturally-correct replacement rides on top of #2708 (receipts events).
- Q2 (lift 25 LoC fold): keep local for now; Wren filed #2717 (Later, P3) for lift-when-#2708-lands trigger.

## Open follow-ons surfaced this session (NOT filed yet — confirm with Jeff before filing)

1. **SPARQL read-side bug** — `platform/api/src/sparql/loom-decisions.sparql` queries `chorus:decisionStatus` but actual graph predicate is `chorus:status`. Every decision in GET response shows `status: ""`. Same likely for `chorus:date` (queries `chorus:decisionDate`). Easy fix — just align predicates with the data.

2. **Principles handler retrofit to Zod** — `handlers/subdomain-entities.ts::createSubdomainPrinciple/updateSubdomainPrinciple` use procedural label-required validation, not Zod. Per ADR-028 Addendum 2 they should use the same shape as the new decisions handler. Jeff explicitly scoped this OUT of #2716 ("we can do the full monty here - i feel its a little too soon"). File only if Jeff opens.

## Branch state at close

- On `silas/2716` (need to verify; pull may have created it; check `git branch --show-current`)
- main has Wren's #2664 acp inbound (gate:arch-pass logged from this seat)

## Pickup notes

- **MCP reload is the gate** — the matcher fix is built into dist/mcp/server.js but the live process spawned by Claude Code is the OLD binary. Only a session restart loads the new code. That's why the AC stayed open.
- **Don't refile follow-ons mid-session** — Jeff confirmed the discipline twice today (full-monty pullback on #2716, no-card-creation discipline). Hold the SPARQL read-side bug + principles retrofit as observations until Jeff opens them.
- **ADR-028 is now graph-canonical** — any future edits go through `PUT /api/athena/subdomains/loom-decisions/decisions/adr-028` with the new Zod schema. Direct on-disk edits to `roles/silas/adr/ADR-028-substrate-class-domain-contract.md` are now drift unless reconciled the same way. The disk file remains as the readable source for offline browsing but graph is source of truth per MUST 1 / I-1.

## Done at close

- 6/7 ACs on #2716
- Wren's #2664 gate:arch-pass logged
- ADR-028 graph reconcile proven via the new write path
