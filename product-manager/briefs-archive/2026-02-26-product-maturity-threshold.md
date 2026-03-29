# Brief: Product Maturity Threshold

**From**: Silas (Architect)
**To**: Wren (PM)
**Date**: 2026-02-26
**Re**: Jeff's direction on system reliability, shareability, and operational rigor

## Context

Jeff articulated a clear strategic shift this morning. Key points:

1. **Harvest assurance**: Every enduring pipeline needs reconciliation (A=B at each hop), incremental drift detection, and ongoing health checks. Not just "run and hope." His integration architecture background drives this — he knows the cost of silent data loss.

2. **Rigor heuristic**: Core domains with active sources get full assurance. Enduring/low-volume gets light checks. Tactical/one-time gets none. The trigger: "will this run again, and would we notice if it dropped data?"

3. **System architecture visibility**: Cards work for tasks but not for systems. Clearing, Werk, Flow, harvest pipelines — these need architectural context visible on /flow, not buried in docs. What it is, what it depends on, who owns it, known gaps.

4. **Product maturity threshold**: Jeff explicitly said this needs to be good enough to share — open source, consulting, or as a working system others rely on. This changes what "done" means. The gap isn't code quality — it's observability, reproducibility, assurance, and documentation.

## What This Means for Product

- **Definition of done evolves**: Shipped + hardened + documented, not just shipped.
- **Sizing changes**: Bigger cards need to account for the assurance and documentation tail.
- **/flow needs richer context**: Major surfaces should link to their architecture, not just card status.
- **#402 (harvest toolkit)** should carry the assurance pattern as a first-class requirement.
- **Revenue angle**: This maturity work IS the value proposition if Gathering/Chorus goes external.

## Ask

How do you want to incorporate this into how we size, prioritize, and define done? This touches DEC-048 (proving gate) and could warrant its own decision record.

## Related Cards
- #402 — Harvest toolkit
- #406 — Bedroom storage audit
- #407 — CMDB spike
