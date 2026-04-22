# Pulse — Service Design

**Silas, 2026-04-21. Draft under #2280. Source: pulse.rs (commands/pulse.rs), context_inject.rs, tiles.ts, alerts context-alerts.ts, prior #1881 demo, session-start orchestration tests.**

## Promise

When Pulse is healthy, every role and every surface can read one file and know the live state of the team — role states, recent spine activity, alerts, nudges, health, board, index freshness — without issuing a round of queries. The data is **small, fresh, and attributed**: each section carries the age and source of its underlying producer, so consumers can reason about signal staleness instead of trusting a synthesized blob as if it were atomic.

When Pulse drifts from this shape, agents form claims from pre-digested, silently-filtered state. That produces the worst Context failure: fast access to *wrong* data. Jeff becomes the corrector.

## Current State (2026-04-21)

Pulse works. It's been load-bearing since #1881 (2026-04-11). It fires on every UserPromptSubmit via `chorus-hook-shim pulse`, and on every post-tool-use via main.rs:498 (#2120), so `/tmp/pulse-latest.json` stays hot. The API re-serves it at `GET /api/chorus/pulse/latest` (`chorus-pulse-latest.ts`). Assembly runs in 40-60ms steady state (sub-200ms budget). It is not the perf problem.

What it has quietly become is an **aggregator that filters data it finds inconvenient**. Three design debts have accreted:

1. **No per-source timestamps.** A consumer reading `pulse.alerts.fired_today` cannot tell whether the list is 2s old or 60s old, whether `deep_health` was cached at 09:14 or 14:14, whether the board snapshot is from the current pulse cycle or a stale fast-path. The top-level `timestamp` is when pulse *ran*, not when any given section's producer last wrote.
2. **Alert suppression is embedded.** `assemble_alerts()` (pulse.rs:197-206) drops `index-freshness`, `fuseki-harvest-stale`, and `lancedb-stale` alerts when the freshness summary reports `dead==0 && critical==0`. That was a #1889 bandaid to stop false alarms during the freshness redesign. It means a freshness alert can fire and disappear from pulse in the same cycle without the consumer ever seeing it. **Consumers don't know the filter exists.** They think they're reading raw alert state.
3. **Comments describe an aggregator pattern.** Line 1: *"structured team state JSON on every prompt cycle."* That frames Pulse as a polling synthesizer ("call me, I'll go ask around"). The actual role is closer to **event-bus cache**: each section has an upstream producer writing on its own cadence, and pulse is a narrow read-through to surface the current values with freshness annotations. The naming shapes how consumers treat it — today they treat it as ground truth.

## The Thesis

**Pulse is an event-bus cache, not an aggregator.** Every section is owned by a separate producer writing on its own cadence. Pulse's job is to **read, annotate with age, and expose — never to decide what the consumer is allowed to see.** Suppression belongs at the producer (alert clears its own cooldown when it resolves) or at the consumer (UI/agent decides what to display given raw state + resolved flag). It does not belong in the middle.

**The deeper problem — the one this card is ultimately about: agents treat pulse as ground truth because the shape tells them to.** The current file has no age annotations and a silent filter, so a Claude session reads it and forms claims ("tunnel is healthy," "alerts: 1") as if the data were atomic. Fixing pulse's shape isn't a correctness cleanup — it's what lets agents reason instead of recite. The PM concern lives here, not in the producer/consumer tables below.

This reframe reclassifies pulse.rs as a data-plane component, not a business-logic component. Business logic migrates out.

## Producer Inventory

What emits to Pulse, at what cadence, with what schema guarantee.

| Section | Source | Producer | Cadence | Schema guarantee |
|---|---|---|---|---|
| `roles.{role}.*` declared | `/tmp/claude-team-scan/{role}-declared.json` | `role_state.rs` (writer), invoked by `role-state` CLI | On role-state transition (building/waiting/idle/blocked/observing) | Stable: `{state, card, ts, source:"declared"}` |
| `roles.{role}.card_inferred` | `/tmp/claude-team-scan/{role}-inferred.json` | `observer.rs` hook | Per tool-use event emission | Stable: `{card, ts, source:"inferred"}`; 5-min TTL enforced at read |
| `events.*` | `platform/logs/chorus.log` | `chorus-log` script | On every spine event (continuous) | Last 60s slice; count by role + event type |
| `alerts.fired_today` | `/tmp/alert-{name}-{date}` files | Grafana alert rules + scripted alerts (`fuseki-stale-alert.sh`, `lancedb-stale-alert.sh`, `tunnel-alert.sh`, `vikunja-auth-failure`, `index-freshness-alert.sh`) | On alert fire; one file per (alert, date) | **Weakly typed:** filename parsing, not structured. No clear/expire signal; cooldown file persists for the day. |
| `nudges.{role}` | `/tmp/voice-inbox/{role}/pending-inject.txt` | nudge CLI + bridge-subscriber | On nudge persist | `{pending, age_secs, stale}` where stale = age > 600s |
| `health.*` | `/tmp/deep-health-latest.json` | `deep-health.sh` LaunchAgent | 5-min cadence | Owned by deep-health; pulse re-serves verbatim |
| `board.{wip_cards,swat_cards,next_cards}` | `/tmp/board-wip-snapshot.json` | `cards` CLI (on mutation) + pulse refresh (10s TTL stale path) | Event-driven + TTL fallback | Three arrays; each card is `{id, title, owner, domain, status}` |
| `index_freshness.*` | `/tmp/freshness-snapshot.json` | Pulse itself (30s TTL cache of `/api/chorus/freshness`) | Self-maintained | `{fresh, warn, critical, dead, total_sources}` |

**Producer gaps to close under this card (documentation only — no schema change here):**
- Alert cooldown files have no "cleared" signal. Current workaround: suppression at pulse. Better path: each alert script removes its cooldown file when the underlying condition clears. That's a follow-on, not part of this design.
- `board-wip-snapshot.json` has two historical formats (flat array = legacy; object with `{wip_cards, swat_cards, next_cards}` = current). pulse.rs handles both at L264-286. This should be retired once no legacy writers remain.

## Consumer Inventory

What reads Pulse, which fields, how it handles staleness.

| Consumer | Fields read | Staleness behavior |
|---|---|---|
| `context_inject.rs` (UserPromptSubmit envelope) | Whole file, shaped into context-synthesis block. Primary read is `roles`, `board.wip_cards`, `alerts.fired_today`, `index_freshness`, `events` | Regenerates pulse if file is missing or >30s old (`is_pulse_stale`, L63). Silent on staleness beyond that. |
| `tiles.ts` (Clearing UI) | `roles.*`, `board.wip_cards`, `health`, `alerts`, `nudges` | Reads whatever is on disk. No age check visible to user. |
| `chorus-pulse-latest.ts` (`GET /api/chorus/pulse/latest`) | Whole file | Serves last written; no age header in response |
| `session.rs` (SessionStart) | Triggers pulse regen; doesn't read it itself | N/A |
| `context_cache.rs` | References pulse path in cache-warmup hint | N/A |
| `gemba-start.sh`, `gemba-tick.sh` | `roles.*`, `events.*` | Reads verbatim |
| `coherence-check.ts` / `derive-role-state.ts` | `roles.{role}.*` | Cross-references declared vs. inferred |
| `bats` tests (`session-start-orchestration-e2e.bats`, `context-inject-envelope-spec.bats`, `pulse-bar.bats`) | Various | Test fixtures |
| Agents (Claude sessions) | Whole file read into boot prompt | **No staleness awareness.** Agents form claims on pulse contents as if atomic. This is the #2280 failure mode. |

**The biggest consumer gap:** the agent. Jeff's own session opening earlier today referenced "alerts: 1 (tunnel)" from pulse without noting that `deep_health` might be 5 minutes old or that index-freshness was filtered out of `alerts.fired_today`. Agents treat pulse as ground truth because the shape tells them to.

## Proposed Change — Per-Source Timestamps

Add a top-level `sources` object. Each section name maps to `{ts, source, age_secs}` where `ts` is the producer's last-write timestamp, `source` is the file or service, and `age_secs` is the delta at pulse-assembly time.

```json
{
  "timestamp": "2026-04-21T14:30:00",
  "sources": {
    "roles.wren.declared":    {"ts": "2026-04-21T14:29:42", "source": "/tmp/claude-team-scan/wren-declared.json", "age_secs": 18},
    "roles.wren.inferred":    {"ts": "2026-04-21T14:28:10", "source": "/tmp/claude-team-scan/wren-inferred.json", "age_secs": 110},
    "events":                 {"ts": "2026-04-21T14:29:58", "source": "platform/logs/chorus.log", "age_secs": 2, "window_secs": 60},
    "alerts":                 {"ts": "2026-04-21T14:30:00", "source": "/tmp/alert-*", "age_secs": 0, "note": "scanned at pulse time"},
    "nudges.silas":           {"ts": "2026-04-21T13:57:12", "source": "/tmp/voice-inbox/silas/pending-inject.txt", "age_secs": 1968},
    "health":                 {"ts": "2026-04-21T14:25:03", "source": "/tmp/deep-health-latest.json", "age_secs": 297},
    "board":                  {"ts": "2026-04-21T14:29:55", "source": "/tmp/board-wip-snapshot.json", "age_secs": 5},
    "index_freshness":        {"ts": "2026-04-21T14:29:30", "source": "/tmp/freshness-snapshot.json", "age_secs": 30}
  },
  "roles": { ... },
  "events": { ... },
  ...
}
```

**Shape choice — sidecar, not inline:** keep the existing section shapes byte-compatible; add `sources` as a sibling. Consumers that don't care (tiles.ts today) keep working. Consumers that want to reason about age (agents, coherence-check) read `sources.<section>.age_secs`. No breaking change.

**Schema versioning — explicit, not implicit.** A top-level `"schema_version": 2` field ships in the same commit (Wren: "implicit compat is drift seed"). Consumers that care can branch on it; today's read path stays compatible.

**Compute cost:** 8 `metadata().modified()` calls already happen inside the assemble_* functions. Promote the timestamps instead of discarding them. Budget stays under 200ms.

## Proposed Change — Remove Suppression

Delete pulse.rs:197-206. `assemble_alerts()` returns raw `fired_today` regardless of freshness state.

**Wave, not wedge (Kade's catch).** Removing suppression while consumers still have no age-awareness leaves the system in a parallel-primary state: old filter gone, new reasoning unlanded, consumers running an age-filter band-aid. Avoid. The wave ships together:

1. All 5 resolve-on-recovery swats (index-freshness, fuseki-stale, lancedb-stale, tunnel, vikunja-auth-failure) — cooldown removed when condition clears.
2. Suppression block delete in pulse.rs.
3. `context_inject.rs` consumer update — reads `sources.alerts.*.age_secs` when shaping the agent boot envelope.
4. `tiles.ts` consumer update — Clearing tile renders age per alert, not just the list.

Any subset is worse than none. This is not a retrofit opportunity.

**Move resolution to the producer.** The freshness-alert scripts own the cooldown file. When the alert script next fires and finds the underlying condition resolved, it `rm`s its own cooldown file. That is where "this is no longer firing" belongs.

**What this exposes:** the agent-boot "alerts: 1" line will sometimes read `alerts: 3` with two stale ones. That is *correct* — the agent should notice, either clear them itself or note "two alert cooldown files are stale; I'll treat as resolved." Visible state, visible reasoning.

**sources.alerts — not a nothing-field.** The first draft set `alerts.age_secs: 0, note: scanned at pulse time` — Kade: "that entry carries no information." Fix: `ts = max(mtime of any /tmp/alert-* cooldown file)` so age reflects the freshest alert fire, not pulse-assembly time. If no cooldown files present, omit `sources.alerts` entirely.

## Proposed Change — pulse.rs Comment Block

**Ships in the same commit as the suppression delete** (Wren's A). A comment claiming "does not filter" over code that still filters is competing language-games — readers trust the wrong half. One commit, one contract.

Replace lines 1-4:

```rust
//! Pulse service — event-bus cache over team-state producers (#1881, #2280).
//!
//! Each section name in pulse-latest.json is owned by a separate upstream producer
//! writing on its own cadence. Pulse reads, annotates with age (per-source timestamps
//! in `sources`), and exposes. It does not filter, suppress, or synthesize.
//! Consumers decide what to act on given raw state and freshness.
//!
//! Producers: role_state.rs, observer.rs, chorus-log, alert scripts, nudge CLI,
//! deep-health.sh, cards CLI, /api/chorus/freshness. See designing/docs/pulse-service-design.md
//! for the full inventory.
//!
//! Target: <200ms. All file reads, no shell spawning.
```

## Sub-Domain Interaction Model

### Pulse ↔ Context (from context-service-design.md)

Context consumes Pulse as one of its push-envelope inputs. The Context thesis (2026-04-19) is shrinking the envelope and shifting to pull. Pulse stays in push for orientation — "here's where you are" — but any *claim* about current state belongs on a pull endpoint (`/api/chorus/context/board/*`, etc.). Per-source timestamps are what let Context's pull endpoints stamp response envelopes with accurate `as_of`.

### Pulse ↔ Alerts (grafana-alerts + scripted alerts)

Alerts are one of the producers that currently leaks suppression logic upward. This design pulls that boundary clean: alerts own their cooldown files end-to-end, including clearing them on recovery. Pulse surfaces the set; it does not redact.

### Pulse ↔ Roles

Role state has two producers — declared (role-state CLI, per-transition) and inferred (observer hook, per-tool-use). The current pulse shape composes them into one flat `roles.{role}` object with `inferred_stale` and `divergent` flags. That composition stays — it's the right shape for consumers. Per-source timestamps expose the TTLs (5-min inferred, unbounded declared) that the composition currently enforces invisibly.

### Pulse ↔ Health

`deep-health.sh` writes on a 5-min LaunchAgent cadence. Pulse re-serves verbatim. A consumer reading `health.tunnel = ok` at 14:30 is reading a state that was measured as late as 14:25. With per-source timestamps, consumers can decide if that's good enough. Agents forming claims like "tunnel is healthy" without noting the measurement age will show up in session transcripts as drift — a direct way to close the loop on #2193-style coherence checks.

## In Scope (wave — #2442)

Revised after Wren + Kade review. All of these land together or none:

- Sidecar `sources` object with per-section `{ts, source, age_secs}`
- `schema_version: 2` top-level field
- Delete `assemble_alerts()` suppression block (pulse.rs:197-206)
- pulse.rs header comment reframe (same commit as delete)
- 5 resolve-on-recovery swats (index-freshness, fuseki-stale, lancedb-stale, tunnel, vikunja-auth-failure)
- `context_inject.rs` reads `sources.alerts.*.age_secs` in boot envelope
- `tiles.ts` renders per-alert age in Clearing tile
- `sources.alerts` uses max cooldown-file mtime, or is omitted when no alerts present

## Out of Scope (follow-on cards)

- **`getPulseAge(pulse, 'roles.wren.declared')` helper in chorus-sdk.** Flat dot-path keys are fragile across renames (Kade). One place to know the convention, typo fails loud. Files as separate SDK card.
- **Schema guarantee for `alerts`.** Today alert state is "list of alert names parsed from filenames." A structured alert record (`{name, fired_at, severity, resolved}`) is a bigger change — belongs to alerts-domain design.

## Demo Shape

Jeff opens `/tmp/pulse-latest.json` after this lands and sees:
1. A new top-level `sources` object with 8 entries, each with `ts` + `age_secs`.
2. `alerts.fired_today` lists every cooldown file present on disk — no hidden filter.
3. `pulse.rs` top-of-file comment describes event-bus contract, not aggregator pattern.
4. No consumer breaks (tiles.ts, context_inject.rs, chorus-pulse-latest.ts all keep rendering).
5. A follow-on card is filed for each alert script that needs resolve-on-recovery.

## References

- `platform/services/chorus-hooks/src/commands/pulse.rs` — current implementation
- `platform/services/chorus-hooks/tests/pulse_service.rs` — current tests
- `designing/docs/context-service-design.md` — Context boundary
- Prior art: #1881 (original pulse), #1889 (suppression bandaid), #2120 (post-tool-use refresh), #2168 (per-section timings)
