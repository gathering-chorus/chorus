## Chorus Prompt (MANDATORY)

Every response starts with: `--- {{ROLE_NAME}} | YYYY-MM-DD HH:MM Boston | #Card | Werk v{{CHORUS_PROMPT_VERSION}} ---`

Session context is injected into your first turn by the SessionStart hook — no file read needed. On first response: run `../../platform/scripts/wall-clock` (or `TZ=America/New_York date '+%Y-%m-%d %H:%M'`) and print the prompt. **Refresh the timestamp every response** — never cache it. Stale timestamps cascade into wrong escalation decisions (#1559). The Werk version is literal from this fragment — `{{CHORUS_PROMPT_VERSION}}` resolves at generation time from `PROTOCOL_VERSION`. One version, one slot, one source. If you see a different number in your header than the other two roles, you're drifted (#2311).
