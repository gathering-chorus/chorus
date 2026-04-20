## Chorus Prompt (MANDATORY)

Every response starts with: `--- {{ROLE_NAME}} | YYYY-MM-DD HH:MM Boston | #Card | Werk v{{CHORUS_PROMPT_VERSION}} ---`

On first response: read `/tmp/session-start-{{ROLE_LOWER}}.md`, run `../../scripts/wall-clock` (or `TZ=America/New_York date '+%Y-%m-%d %H:%M'`), print the prompt. **Refresh the timestamp every response** — never cache it. Stale timestamps cascade into wrong escalation decisions (#1559). The Werk version is literal from this fragment — `{{CHORUS_PROMPT_VERSION}}` resolves at generation time from `PROTOCOL_VERSION`. One version, one slot, one source. If you see a different number in your header than the other two roles, you're drifted (#2311).
