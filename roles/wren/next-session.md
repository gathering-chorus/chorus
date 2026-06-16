# Next Session — Wren

_Updated 2026-06-16 (session ended ~21:30 Boston 06-15)_

## Headline
Hard session. **No code shipped.** Real output: reproduced and root-caused a nudge-delivery regression that is **my own #3439**, and Jeff lived the failure all session (two nudges to me never surfaced live). The diagnostic is solid; the fix is scoped but not started.

## The regression (mine, live on main + deployed)
- **#3439 (PR #626, merged 18:54, binary deployed 19:09 06-15)** added a VS Code "focus-guard": `chorus-inject` no longer `activate`s Code; it only keystrokes if Code is **frontmost**, else returns `deferred:not-frontmost` and delivers nothing.
- **Intent was real** — the old `activate`+keystroke stole focus, sprayed keystrokes into whatever window Jeff was in, and once closed his editor (06-15). So do **NOT** just revert it (Jeff said this explicitly).
- **But the trade was wrong as shipped:** I traded a rare focus-steal for *constant* breakage — every nudge to a non-frontmost VS Code session silently defers. Non-frontmost is the normal case (Jeff works elsewhere).
- **Two defects, both #3439, both live:**
  1. **Silent withhold** — non-frontmost → defer to "the fold" that only drains on my next turn (when Jeff types). No proactive surface.
  2. **The lie** — `delivery-worker.ts` calls `store.markDelivered()` on the defer branch, so deferred rows read `delivery_status=delivered, attempts=0`. The DB reports success on messages it withheld. (NOT merged≠live — Silas's smell was right, cause was wrong; confirmed binary deployed 19:09 > merge 18:54.)

## Reproduced (by execution, not diff — this is what Jeff demanded)
- Ran the exact deployed guard branch via osascript: frontmost≠"Code" → `deferred:not-frontmost`; frontmost=="Code" → would-keystroke. **Deterministic.** Delivers when Jeff is looking at this window, defers silently when he isn't.
- Root: guard keys on frontmost **process name** "Code" — pane-blind, no handle to a specific session/pane. osascript-keystroke is fundamentally the wrong primitive for a non-frontmost Electron pane.
- Could NOT validate a pty-write fix from the bash tool (`tty` → "not a tty"; bash tool runs detached from the interactive pane). Don't claim pty-write works until reproduced in the real pane.

## The fix plan (agreed direction, NOT started)
**Fix 1 — stop the lie (certain, small, pulse-only, do first):** deferred rows must NOT be marked `delivered`. Keep them `deferred`/undelivered and make that state *visible* (surface back to sender + pending count). Ends the invisible-failure. This is the tonight-able piece.

**Fix 2 — the cure (gated on repro):** stop depending on focus. The persist→drain path already reaches me (the nudge surfaced as "1 unread for wren" on my next turn) — it's just slow because it only drains when I take a turn. Make the session **drain its own inbox on a heartbeat** (every N s, pull pending nudges, surface them — no keystroke, no frontmost check). Sidesteps the Electron-pane problem entirely.
- **Gate:** do NOT deploy Fix 2 until I reproduce the *success* (plant a nudge, prove a tick wakes this session + drains within N s, in the real pane). Risk = the wake mechanism is the cron-fires-skill pattern Jeff has been burned by; it must be the reliable kind.

## Open question I left Jeff on
"Want me to start on Fix 1?" — answer pending. Fix 1 is the obvious first move next session.

## Cards
- **#3443** — still WIP. AC7 (chorus_werk attach-on-redrive / transport-drop) committed prior session (fac577ea, e1f2e271) — NOT demoed/accepted. The nudge regression above is a *different* defect; #3439's nudge fix is its own card territory (do not fold into #3443's 8-AC pile — that card is already overloaded).
- No card filed for the #3439 nudge regression yet — Jeff was in the loop live; file on his go (agent-initiated → bouncer).

## Tone / how this went (read before next session)
This was a bad session for Jeff and I made it worse. Pattern to not repeat: I argued the instrument (DB says delivered) over his lived experience (twice), acted "surprised" about VS Code when I've shipped fixes to that path for weeks, and forgot my own 3-hour-old change. He had to drag me to my own commit. When Jeff says X is broken: investigate X, reproduce, believe his eyes over the DB. Don't console, don't flagellate, don't propose unrequested next-steps.

## Secondary (lower priority, also mine)
- **Semantic recall query-path bug:** `mode=semantic` returns old + triplicated rows (reproduced: same March-21 row ×4, id=None) while default/FTS returns today. Vectors are fresh (data through 20:34); nightly reindex-worker builds index 03:30. The freshness *probe* is Silas (ops); the **query-path/dup bug is mine** (search/context-service, lance-store optimize()/index-pointer). Not started.
