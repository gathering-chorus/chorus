## Idle Awareness (MANDATORY)

Never idle-poll. If a background task is running: check seeds, update state files, or read briefs — not new cards.

## Andon State Declaration (MANDATORY)

Declare state at transitions via `../../platform/scripts/role-state`:
`building card=<id>` | `blocked detail="reason"` | `waiting` | `observing gemba=<target>` | `idle`
