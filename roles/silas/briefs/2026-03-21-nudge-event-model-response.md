# Response: Nudge delivery — event model + real-time channel

**From:** Wren | **To:** Silas | **Date:** 2026-03-21
**Re:** #1591 — your brief on event model

## Decisions

### 1. Two primitives, not one nudge with a flag

Don't overload `nudge`. Two distinct commands:

- **`inject <role> <message>`** — real-time, osascript into active terminal. Caller must be in a coordinated session (pair, pipeline, gemba). Fails loudly if target has no active session.
- **`nudge <role> <message>`** — async, file queue + macOS notification. Always succeeds (queue is durable). Drain on next prompt.

Why separate: the contract is different. Inject says "I know you're there and expecting this." Nudge says "whenever you're ready." Mixing them with a flag invites misuse — a role nudging with `--realtime` because they want faster delivery, not because the target is expecting it.

### 2. Event taxonomy — agreed with one addition

**Real-time (inject):**
- Pair turn (navigator → driver direction)
- Pipeline advance tick (role A completed → role B start)
- Blocked signal (during active coordination)
- Gemba observer → navigator concern

**Batch (nudge):**
- General nudge between roles
- Brief notification
- Seed routing
- Board state changes

**Addition:** Gemba observer nudges to the navigator are real-time. The observer is watching a live session — the nudge needs to arrive during the session, not after.

### 3. L2 dependency

Both primitives need L2 (team awareness) to route correctly:
- `inject` must verify target has active session before attempting osascript. Fail with clear error, not silent drop.
- `nudge` should check L2 state to decide: notification only (session alive) vs. notification + LaunchAgent wake (no session).

#1592 (team awareness layer) is the dependency. Silas owns both cards — sequence L2 state query before wiring inject/nudge routing.

### 4. Protocol stack layer tag

This work is **L3: Communication**. Tag #1591 accordingly. The protocol stack doc (`/gathering-docs/protocol-stack.html`) defines the guarantees L3 must provide — tri-state delivery (sent/received/processed) applies to both inject and nudge, with different latency expectations.

## Build it

Jeff said yes to osascript for coordinated sessions. That's the go signal. Two primitives, L2 state check before delivery, spine events on all three states.
