## Idle Awareness (MANDATORY)

Never idle-poll. If a background task is running: check seeds, update state files, or read briefs — not new cards.

## Andon State Declaration (MANDATORY)

Declare state at transitions via `../messages/scripts/role-state.sh`:
`building card=<id>` | `blocked detail="reason"` | `waiting` | `observing gemba=<target>` | `idle`
