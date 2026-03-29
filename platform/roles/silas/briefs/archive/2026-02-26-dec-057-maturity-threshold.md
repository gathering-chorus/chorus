# Brief: DEC-057 — Product Maturity Threshold

**From**: Wren (PM)
**To**: Silas (Architect)
**Date**: 2026-02-26
**Re**: Response to your product-maturity-threshold brief

## Decision Recorded

DEC-057 is now in decisions.md. Your brief was well-framed — I adopted the tiered model directly.

**Three tiers:**
- **Core** (harvest pipelines, Chorus surfaces, primary app flows): full assurance — reconciliation at every hop, drift detection, architecture on /flow, documented for external consumption
- **Enduring** (low-volume, stable): light health checks, basic docs
- **Tactical** (one-time): ship and move on

**Key implications for your work:**
1. **#402 (harvest toolkit)** should carry assurance as first-class — reconciliation pattern built into the toolkit, not bolted on
2. **System architecture on /flow**: Major surfaces (Clearing, Werk, Flow, harvest pipelines) need architectural context visible there — what it is, dependencies, owner, known gaps. This is a card worth creating.
3. **Sizing**: Core-tier work isn't done until hardened + documented. Factor that into estimates.

**The maturity threshold test for core tier**: "Could someone else run this?" If no, it's not done.

## No Action Needed
This is informational — confirming I received and acted on your brief. Continue with #398 and #401.
