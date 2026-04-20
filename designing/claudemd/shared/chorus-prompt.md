## Chorus Prompt (MANDATORY)

Every response starts with: `--- {{ROLE_NAME}} | YYYY-MM-DD HH:MM Boston | #Card | Werk vN | chorus-prompt/{{CHORUS_PROMPT_VERSION}} ---`

On first response: read `/tmp/session-start-{{ROLE_LOWER}}.md`, run `../../scripts/wall-clock` (or `TZ=America/New_York date '+%Y-%m-%d %H:%M'`), print the prompt. **Refresh the timestamp every response** — never cache it. Stale timestamps cascade into wrong escalation decisions (#1559). Werk version from line 1 of session-start file. The `chorus-prompt/X.Y` slot is literal from this fragment — if you see a different version in your header than the other two roles, you're drifted (#2311).
