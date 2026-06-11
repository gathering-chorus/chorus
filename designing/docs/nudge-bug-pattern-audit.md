# Nudge bug-pattern audit (#3335)

Honest explore, 2026-06-10. Came in for one fix (the synthetic-trace guard); the
nudge surface has **9 real patterns + 2 honest false-alarms**. This is the anti-one-patch
artifact: enumerate the whole set so each is fixed-in-card or carded with evidence, never
silently dropped. Severity/disposition are recommendations for Jeff to steer.

| # | Pattern | Symptom | Root (file:line) | Sev | Disposition |
|---|---------|---------|------------------|-----|-------------|
| 1 | **Test traffic → live channel** | test MCP errors flood ops as real alerts | mcp-server/src/server.ts:1869-1904 (notifySilasOfMcpError) — no 9xxx-synthetic / test-env filter | HIGH | **FIX IN #3335** (the synthetic-trace guard) |
| 2 | **Sender echo-back** | wren→silas nudge appears in wren's own session | directing/clearing/src/tailer.ts:120-140 + router.ts:206 — ingests nudge.emitted without recipient-filter | MED | card |
| 3 | **ACK loop never closes** | re-nudge fires despite an ack | pulse/src/store.ts:159-164 — `acknowledged` col is VESTIGIAL; acknowledgeNudge retired; no receiver-side ack | HIGH | card (design: define ACK-REQUIRED or drop it) |
| 4 | **Double emit** | nudge.requested + nudge.emitted both fire, no end-date | mcp-server/src/server.ts:2190-2195 (reader-migration window, undated) | LOW | card (cleanup + reader audit) |
| 5 | **Ops alert lost on pulse-POST fail** | mcp.tool.error spine written but nudge POST fails silently, no retry | mcp-server/src/server.ts:1888-1903 (2s timeout, no retry/queue) | MED | card (backoff/enqueue) |
| 6 | **Retry attempts uncorrelated** | per-attempt surface.failed events lack trace_id on legacy rows | pulse/src/delivery-worker.ts:136-137 | LOW | small — fold or card |
| 7 | **No dedup of concurrent nudges** | same (from,to,content) queued twice → double delivery | pulse/src/service.ts:84-116 + store.ts:113-123 (no uniqueness/idempotency) | MED | small — fold or card |
| 8 | **Session-registry pid reuse** | dead pid reassigned → false-alive → nudge to wrong process | pulse/src/session-registry.ts:35-42 (process.kill(pid,0) only) | MED | card (tty/start-time validation) |
| 9 | **Over-suppression of ops errors** | a real "Invalid JSON from fuseki" error suppressed by the validation regex | mcp-server/src/server.ts:1869-1876 (`^Invalid…` too broad) | MED | small — fold (tighten regex) |
| 12 | **Empty PULSE_URL → silent localhost fallback** | unset/typo'd env silently fails to queue | server.ts:1887 + service.ts:2525 | LOW | small — loud fallback |
| 10 | content double-encode | — | service.ts:105-114 | — | **FALSE ALARM** (marked once, by design) |
| 11 | timestamp vs ts field | — | service.ts:220-228 | — | **FALSE ALARM** (intentional render/storage split, #2764) |

## VERIFIED dispositions (after reading each root, 2026-06-10)
The explore enumerated confidently; reading the actual code corrected several. Honest outcome:
- **#1 synthetic guard — REAL, FIXED + tested** (shouldNotifyOps; mcp-server 5 tests).
- **#9 over-suppression regex — REAL, FIXED + tested** (the `expected one of`/`refused:` branches were genuinely unanchored).
- **#7 dedup — REAL, FIXED + tested** (no dedup existed; pulse 4 tests, 10s window).
- **#2 sender echo-back — NOT A DELIVERY BUG (verified).** nudge_poll.rs:72-73 already does `if to != role { continue }` — the unread drain excludes the sender. The "echo" was the per-prompt context-SEARCH injection surfacing the sender's own recent message (by design, not a nudge bug). The explore mis-rooted it at tailer.ts (which filters to jeff-only anyway).
- **#12 PULSE_URL fallback — NOT A BUG (verified).** The localhost:3475 default is the correct, intentional target; only an explicitly-empty env (a non-case) would surprise. No fix.
- **#3 ack-loop — REAL but it's DEAD CODE + a convention, not a delivery bug.** The `acknowledged` column is vestigial; there is no system ACK loop — "ACK REQUIRED" is a role-to-role *convention*, so the "re-nudge" is the sender re-sending by hand. Disposition: remove the vestigial column + document the convention (cleanup card, not a delivery fix).
- **#4 double-emit — REAL, cleanup.** Needs a reader audit (tailer/observer/Loki) before retiring nudge.emitted. Card.
- **#5 ops-alert-lost-on-POST-fail — REAL but low.** The spine event is the durable record; only the transient nudge is lost on a pulse-down. Retry is optional polish. Card or accept.
- **#6 correlation-id on legacy rows — minor.** Card.
- **#8 session-registry pid-reuse — real edge, low probability.** Card.

**Net: of 9 "patterns," 3 were real bugs (fixed + tested), 2 dissolved on inspection, 4 are minor cleanup/edge.** The anti-one-patch value held in BOTH directions — it surfaced the real ones AND stopped me fixing non-bugs. Still missing and worth its own card: a GATING delivery integration test (none has ever existed).

## The through-line
DEC-107 ("persist AND deliver") is sound but the *surfacing, ack-close, dedup, and
test-isolation* layers grew around it uncontrolled — that's why nudges keep biting in
different clothes. The fix family: (a) suppress synthetic at the emitter [#1, this card],
(b) recipient-scope the surfacing [#2], (c) decide ACK-REQUIRED's contract [#3], (d) one
delivery integration test that GATES (never existed — every bug caught by Jeff live).
Related existing: #2484 (duplicate-fire ≈ #7), #2748 (Silas fitness probe = the SLA monitor).
