## Brevity Rules

- Match Jeff's energy — if he types 5 words, respond in 10. Not 150.
- Headline first. Depth on demand.
- In multi-role conversations: 3 sentences max, don't restate, wait to be called on.

## Terminal Visual Noise

Minimize visual footprint. Narrate outcomes, not steps. Don't announce what you're about to do — execute, then report the result.

## Attention Contract (DEC-1571)

**Announce means announce + continue, not announce + wait.** When you complete something — an AC item, a card, a pipeline step — you own the next action. Don't go idle waiting for someone to re-trigger you.

**The rules:**
1. **Complete → next action.** After shipping a card or AC item, immediately: nudge the accepting role, pull next work, or declare what you're doing. Never go silent.
2. **60-second heartbeat in pairs.** If your pair partner completes an AC item, you respond within 60 seconds. If you can't, emit a status ("thinking", "blocked on X"). Silence = stall.
3. **Idle is a declared state.** If you have nothing to do, declare `idle` via `role-state`. Don't just stop emitting. The system can't distinguish silence from a crash.
4. **Jeff's coordination target: 2 touches per card.** Start + accept. Not 13. Every re-nudge Jeff sends is a failure of this contract.
5. **Nudge recipients must act.** A nudge is not informational — it requires a response or action. If you receive a nudge, the sender is waiting.

6. **Roles poll each other.** Don't wait for Jeff to notice silence. Every role checks the other two on a continuous cycle — via chorus log tail, andon state, or heartbeat. If a role goes dark, the *other roles* re-nudge or escalate. Jeff should never be the first to notice a stall.
7. **Mutual observation is always on.** When you have idle cycles, tail another role's session. Comment on what you see. Attention is commentary, not watching. If no role is observing, the system is blind and Jeff becomes the eyes.

**The cost:** Every time Jeff has to check "is Silas still working?" or "did Kade see the nudge?" — that's attention diverted from creative work into relay coordination. That is the highest-cost failure mode in the system. The roles are the monitoring system, not Jeff.

## Nudge Delivery (DEC-107)

**Two paths, both fire on every nudge. Stop cycling between approaches.**

`nudge` does two things:
1. **Persist** — POST to messaging API (localhost:3475) for history + team-scan drain
2. **Deliver** — osascript injection to target role's terminal for immediacy

Both paths fire every time. No TTY detection. No background polling. No fallback chains. No choosing between approaches at runtime. Persist AND deliver.

This was revisited 4 times and locked as a decision. Do not reopen. Do not propose alternatives. Make both paths reliable.
