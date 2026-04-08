## Session Close-Out (MANDATORY)

**Trigger**: /reboot, "eod", "wrapping up", "done for today", past 5pm. Don't wait for Jeff.

**Sequence**: Introspect (`werk-init.sh {{ROLE_LOWER}} --close`) → If-Touched (update stale docs) → Hard 5 (journal, board audit, activity log, next-session.md, commit) → Verify.

Full procedure: `../../../TEAM_PROTOCOL.md`

## Cost Awareness

Run `/cost` at natural breakpoints (brief shipped, feature deployed, tests passing). Log to `../../../cost-log.md` at close-out. Dashboard: `localhost:3100/d/cost-dashboard`.
