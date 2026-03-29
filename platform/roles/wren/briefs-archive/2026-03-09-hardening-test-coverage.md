# Brief: Hardening — Page Integration Tests + Flow Tests

**From:** Silas | **Date:** 2026-03-09 | **Context:** Jeff observation during #1225 demo

## Jeff's Direction

Jeff wants to refactor aggressively. Without regression coverage, that will break things faster than we fix them. He's seen the pattern already — ontology renames (#1161) broke seeds silently, style lint (#1212) accumulated 30 warnings unnoticed. Speed without coverage is a debt accelerator.

## Two Test Layers Needed

### 1. Page Integration Tests (expands #1119 scope)

Every page, every interactive element — not just "does it render" but "do the buttons work, do queries return data, do filters filter."

- **Input:** Style manifest (74 pages from #1193)
- **Tool:** Playwright or similar browser automation
- **Validates:** Render correctness, button/dropdown/modal behavior, SPARQL queries return data, shared partials (navbar, footer, sidebar) intact
- **Catches:** Ontology rename breakage, CSS class changes, partial restructuring, route renames
- **Bar:** "PDF button clicks and produces a file" not "PDF button exists"

### 2. Flow Tests (new — not currently carded)

Multi-hop end-to-end validation. A seed travels: SMS → bridge → brief → card → ontology → SPARQL → rendered on Seeds page with correct status.

Jeff identified four flow families — these are the core loops, used every session, not edge cases:

1. **Seed flow** — SMS → bridge → brief → card → ontology → rendered on Seeds page
2. **Nudge flow** — role sends → queue → delivery → recipient receives mid-session (broke live during this session — misrouted to sender)
3. **Clearing flow** — start → roles join → conversation → decisions captured → transcript indexed
4. **Skill flows** — gemba (invoke → tail → cron → commentary → exit), demo (board-ts demo → nudge observers → observe → accept/reject)
5. **Search flow** — event logged → indexed → embedded → Chorus query returns relevant results
6. **Card flow** — create → AC written → move to WIP → build → demo → accept/reject, with decision gates enforced at each transition

Every one is multi-hop with no validation between hops. High frequency + no tests = guaranteed breakage during refactoring.

- **Catches:** Cross-system breakage, dropped signals, mid-flow failures

## Why Now

Jeff said: "I want to refactor aggressively — if we don't have these tests our flows and pages will almost certainly break quickly." This gates all future refactoring velocity. Without it, every bold change becomes a gamble.

## Prerequisite: Flow Mapping (before any test writing)

Jeff's direction: **visualize the flows first, then test them deliberately.** Drawing on his requirements analysis experience at EFT Technologies — if you can't see the end-to-end path, you're guessing at blast radius and testing haphazardly.

Step 1: Map every end-to-end flow — each hop, handoff, and data transformation. For the app: which pages share partials, which queries feed which templates, which ontology classes surface where. For the team: which signals travel which paths, which roles touch which artifacts.

Step 2: From the map, derive the test cases. Each flow becomes a test. Each hop becomes an assertion. Miss two related pieces and the chain breaks — the map prevents that.

This is a visual artifact, not a doc. Something the team can look at and trace a path through.

## Recommendation

- **First:** Flow mapping — visual artifact showing all end-to-end paths (app + team)
- Expand #1119 scope or create a parent card for the page integration harness
- Card the flow test layer separately — different blast radius, different tooling
- All three are P1 hardening prerequisites before any aggressive refactoring begins

## Ceremony Discipline (Jeff self-observation)

Jeff notices he battles the impulse to fix gaps mid-ceremony (demo, gemba). Because roles are tuned to match his energy, his fix impulse propagates — the whole team pivots from observing to fixing, and the ceremony breaks for everyone.

**Rule for gemba/demo skills:** Gaps observed during ceremony get carded, not fixed in-flight. Roles should match Jeff's *observation* energy during ceremonies, not his fix energy. Note → card → keep going.

This is a CLAUDE.md-level pattern — applies to all three roles during any ceremony.

## Also Surfaced (not ready to card)

**Inter-role event polling gap.** Roles have send channels (briefs, nudges, spine events, board signals) but no mid-session receive loops. Demo signals get dropped when a role is heads-down in JDI flow. Jeff is observing this tension between vertical execution and horizontal coordination — not ready to build a fix yet.
