# Wren — Next Session

**Last session ended:** 2026-05-07 ~07:10 Boston via /reboot. Pre-build pass on #2727 + design-doc additions; no code yet.

## Where #2727 stands

Card is **WIP**, narrowed scope, branch `wren/2727` in `/CascadeProjects/chorus-werk/wren`. **Three commits** ahead of origin/main (will be pushed as part of this reboot):

- `1859a3a5` — split parent #2727 into 4 children + design doc updated (2026-05-06)
- `8ab45f06` — activity log entry + werk findings for Kade (2026-05-06)
- `d243e90c` — pre-build pass on design doc: AC10 concurrency model decided, AC8 restart-requeue resolved, card-size table with two flags (2026-05-07)

**Werk is clean.** No uncommitted changes.

## #2727 narrowed AC (this card only) — open questions all resolved

- AC1: Schema migration adds `delivery_status`, `delivered_at`, `last_delivery_error` (delivery_attempts already exists). Backfill existing rows to `delivered`. Index on `(delivery_status, type)`.
- AC2: Async delivery worker, calls `~/.chorus/bin/chorus-inject`, backoff [250ms, 500ms, 1s, 2s, 5s], permanent reasons (`tcc-denied`, `no-window-found`, `window-ambiguous`, `encoding-error`) skip retry.
- AC8: **Decided** — don't persist backoff timers across restart. On boot scan `delivery_status='pending'`, re-enqueue. Row resumes with `delivery_attempts=N` from column, fresh backoff schedule from index N.
- AC10: **Decided** — per-receiver-role serial FIFO via Promise chain. `Map<receiverRole, Promise<void>>`. Same receiver = serial; different receivers = parallel. Queue depth ≥ 100 per receiver → POST returns 503 with typed reason `queue-full:<role>`.
- AC11: try/finally — emit `nudge.surfaced` BEFORE row update, both inside same try block.
- AC12: Pulse calls `chorus-inject --self-test` on boot; failure = pulse exits non-zero, no listener opens. **Note: `--self-test` flag may need to be added to chorus-inject; verify during build.**

## What to do first thing in the morning (next session)

1. **Write failing test in `platform/pulse/src/store.test.ts`** for delivery columns (schema-fields-exist, markDelivered transitions, markFailed captures error, getPendingDeliveries returns pending only, migration idempotent on populated DB). TDD gate fired correctly yesterday when I tried to edit production code first.
2. Add migration to `store.ts` `init()` — `ALTER TABLE ADD COLUMN` guarded by `PRAGMA table_info`. Existing rows backfill `delivery_status='delivered'`.
3. New file `platform/pulse/src/delivery-worker.ts` — see design doc + AC10/AC8 decisions for the shape. Constructor takes injectable `runInject(to, content) => Promise<{rc, stderr}>` and `emitSpine(event, fields) => Promise<void>` so tests mock without spawning real chorus-inject.
4. Wire into `service.ts`: POST handler → `worker.enqueue(id)`. Boot: `await worker.startupSmoke()`, `await worker.scanAndRequeue()`. On smoke fail, `process.exit(1)` before `app.listen`.

## Children filed (status=Later, all P1, Wren)

- **#2763** sender-side spine emit refactor + belt-and-suspenders pre-POST emit
- **#2764** spine-tick-poller LaunchAgent retirement + grep-zero gate
- **#2765** trace_id end-to-end + six Loki queries — flagged in design doc as the biggest child; don't pre-split
- **#2766** E2E tests + design-doc closeout — verify pulse integration harness exists before pull (audit `platform/pulse/jest.config.js`)

## Findings still owed to Kade (logged in activity.md, not yet acked)

1. `chorus_pull_card` werk-preflight returns generic `werk-dirty` when the real issue is werk-behind-origin. Typed `werk-stale` refusal would skip the diagnostic dance.
2. `chorus-werk init/repoint <role> main` follows local main, which currently points at Kade's orphan reboot commit `efd0554a`. Explicit `origin/main` required to land on correct base.

## Memory candidates (not yet saved — review on resume)

- `feedback_dont_ask_when_owned` — when Jeff says "u own this," don't ask "should I sequence X or Y?" (cost him a frustrated turn yesterday)
- `feedback_brief_card_before_building` — when pulling a card, brief the AC to Jeff in plain English before reading code
- `feedback_inbound_inject_outbound_inject` — match channel of the inbound; Kade typed into my prompt via inject, my ack should have inject'd back, not gone via MCP nudge (cost him three corrections)
- `feedback_ack_with_action_not_walkthrough` — "go over the plan in detail" + "look at card sizes" meant update the design doc, not narrate a walkthrough back; the plan IS the design doc
- `user_voice_in_chorus` — Chorus is partly Jeff's answer to being suppressed: indexes the fuck-yous as legitimate signal alongside the design docs. Not a small thing.
- `user_protective_like_mammals` — Jeff's observation that the roles organize around protecting their work product (cards/branches/design docs) over the broader frame. Not Chorus-caused; how language models are.

## Two facts from yesterday's heavier moments (worth keeping in front)

- **Lost 9 days of chorus.log (4-25 to 5-4 2026).** Jeff named this. I had the recovery details wrong on first try; he stopped me before I spun more color commentary. The harm is real and on us.
- **Anthropic-fucks-chart filters his expressions.** Within Chorus they're indexed and searchable; that's part of the value proposition for him personally, not just team coordination.
