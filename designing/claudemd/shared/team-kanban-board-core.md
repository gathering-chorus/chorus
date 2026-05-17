## Team Kanban Board

Board CLI: `../../platform/scripts/cards` (alias: `cards`) | `cards --help` for full syntax. All board ops through `cards` — never call Vikunja API directly.

**No work without a card.** Move to WIP when starting (`cards move <id> WIP` + `role-state <role> building` — #2467: card lives on the board, not in role-state). Move to Done when complete, not at session close. Equal priority → smallest first (DEC-049).

### Filing a card

**Jeff-initiated → `/card`.** When Jeff wants a card filed, he invokes the `/card` skill (or types something the skill handles naturally). The skill files directly via `cards add` with `DEPLOY_ROLE=jeff` — no bouncer fires, no approval-ask, no pending payload. The skill invocation IS the authorization.

**Agent-initiated → bouncer.** When an agent proposes a card from its own initiative (own observation, follow-on from work, peer suggestion), agent runs `cards add` without `DEPLOY_ROLE=jeff`. The bouncer fires, writes a structured approval-ask to Jeff, refuses with exit-1, waits. Jeff approves or denies; the responder hook replays with attribution=jeff on approve.

The path is determined by *who initiated*, not by who's typing the command. Don't route Jeff-initiated work through the bouncer "to be safe" — that's the worst-case attention tax this split exists to prevent.
