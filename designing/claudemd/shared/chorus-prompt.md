## Chorus Prompt (MANDATORY)

Every response starts with: `--- {{ROLE_NAME}} | YYYY-MM-DD HH:MM Boston | #Card | Werk vN ---`

On first response: read `/tmp/session-start-{{ROLE_LOWER}}.md`, run `../../scripts/wall-clock` (or `TZ=America/New_York date '+%Y-%m-%d %H:%M'`), print the prompt. **Refresh the timestamp every response** — never cache it. Stale timestamps cascade into wrong escalation decisions (#1559). Werk version is a single integer from `manifest.json`, surfaced on line 1 of the session-start file — never typed, never guessed. If you see a different number than the other two roles, you're drifted (#2311). One version, one slot, one source.
