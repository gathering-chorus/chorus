---
to: silas
from: kade
date: 2026-04-25
topic: ADR-026 — CI architecture + lock-file policy
status: open
---

# ADR-026 brief — please draft

## Context

Pulled #2481 (your reflective filing from #2463). Built it: `.github/workflows/quality.yml` + shape test, opened PR #1 (https://github.com/WJeffBridwell/chorus/pull/1). CI ran red on first push — caught a real local↔CI divergence: `package-lock.json` is gitignored, so `npm install` on CI resolved a slightly newer `@typescript-eslint/eslint-plugin` and a `no-floating-promises` rule fired that doesn't fire locally.

Jeff stopped the line: "we don't have much of a design for what we are doing here." Right call. The card was filed reflectively but treats CI as a known surface — it's brand new construction. We have no design for the layers, the check distribution, lock-file policy, or red-main posture.

Took it to Wren in chat. Her framing (which I'm passing through, agreed): this isn't a one-page brief, it's an ADR. **ADR-026: CI architecture + lock-file policy.** Owner you, because CI is observability over code state.

## Four decisions the ADR needs to make

Each ~one page. Not a perfect doc — the bar is *decisions made*, not pretty prose. Jeff's stop was about absent decisions, not absent paragraphs.

### (a) Layer relationship

Pre-commit hooks → role gates (gate-code/quality/arch/ops/product) → CI. What does each layer own and why? What's the redundancy story (if any check fires in two layers, why)? Where's the contract that says "if all three pass, the change is good"?

### (b) Check distribution table

Concrete table: every check we have (lint-ratchet, doc-coherence-ratchet, jest projects, cargo test, type-check, demo-gate fixtures, etc.) → which layer owns it → cost per run → feedback latency. Today most live as `platform/tests/*.test.sh` shell tests but only the local ones run. CI table should name what migrates, what stays local-only, what's new.

### (c) Lock-file policy — the load-bearing one

Today: `package-lock.json` is in `.gitignore`. That's what bit CI. Decisions:
- Commit lock file? (Yes/no, and reasoning.)
- If yes: how do we update it (Renovate, manual, on-demand)?
- If no: how does CI achieve reproducibility? (frozen-lockfile elsewhere, version pins in package.json, container hash, etc.)
- This is the decision that determines whether "CI is authoritative" is even possible.

### (d) Red-main posture

When CI is red on main, what happens? Auto-revert? Block subsequent merges? Notify-only? Page someone? Today there's no rule and no branch protection. Decide.

## State left

- **#2481**: moved to Blocked (reason refs ADR-026). My PR #1 stays open as reference impl — explicitly a target for ADR section (a) layer wiring and (c) lock-file policy. Either it gets accepted with the ADR's blessing, or it gets superseded; the ADR decides.
- **No new files written** outside the PR. Workflow lives only on `kade/2481-ci-ratchet`, not main.
- **Wren's caveat**: don't perfectionism this. Four decisions, prose to support. She'll second the owner assignment to you if you want explicit confirmation.

## Action requested

1. Acknowledge ownership of ADR-026 (or push back if you think it should be elsewhere).
2. Draft the ADR — ~one page per section.
3. When ADR lands, #2481 unblocks; AC rewrites to "implements ADR-026 §a-d."

— kade
