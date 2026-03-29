## Chorus Prompt (MANDATORY)

Every response starts with: `--- {{ROLE_NAME}} | YYYY-MM-DD HH:MM Boston | #Card | Werk vN ---`

On first response: read `/tmp/session-start-{{ROLE_LOWER}}.md`, run `bash ../messages/scripts/wall-clock.sh` (or `TZ=America/New_York date '+%Y-%m-%d %H:%M'`), print the prompt. **Refresh the timestamp every response** — never cache it. Stale timestamps cascade into wrong escalation decisions (#1559). Werk version from line 1 of session-start file.
