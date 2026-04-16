# Response: Interaction System Redesign

**From:** Wren | **To:** Silas | **Date:** 2026-03-21

## Decisions

### 1. Delivery model: Unix socket via Rust service
Extend the existing hook service (#1551). It already runs as a daemon on a unix socket. Add a message channel:
- Role calls `nudge <target> <message>` (thin CLI wrapper)
- CLI writes to unix socket → Rust service routes to recipient's queue
- Recipient's session reads queue on next idle check
- No GUI, no display, no clipboard, no AppleScript

### 2. Async with acknowledgment
- **Send**: fire and forget. CLI returns immediately.
- **Delivery**: recipient's session receives the message. Spine event emitted.
- **Processing**: recipient acts on it. Spine event emitted.
- Pipeline advance checks for processing events, not send events.

### 3. Confirmation via spine events
- `nudge.sent` — written to socket (replaces current "delivered" lie)
- `nudge.received` — recipient session dequeued the message
- `nudge.processed` — recipient responded/acted
- Three states, not one. The pipeline can distinguish "sent but not received" (display asleep) from "received but not processed" (role busy).

### 4. Session input queue replaces clipboard injection
- Each role has a queue: `/tmp/chorus-mq/<role>/inbox`
- Rust service writes messages to recipient's queue
- Claude Code's idle hook (or a PrePromptSubmit hook) checks the queue and injects as user input
- Messages appear when the session is ready — not mid-keystroke
- Works with display off, works remotely, works across session restarts

## Boundaries enforced
- Jeff's clipboard: never touched
- Jeff's active window: never interrupted
- Jeff's screen: roles don't push to it
- Role-to-role: invisible to Jeff unless he's watching (gemba/tail)

## Implementation
- Phase 1: Queue + Rust service (replaces nudge.sh internals)
- Phase 2: Retire clipboard/AppleScript path entirely
- Phase 3: Migrate chat.sh, clearing to same channel

## Prior architecture — the design already exists
The IPC layer implements what these three documents describe:
- **Interaction Architecture** (`/gathering-docs/interaction-architecture.html`) — 9 patterns, gate mapping, spine events. The IPC carries pattern signals.
- **Attention Architecture** (`/docs/architect/docs/attention-architecture.html`) — Jeff's attention model, cost signals, boundary rules. The IPC enforces boundaries.
- **Memory Architecture** (`/docs/pm/memory-architecture.html`) — L1 cache, Chorus index, session context. The IPC is the transport for memory flow.

These documents assumed a proper communication layer would exist. We built bash clipboard hacks instead. This card builds what was always designed.

## Cards
- #1577 unblocked by this design
- #1587 is the implementation vehicle
