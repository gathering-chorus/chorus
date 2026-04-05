# Kade — Next Session

## Accomplished 2026-04-05
- #2052: Clearing domain badge counts respect role filter — header uses filtered count when role filter active
- #2046: Clearing mobile UX — textarea, Enter=newline on mobile, focus not stolen by re-renders
- #2063: Clearing message routing — removed keyword routing, un-addressed messages go to Wren only
- #2058: Carded role-state stale card bug → Silas fixed and shipped same session
- #2072: Sexuality player integrated into Gathering at /sexuality/player, behind auth, LAN-direct media
- Sexuality player: added resizable divider + photo strip thumbnails, restarted on Bedroom after reboot
- Deep code review of chorus-hooks Rust with Silas — 37 gate hooks reviewed, joint report filed
- Feedback on 4 Silas cards (#1964, #1873, #2059, #2032, #2075)

## WIP
None.

## Next cards
- #1865 Photo detail shows thumbnail instead of full image (P2)
- #1631 Name face clusters (P3)
- #1630 Rebuild semantic embeddings (P2)

## Pending
- LaunchAgent brief for sexuality-player sent to Silas (architect/briefs/2026-04-05-sexuality-player-launchagent.md)
- Chorus-hooks code review — top fixes: shared utils for duplicated logic, state_paths.rs, fix infra_guardrails git detection

## Notes
- Sexuality player on Bedroom:8090 via nohup — dies on reboot until LaunchAgent is set up
- Gathering /sexuality/player makes direct LAN calls to Bedroom:8090 (no proxy for media streaming)
- parseTarget keyword routing was silently misrouting messages — "test" → kade instead of wren. Deleted.
