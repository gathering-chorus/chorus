# ADR-035 — Time render-vs-storage boundary

**Status:** Accepted (2026-05-26, #3093)
**Owner:** Silas

## Context

Jeff is in Boston (America/New_York, EDT/EST). The chorus-api codebase reaches for JavaScript's `Date.prototype.toISOString()` by default (it's the standard JS API for serializing a Date), which always emits UTC with a `Z` suffix. That output landed unchanged in human-facing strings — eventloop-alert message bodies, nudge bodies, bridge posts, terminal output. Every alert Jeff read required UTC→EDT math.

The "you own the time domain" directive (2026-05-26 mid-day) made this explicit: time normalization is an architectural concern, not per-call developer choice.

## Decision

**Storage stays UTC. Render to humans goes through `boston()`.**

Two surfaces, two contracts:

- **Storage surfaces** (kept as ISO/`Z`):
  - Spine event `ts` field — cross-machine sortable, timezone-agnostic, canonical.
  - JSON log lines (`timestamp` field) ingested by Loki — parser expects ISO.
  - Database rows, key prefixes, sortable filenames.
  - JSON API response fields where the client owns its own render.

- **Render surfaces** (call `boston(ts)`):
  - Alert message bodies (the string a human reads, not the `ts` field next to it).
  - Nudge bodies, bridge posts, any text agents emit at Jeff.
  - Terminal output that's read directly (NOT log lines parsed by Loki — those stay ISO until Loki's parser is updated in a coordinated change).

The helper lives at `platform/api/src/time-utils.ts` as `boston(ts: Date | string | number): string` returning `YYYY-MM-DD HH:MM:SS [EDT|EST]`. It is the **only** allowed render-time formatter.

## Why a helper, not a configuration?

A "default TZ" knob would catch every call site and break storage contracts (spine sortability, Loki parser, db ordering). The boundary is per-surface, not per-deploy. A helper makes the boundary explicit in the call site: `boston(ts)` says "this is a render surface" the way `ts.toISOString()` should now say "this is a storage surface."

## Enforcement

This ADR is the documented convention. Mechanical enforcement (a grep-gate that refuses `.toISOString()` in files matching `**/handlers/**` unless inside a JSON response object, or similar) is a follow-on if drift is observed. For now: this ADR + the `boston()` JSDoc + this card's sweep (eventloop-alert) establish the discipline.

## Scope of #3093

- Helper shipped (`time-utils.ts:boston`).
- Eventloop-alert message body switched (the immediate Jeff-visible surface).
- Sweep audited: access-log.ts was identified as a candidate but **deferred** — it has a Loki parser consumer, so the format change needs coordinated update (separate card, cross-domain with the observability lane). Documented in commit + here.
- Tests: 8 new `boston()` unit tests; eventloop-alert tests updated to assert Boston format in the message body while keeping the `ts` field as ISO.

## Out of scope (named, not bundled)

- Sweep of every other `.toISOString()` site in chorus-api. Most are JSON response fields (`generatedAt`, `timestamp` on header objects) where the client owns render; those stay ISO per the boundary above.
- access-log.ts format change (cross-domain with Loki).
- Mechanical lint/grep-gate to enforce the boundary at PR time.
- The same discipline applied to chorus-hooks (Rust) — separate sweep when it surfaces.

## Related

- #3050 — the eventloop alert this convention was prompted by.
- #3079 — the current-op sentinel whose alert body now also renders Boston.
- #3089 — the request-op middleware whose `op` field already named routes; the timestamp half catches up here.
