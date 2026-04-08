# Cockpit v0 — The Organic Layout

**Date**: 2026-02-13
**Screenshot**: `cockpit-v0-screenshot.png` (save manually — see below)

## What It Is

Jeff's natural working layout, discovered not designed. Four panes on a single screen:

### Left — Observe (Grafana)
- Infrastructure context diagram (canvas dashboard)
- App Operations view showing all services and their ports
- Console open for real-time issues
- Log search via Loki

### Center — Build (Browser)
- The running app at localhost:3000
- Profile page, collections, admin — the product itself
- Multiple browser tabs for related services (Gmail, Calendar, Google Drive, Gemini)

### Bottom — Remember (Finder)
- Claude project memory files visible
- Architecture docs, auth flow docs, ontology docs
- Shared memory artifacts (visibility-enforcement-gap.md, etc.)

### Right — Collaborate (Three Terminal Tabs)
- 🐦 Wren (PM) — observing, synthesizing, tracking decisions
- 🏛 Silas (Architect) — drafting conceptual model and glossary
- 🔧 Kade (Engineer) — stabilizing test suite, triaging failures

## Why This Matters

This IS BL-001 (Developer Workspace / Cockpit) in its most honest form. No custom tooling — just deliberate window arrangement on macOS. The four concerns (observe, build, remember, collaborate) emerged naturally from how Jeff works.

## Questions for Cockpit Evolution

1. What friction does this layout have that's worth removing?
2. Could a tmux/terminal layout replace the Finder pane with something more dynamic?
3. Should Wren be able to poll Silas/Kade's state files automatically?
4. Is there a way to surface kanban board status in the cockpit without opening GitHub?
5. Could the tab-title system extend to browser tabs or Grafana dashboards?

## Relationship to BL-001

BL-001 should be re-scoped: not "build a cockpit" but "reduce friction in the cockpit that already exists." The layout works. The question is what's annoying about it.
