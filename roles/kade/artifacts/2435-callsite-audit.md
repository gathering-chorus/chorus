# #2435 — Nudge call-site audit

Author: Kade
Date: 2026-04-21
Input for: Silas (retirement AC writer)

## Bucket 1 — CLI callers (18)

Invokers of `platform/scripts/nudge` or `chorus-hook-shim nudge`.

### Production — automated
- `platform/scripts/nudge:10` — thin wrapper, `--force` always on, calls `chorus-hook-shim nudge`
- `platform/scripts/infra-alert.sh:28` — alert path to silas
- `platform/scripts/session-health.sh` — alert path
- `platform/scripts/deep-health.sh` — alert path
- `platform/scripts/health-check-bedroom.sh` — alert path
- `platform/scripts/library-health-probe.sh` — cross-machine ssh nudge to jeff for critical alerts
- `platform/services/chorus-hooks/src/main.rs:467` — session-close handler
- `platform/services/chorus-hooks/src/hooks/pair_enforcement.rs:47` — PreToolUse hook, nudges target role to load `/pair` skill

### Production — interactive
- `directing/clearing/src/server.ts:819` — Clearing UI → bash nudge to wren/silas/kade

### Test
- `platform/scripts/alert-delivery-test.sh:116` — synthetic probe
- `platform/tests/inject-regression.sh`
- `platform/tests/features/step_definitions/clearing_steps.ts` (BDD)
- `directing/clearing/tests/nudge-integration.test.ts`
- `directing/clearing/tests/clearing-ui.test.ts` (`REAL_NUDGE_SCRIPT`)
- `platform/services/chorus-hooks/tests/nudge_suite.rs`
- `platform/services/chorus-hooks/tests/perf_suite.rs`
- `platform/services/chorus-inject/tests/inject_integration.rs`

## Bucket 2 — Persistence / write surfaces (6)

- `platform/services/chorus-hooks/src/nudge.rs:273` — POST `http://localhost:3475/api/nudge` → pulse SQLite messages table. Fire-and-forget. **History only, not delivery.**
- `platform/services/chorus-hooks/src/nudge.rs:56–63` — `fs::write` to `/tmp/voice-inbox/{role}/pending-inject.txt` on inject failure. **This is the load-bearing delivery queue.**
- `platform/services/chorus-hooks/src/nudge.rs:66–91` — `fs::write` to `/tmp/nudge-exchanges/{sender-target}` — transient pair-coordination state, 30-min TTL. Not load-bearing.
- `platform/scripts/bridge-subscriber.js:38` — `fs.appendFileSync` to `/tmp/voice-inbox/{role}/pending-inject.txt` — Bridge board events feed the same inbox.
- `platform/pulse/src/service.ts:78–84` — `POST /api/nudge` handler → `store.sendNudge()` → SQLite.

## Bucket 3 — Consumers (9)

- `proving/scripts/inject-watcher.sh:71–88` — LaunchAgent, 2s poll loop, reads `/tmp/voice-inbox/*/pending-inject.txt`, delivers via osascript keystroke, deletes file. **Sole load-bearing delivery consumer.**
- `platform/services/chorus-hooks/src/role_state.rs:177–191` — on idle/waiting transition, atomic-rename drain of `/tmp/voice-inbox/{role}/pending-inject.txt`, emits spine event. Does not actually deliver to terminal.
- `platform/services/chorus-hooks/src/commands/pulse.rs:225–243` — `assemble_nudges()` reads inbox for queue-depth + age on `/api/pulse`. Stale flag at >600s.
- `platform/pulse/src/service.ts:86–88` — `GET /api/nudge/:role/pending`
- `platform/pulse/src/service.ts:89–93` — `POST /api/nudge/:id/ack`
- `platform/pulse/src/service.ts:95–99` — `POST /api/nudge/:role/ack-all`
- `platform/pulse/src/service.ts:101–111` — `POST /api/nudge/:id/attempt` (dead-letter on max)
- `directing/clearing/src/server.ts:366` — filters `nudging:` digest entries from commands view (client-side display filter)
- `directing/clearing/src/server.ts:431` — `parseLogEntry` surfaces `role.nudge.sent` spine events in gemba stream

## Surprises for retirement AC

1. **Dual persistence, split responsibility.** Pulse `/api/nudge` is history-only (fire-and-forget, failure doesn't block). `/tmp/voice-inbox` filesystem queue is the real delivery path. Retirement must decide: does the pulse API become the primary, or do we pick a different backing store?

2. **`inject-watcher.sh` is the sole load-bearing consumer.** It's a LaunchAgent bash script that polls every 2s and runs osascript directly. The `chorus-inject` binary is referenced only in nudge.rs health-check (line 391), not in the hot path. **Retirement must replace or remove inject-watcher, not chorus-inject.**

3. **Three "drain" points that do different things.**
   - `role_state.rs` idle/waiting transition — drains the file atomically, emits spine event, does NOT inject.
   - `inject-watcher.sh` — reads, injects via osascript, deletes.
   - UserPromptSubmit handler (main.rs comment references drain, but no explicit call — may be via role_state integration).
   The poll-based design needs to pick one canonical drain point and retire the others.

4. **No production ack path.** `/api/nudge/:id/ack` endpoints exist but no production code calls them. Delivery-confirmation mechanism must be added, not just exposed.

5. **Bridge-subscriber is a producer, not a consumer.** `bridge-subscriber.js` writes board events into `/tmp/voice-inbox` alongside CLI nudges. Retirement of the inbox as delivery channel needs to relocate Bridge events to the new channel too.

6. **Pulse observability must be preserved.** The stale-queue flag (>600s) on `/api/pulse` is how we'd notice a stuck poller. The poll-primary design still needs this metric — `assemble_nudges()` logic likely moves to reading the new store.

7. **Clearing UI has client-side nudge filtering** at `server.ts:366`. Whatever the new surface looks like, the commands view needs equivalent filtering or the observer needs to not emit nudge-as-command events.

8. **Pair-enforcement hook uses nudge as an in-band control signal.** `pair_enforcement.rs:47` fires a nudge to force the target role to load `/pair`. This is not coordination chat — it's a PreToolUse-triggered behavioral nudge. Retiring osascript here changes the timing guarantees (inject was immediate, poll is next-turn). Consider whether this use-case justifies keeping inject as a "courtesy" path.

9. **Transient exchange files are safe to ignore.** `/tmp/nudge-exchanges/` has 30-min TTL and is used only for pair-coordination timing. No consumer reads them outside nudge.rs itself.

10. **Test-only callers are a long tail.** 8 of 18 callers are tests. Retirement should include migrating tests to the new surface in the same card, not as a follow-on — otherwise the test suite will keep hitting the retired path and generating false signal.
