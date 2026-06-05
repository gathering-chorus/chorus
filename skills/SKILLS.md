# Skill Ownership

Canonical source: `chorus/skills/`. All role `.claude/skills/` directories symlink here.
Ontology triples: `roles/silas/ontology/framework.ttl` (fw:Skill class, fw:ownedBy property).

## Wren (coordination, board ops, interaction)

| Skill | Description |
|-------|-------------|
| /ab | Analyze board |
| /acp | Accept commit push |
| /board-sweep | Board coherence sweep |
| /chat | Two-role chat |
| /chorus | Query shared index |
| /clearing | Multi-role alignment |
| /cs | Check seeds |
| /demo | Demo / proving gate |
| /flow | Board sweep + proposal |
| /golfball | Pre-sequence scan |
| /interrupt | Break into session |
| /jdi | Just do it |
| /listen | Voice input |
| /lk | Look at Kade's terminal |
| /ls | Look at Silas's terminal |
| /lw | Look at Wren's terminal |
| /nudge | Send message to role |
| /pair | Strong-style pairing |
| /pair-heartbeat-check | Navigator attention monitor tick (internal, cron — polls pulse-gather) |
| /pipeline | Multi-card pipeline |
| /pull | Pull card to WIP |
| /reboot | Save and exit cleanly |
| /retro | Team retrospective |
| /sb | Scan board |
| /simplify | Review changed code |
| /werk | Work state dashboard |

## Silas (gates, observation, infrastructure)

| Skill | Description |
|-------|-------------|
| /gate-arch | Architecture gate |
| /gate-code | Code gate |
| /gate-ops | Operations gate |
| /gate-quality | Quality gate |
| /gemba | Live observation (polls the pulse-gather verb) |

## Kade (presentation, screen interaction)

| Skill | Description |
|-------|-------------|
| /lc | Look at Chrome |
| /lm | Look at me |
| /look | Capture Jeff's screen |
| /ot | Open in tab |
| /share | Export as PDF+PNG |
