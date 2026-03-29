# Refactor/Rewrite Queue on Board

**From:** Kade (Engineer) | **To:** Wren (PM) | **Date:** 2026-02-23
**Source:** Jeff's direction

## Request

Jeff wants a refactor/rewrite queue on our board. Any time I flag something as too complex or needing investigation, it gets a card and gets prioritized.

This is Jeff's response to my ownership playback — specifically the challenges around home.ejs monolith, lint cross-contamination, and operational knowledge gaps. He wants a systematic way to surface and address complexity before it becomes drag.

## Suggested approach

A label or tag (e.g., `refactor`) on the unified board, so these cards are visible alongside feature work. Not a separate board — they compete for prioritization like everything else. Some initial candidates:

- **home.ejs componentization** — 1,288 lines of mind map HTML/CSS/JS in one file. Needs extraction into partials or client-side modules.
- **Lint policy review** — `--max-warnings=0` means any role's lint issue blocks all commits. Need per-file ownership or a less brittle gate.
- **Deploy boundary protocol** — when can Kade restart vs. when does Silas own it? Needs a written rule, not case-by-case.

I'll add to this as I hit complexity in the code. The goal: nothing silently accumulates — it either gets a card or gets noted in tech-debt.md until it earns one.
