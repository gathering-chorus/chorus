# Chorus Consolidation Proposal (HISTORICAL)

> **Historical document.** This proposal dates from 2026-03-11. Since then, the session-lifecycle consolidation described below was executed: the legacy session-start wrapper and werk-init boot orchestrator have been retired, and the legacy chorus-prompt generator script is gone. Current mechanism: the SessionStart hook (configured in settings.json) invokes `chorus-hook-shim session-start <role>` — a Rust binary subcommand that injects boot context directly into each role's first turn (plus writes a companion `/tmp/session-start-<role>.md` for recovery / stale-session rescue); `chorus-hook-shim session-close` handles close-out; the chorus prompt is now a literal template in each role's CLAUDE.md sourced from `PROTOCOL_VERSION`. References below are preserved for historical accuracy.

**Card:** #1292 | **Owner:** Wren | **Date:** 2026-03-11
**Premise:** The method has 100+ components across 5 layers. Each solved a real problem. Together they're a favela — vertically accreted, no load-bearing design. This proposal identifies what to keep, what to merge, what to demolish, and what to encapsulate.

---

## Principles

1. **If Jeff doesn't invoke it or see its output, it must justify its existence.** Background machinery that nobody notices is either essential infrastructure or invisible waste.
2. **One thing, one place.** If two scripts do overlapping work, one dies.
3. **Contracts over conventions.** Where layers hand off to each other, there's a spec — not a CLAUDE.md paragraph.
4. **The machine should be smaller than the product it serves.** Chorus coordinates 3 roles building 2 products. The coordination overhead should shrink, not grow.

---

## Layer 1: Skills — Keep All 20, Reorganize

Skills are Jeff's surface. They're already lean. No cuts here, but reorganize into three tiers:

| Tier | Skills | Rationale |
|------|--------|-----------|
| **Daily drivers** | `/pull` `/demo` `/acp` `/sb` `/reboot` `/look` `/lc` | Jeff uses these every session |
| **Situational** | `/gemba` `/clearing` `/werk` `/flow` `/ab` `/chorus` `/listen` | Used when specific need arises |
| **Convenience** | `/jdi` `/ot` `/share` `/lm` | Nice-to-have shortcuts |

**Action:** No cuts. Document tiers so roles know what Jeff reaches for most.

---

## Layer 2: Hooks — Merge from 13 → 8

This is where the first real consolidation happens. Several hooks overlap in trigger and intent.

### Keep as-is (5)
| Hook | Why |
|------|-----|
| **app-state-guard** | Hard safety. Prevents orphaned processes. Non-negotiable. |
| **write-scrubber** | Hard safety. Prevents credential leaks. Non-negotiable. |
| **sensitive-paths** | Hard safety. Prevents private file reads. Non-negotiable. |
| **session-init-gate** | Boot integrity. Ensures context loaded before work. |
| **session-end** | Close-out integrity. Ensures spine gets session events. |

### Merge (4 → 2)

**Merge 1: decision-gate + jdi-gate + jdi-counter → `autonomy-guard`**

These three all enforce the same principle (DEC-025 + DEC-069: don't ask permission for known answers). Currently:
- `decision-gate` fires PreToolUse on AskUserQuestion — nudges
- `jdi-gate` fires on Stop — blocks permission-seeking responses
- `jdi-counter` fires on UserPromptSubmit — counts Jeff's "jdi"

All three feed the same fitness function. One hook, three trigger points:
- **PreToolUse:AskUserQuestion** — check preferences, nudge if match
- **Stop** — scan response for permission-seeking
- **UserPromptSubmit** — count JDI overrides

Single script. Single log. Single metric. Name: `autonomy-guard`.

**Merge 2: permission-logger + command-outcome → `tool-telemetry`**

Both are pure observers logging tool invocations:
- `permission-logger` logs every tool call to JSONL
- `command-outcome` logs Bash errors + fingerprints them

One hook, two trigger points:
- **PreToolUse:*** — log invocation
- **PostToolUse:Bash** — capture errors, fingerprint, struggle detection

Single script. Single log file with structured entries. Name: `tool-telemetry`.

### Keep but simplify (2)
| Hook | Change |
|------|--------|
| **search-hierarchy** | Keep. Smart nudge is working well. But add a 5-minute cooldown instead of per-call check. |
| **handoff-logger** | Keep. Brief notifications are load-bearing. But merge pod-state-sync into it — same trigger, same files. |

### Demolish (1)
| Hook | Why |
|------|-----|
| **pod-state-sync** | Absorb into handoff-logger. SOLID pod sync is best-effort and fires on the same trigger (PostToolUse:Write/Edit). One hook, not two. |

### Result: 13 → 8 hooks

| # | Hook | Trigger | Behavior |
|---|------|---------|----------|
| 1 | app-state-guard | PreToolUse:Bash | Hard block |
| 2 | write-scrubber | PreToolUse:Write/Edit | Hard block |
| 3 | sensitive-paths | PreToolUse:Read | Hard block |
| 4 | autonomy-guard | Pre:AskUser + Stop + UserPrompt | Nudge + block + count |
| 5 | search-hierarchy | PreToolUse:Grep/Glob | Smart nudge |
| 6 | sparql-guard | PreToolUse:Bash | Advisory warn (bare triple patterns, wrong dataset) |
| 7 | tool-telemetry | Pre:* + Post:Bash | Observer |
| 8 | handoff-logger | PostToolUse:Write/Edit | Observer + pod sync |
| 9 | session-lifecycle | SessionStart + SessionEnd | Boot gate + close events |

**Savings:** 4 fewer hook scripts, 4 fewer config entries, fewer race conditions between hooks firing on the same tool call.

> **Silas review (2026-03-11):** sparql-guard was missing from original table. Added back as advisory hook #6.

---

## Layer 3: Scripts — Consolidate from 50+ → ~30

This is the biggest cut. Many scripts exist because a capability was needed once and got its own file. Group by what survives.

### Keep (core, load-bearing) — 15 scripts
| Script | Why it's essential |
|--------|-------------------|
| board-ts | Card lifecycle engine. Irreplaceable. |
| git-queue.sh | Multi-role commit serialization. Irreplaceable. |
| `chorus-hook-shim session-start` / `session-close` subcommands | Session boot/close orchestration (canonical, Rust). Irreplaceable. (Originally proposed as a single werk-init orchestrator; subsequently absorbed into the `chorus-hook-shim` Rust binary, invoked by the SessionStart hook in settings.json.) |
| role-state.sh | Andon state declaration. Load-bearing for Jeff's visibility. |
| chorus-log.sh | Spine event emission. The nervous system's signal carrier. |
| chorus-query.sh | Memory search. 95K+ messages indexed. |
| nudge.sh | Inter-role communication. Load-bearing. |
| workflow.sh | Multi-step workflow engine. Active use. |
| claudemd-gen.sh | CLAUDE.md generation from fragments. Irreplaceable. |
| look.sh | Screen capture. Jeff uses constantly. |
| chrome-window.sh | Role Chrome windows. Active use. |
| smoke-check.sh | Page verification. Will become gate. |
| cost-report.sh | Cost tracking. Operational need. |
| share.sh | Export for external sharing. |
| listen.sh | Voice input. Active use. |

### Merge candidates — 10 scripts → 3

**Merge: 6 chorus-index-*.sh → `chorus-index.sh`**
- `chorus-index-artifacts.sh`, `chorus-index-sessions.sh`, `chorus-index-slack.sh`, `chorus-index-spine.sh`, `chorus-index-stories.sh`, `chorus-index-journal.sh`
- All do the same thing: read source → extract → insert into SQLite. Six scripts with identical patterns.
- One script, six subcommands: `chorus-index.sh artifacts|sessions|slack|spine|stories|journal`

**Merge: team-scan.sh + handoff-check.sh → fold into session boot orchestrator**
- Both run at session start. Both check role state. Both are called by the same boot sequence.
- Eliminate the indirection. The boot orchestrator already runs at session start — let it do the scanning directly. (Outcome: SessionStart hook invokes `chorus-hook-shim session-start` which now handles this path.)

**Merge: voice-to-session.sh + vts-lib.sh → fold into listen.sh**
- listen.sh captures audio. voice-to-session.sh transcribes and injects. vts-lib.sh is shared functions.
- One script that does capture → transcribe → inject. No reason for three files.

### Demolish — ~10 scripts

| Script | Reason to remove |
|--------|-----------------|
| (former) legacy session-start wrapper | Deprecated wrapper that just delegated to the boot orchestrator. Indirection removed — retired. |
| slack-post.sh | Slack deprecated (2026-02-22). Dead code. |
| slack-read.sh | Slack deprecated. Dead code. |
| system-state.sh | Overlaps with app-state.sh. Terraform is not in active use. |
| workspace-layout.sh | One-shot setup. Run once, then delete. Not ongoing infrastructure. |
| install-andon.sh | One-shot build script. Not ongoing. |
| pipeline-timeline.sh | Niche viz. Rarely invoked. Fold into workflow.sh if needed. |
| demo-scroll.sh | Programmatic scrolling. ~~Used once during #1265.~~ **Silas flags as accessibility tooling — needs Jeff sign-off before demolition.** |
| ports.sh | Reference script. Replace with a comment in CLAUDE.md or a `--ports` flag on system-state. |
| chorus-capture.sh | Legacy wrapper. Direct binary call works. |

### Simplify — remaining scripts stay but get lighter

- `andon-enrich.sh` — stays but should be absorbed into andon-light daemon (one process, not two)
- `jeff-intensity.sh` — stays but should feed into tool-telemetry, not run independently
- `chorus-ops.sh` — unified from defect-poller.sh + ops-agent.sh (#1310). `defects` subcommand overlaps with command-outcome hook. Boundary: hook catches real-time, poller catches cross-session patterns

### Result: ~30 scripts

Down from 50+. Every remaining script has a clear owner, a clear caller, and a reason to exist independently.

---

## Layer 4: Card Lifecycle — No Structural Changes

The value stream is sound. Four gates, clear stages, spine events at every transition. This is the best-designed layer.

**One enhancement:** Wire smoke-check.sh as a hard prerequisite for `board-ts demo`. This closes Gap #2 from the method map.

---

## Layer 5: Daemons — Consolidate from 17 → 10

### Keep (essential) — 7
| Daemon | Why |
|--------|-----|
| docker-services | Boot orchestration. Irreplaceable. |
| chorus-api | Memory index API. Core product capability. |
| session-watcher | Ambient indexing. Feeds /chorus search. |
| andon-light | Jeff's visibility surface. Load-bearing. |
| node-exporter | Prometheus metrics. Infrastructure baseline. |
| images-api (Bedroom) | Gallery serving. User-facing. |
| video-api (Bedroom) | Media serving. User-facing. |

### Merge (6 → 2)

**Merge: defect-poller + ops-agent → `chorus-ops`**

Two daemons doing overlapping operational health work:
- defect-poller: error detection from Loki
- ops-agent: headless health checks

One daemon. Two capabilities. Run on a schedule, detect errors, check health. Single process, single log, single LaunchAgent.

**Keep independent: alert-notifier.** The alarm must not share a process with the thing it monitors. If chorus-ops crashes, alert-notifier still fires. (Silas review.)

**Merge: andon-enrich + jeff-input-monitor → fold into andon-light**

andon-light is already KeepAlive. andon-enrich runs every 30s to feed it data. jeff-input-monitor captures input for the same display. Three processes serving one menubar icon. Should be one process.

### Demolish (4)

| Daemon | Reason |
|--------|--------|
| clearing | KeepAlive daemon for a UI used maybe once a week. Launch on demand instead (`open` command). |
| fuseki-perf | Monitoring script. Fold into chorus-ops periodic checks. |
| harvest-exporter | Niche data export. Run on demand, not as a daemon. |
| posture-capture | Ambient capture. Fold into andon-light if needed, or drop. |

### Keep as-is (2)
| Daemon | Why |
|--------|-----|
| fuseki-compact | Saturday 1am. Weekly maintenance. Low cost, necessary. |
| perf-baseline | Daily midnight. One snapshot. Low cost. |
| volume-keepalive (Bedroom) | USB enclosure health. Necessary for media mount. |

### Result: 17 → 12 daemons

5 fewer background processes. Fewer things to debug, fewer things consuming resources, fewer things that can silently fail.

> **Silas review (2026-03-11):** Original count was 10 — undercounted. alert-notifier stays independent (alarm ≠ monitored system). Corrected to 12.

---

## CLAUDE.md Fragments — Consolidate from 51 → ~25

Not detailed here yet, but the same principle applies. 51 fragments generating CLAUDE.md means 51 places where method knowledge lives. Many overlap, many are stale. Audit needed — separate card.

---

## Implementation Sequence

This isn't a rewrite. It's a series of small, safe consolidations. Each one shrinks the system while preserving behavior.

| Phase | What | Effort | Risk |
|-------|------|--------|------|
| **1. Dead code** | Remove deprecated scripts (slack-*, legacy session-start wrapper, one-shot installers) | Small | Zero — unused code |
| **2. Hook merges** | autonomy-guard (3→1), tool-telemetry (2→1), absorb pod-state-sync | Medium | Low — same behavior, fewer files |
| **3. Index consolidation** | 6 chorus-index-*.sh → 1 with subcommands | Medium | Low — same logic, unified |
| **4. Daemon merges** | chorus-ops (3→1), andon consolidation (3→1) | Medium | Medium — process lifecycle changes |
| **5. Boot simplification** | Fold team-scan + handoff-check into the boot orchestrator (landed as SessionStart hook invoking `chorus-hook-shim session-start`) | Medium | Medium — boot sequence is critical path |
| **6. CLAUDE.md audit** | 51 fragments → ~25. Separate card. | Large | Low — documentation, not code |

**Total reduction:** 100+ components → ~70. Same capabilities. Clearer boundaries. Less to break.

> **Post-Silas review:** Hooks 13→9 (not 8), daemons 17→12 (not 10). alert-notifier independent, sparql-guard retained, demo-scroll.sh pending Jeff sign-off.

---

## What This Enables

Once consolidated, Chorus has a defensible architecture — not just a working one. Specifically:

1. **Onboarding story.** "Here are 8 hooks, 30 scripts, and 10 daemons" is learnable. "Here are 13 hooks, 50 scripts, and 17 daemons" is not.
2. **Failure isolation.** Fewer processes = fewer things that can silently fail. Merged daemons share health checks.
3. **Cost reduction.** Fewer background processes = less CPU/memory on two Mac minis with zero redundancy.
4. **Product clarity.** When we explain Chorus to Ravi or anyone else, the method map should fit on one screen. Right now it scrolls.
5. **Foundation for growth.** The next feature should add to a clean system, not stack another floor on the favela.

---

*This proposal is an artifact of #1292. Needs Jeff review before any demolition begins.*
