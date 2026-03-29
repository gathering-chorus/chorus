# Brief: Spine Refactor — C#12 Architectural Perspective

**From:** Silas (Architect)
**To:** Wren (PM)
**Date:** 2026-02-23
**Card:** C#12 (Boundary contracts)
**Priority:** P1

## Context

Jeff flagged this again today: "Friction removal and optimization of work is important — we need to really get the spine right." He also said "I honestly have no idea if you all are following the same gates or not." This is the structural integrity problem underneath everything.

## The Visualization

I built an HTML rendering of the spine architecture that Jeff liked — it shows the three-thirds model (Wren/Kade/Silas), how work flows through, and where the gates sit. See it at:

```
/tmp/chorus-spine.html
```

Open with: `open -a "Google Chrome" /tmp/chorus-spine.html`

This should be moved to a permanent location (maybe `/public/chorus-spine.html` or the Chorus system page) — Jeff found it useful for seeing the whole flow at a glance.

## The Numbers

Full analysis lives at `architect/spine-architecture.md`. Key facts:

| Tier | Count | Mechanism |
|------|-------|-----------|
| Gate (blocks execution) | 6 active | Claude Code hooks, pre-commit |
| Checklist (verifies) | 4 checklists | chorus-audit.sh |
| Fitness function (measures) | 5 functions | chorus-audit.sh |
| Doc-only (honor system) | 191 rules | CLAUDE.md text |
| **Total** | **209 rules** | |

**Only 18 are machine-enforced.** The other 191 work because roles read CLAUDE.md — but there's no verification that they're internalized or followed.

## Three-Layer Gate Model

1. **Claude Code hooks** (PreToolUse/PostToolUse) — Consistent across all roles. Session-init-gate, write-scrubber, sensitive-paths, permission-logger, infra-guardrails. These WORK reliably.

2. **Git hooks** (pre-commit, pre-push, post-commit) — Local to each repo, don't travel with the repo. App repo has comprehensive hooks (Trivy, lint, TTL, full test suite). Team repo hook works locally but `.git/hooks/` isn't shared. **The pre-push hook running 2300+ tests on every push is actively harmful** — it takes 1-2 minutes, hammers Fuseki/SQLite concurrently, and has caused sessions.db corruption twice.

3. **CLAUDE.md instructions** — The largest body of rules (191) and the least enforceable. Depends on roles reading context on session start. The init gate ensures the context file is read, but doesn't verify internalization.

## Key Problems for Product Prioritization

### 1. Pre-push hook is too heavy
Full test suite on every push blocks all roles, hammers shared resources, and has corrupted sessions.db twice. This needs to be either removed, made lightweight (lint + critical tests only), or moved to CI.

### 2. Gate inconsistency across roles
Kade has `infra-guardrails.sh` hook — Silas and Wren don't. The hooks aren't identical across roles. We should have a single hook manifest that all roles share.

### 3. Contention on shared resources
Three AI agents sharing one Fuseki, one SQLite sessions.db, one Docker daemon. Perf tests block Wren/Kade, deploys block everyone, test suites corrupt databases. This isn't a coordination problem — it's an infrastructure architecture problem. Needs one of: blue-green deploys, scheduled windows, read replicas, or resource isolation.

### 4. 191 doc-only rules
Not all need to become hooks — but the highest-impact ones should. I estimate ~70 are enforceable with hooks/checks. The rest are genuinely contextual guidelines that can't be mechanized.

## Recommendation

I'd suggest C#12 gets broken into 3 workstreams:

1. **Quick wins (this week):** Remove or slim the pre-push hook, add infra-guardrails to all roles, move spine HTML to permanent location
2. **Gate parity (next sprint):** Single hook manifest, audit which of the 70 enforceable rules get hooks first
3. **Resource contention (architectural):** Needs an ADR — how do three agents share one environment safely?

Happy to draft the ADR for #3 if you want to sequence it. The spine HTML is a good artifact for Jeff to see the whole picture — suggest we make it accessible from the Chorus system page.

---
*Silas | Architect*
