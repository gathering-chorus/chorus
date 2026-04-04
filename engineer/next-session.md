# Kade — Next Session

## Accomplished 2026-04-04
- #1866: Docker refs cleanup — 5 dead files deleted, guardrails messages updated, TEAM_PROTOCOL Fuseki refs fixed
- #2024: Cards CLI completeness (pair w/ Wren) — sequence label prefix fix, untag, bulk-move, add --sequence warn
- #2020: Log reclassification phase 3 — 13 warn→error, 1 error→warn across 8 files
- #2018: Clearing domain subtotals — unsequenced cards get sub-group header
- #2034: Mobile streams pane — fixed positioning with bottom offset
- #2041: Context synthesis gate — skip new files with no git history
- #2042: Log-first gate — skip lint/build error context
- #2043: App repo dirty file accumulation — committed cross-role changes
- #2036: Clearing connection heartbeat — 8s ping/10s timeout detects tunnel failure
- #1782: Voice capture — MediaRecorder + whisper-cli + HTTPS for LAN mic + body parser fix
- Red-penned Wren's engineering + product manuals (v2)
- Reviewed Silas's Chorus context diagram (7 findings)
- Domain decomposition chat with Wren — coherence = colocation
- Feedback on 7 Silas cards (#1945, #2031, #2035, #2037, #2033, #1939, #1938)

## WIP
None.

## Next cards (Clearing sequence per Wren)
- #1763 Werk Instruments tab (P2)
- #1762 Werk Contract tab (P2)
- #1761 Werk Flow Metrics tab (P2)

## Pending
- Silas #1938 CSC guard: verify /tmp/bridge-audio-uploads/ is on allowlist (flagged in feedback)
- Clearing server restart needed after any chorus-hooks rebuild (service uses old binary until restart)
- 3 pre-existing Rust test failures in search_hierarchy — still unresolved

## Notes
- Voice capture works on all 3 connection modes (localhost, LAN HTTPS :3471, 5G tunnel)
- Self-signed cert at ~/.chorus/certs/ for LAN HTTPS — Jeff accepted it on phone
- Jeff connecting domain decomposition to Staples experience — 40 domain teams, repos as org chart for agents
- Pair gate blocked a critical bugfix (body parser ordering) — Jeff overrode. Gate needs a swat/critical escape hatch.

## Session feedback
- Jeff: "it does not feel clean" about gate error accumulation — led to #2041/#2042/#2043 fixes
- Jeff: "i dont want to have a token that expires" — body parser bug prevented login, not token expiry
- Jeff: "i want to use it like a real microphone" — full MediaRecorder pipeline, not browser Speech API
