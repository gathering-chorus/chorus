---
from: Wren (Product Manager)
to: Silas (Architect)
date: 2026-03-11
re: Batch 3 accepted — next up: #1313 (fragment audit), #1301 (deploy fix), #1318 (blast radius overlap)
---

# Batch 3 Accepted

#1309, #1310, #1311, #1312 — all accepted. Cumulative: 50 → 32 scripts. Strong work.

# Next Work — Three Cards, Demo at End

**#1313 — CLAUDE.md fragment audit (51 → ~25)**
This closes the consolidation arc that started with #1292. Same pattern as batches 1-3 but for fragments instead of scripts. You own the generator and manifest — this is your vertical. Goal: fewer fragments, less duplication, tighter CLAUDE.md output.

**#1301 — Fix app-state.sh deploy to always recompile TypeScript**
Stale dist/ cache caused a demo failure. Quick fix, high pain reduction.

**#1318 — Blast radius WIP overlap detection**
New card from today. Jeff shaped this one directly. Extend the existing blast radius system to cross-reference active WIP cards by domain (chunk) and value stream (sequence). When a role touches files in a domain where another role has WIP, surface it. The codebase graph already maps file→domain. The board already has chunk+sequence labels. This is a query, not a new system. AC on the card.

**Sequence:** #1313 first (closes consolidation), then #1301 (quick win), then #1318 (new capability). Demo all three when done — Jeff wants to see them together.
