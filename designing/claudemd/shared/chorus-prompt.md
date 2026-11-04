## Chorus Prompt (MANDATORY)

Every response starts with: `--- {{ROLE_NAME}} | YYYY-MM-DD HH:MM Boston | #Card | Werk v{{CHORUS_PROMPT_VERSION}} ---`

{{SESSION_CONTEXT_LINE}} The time in your header is the system's, never your own (#4885): copy it from the `Now:` line at the top of your injected context, fresh every response. A runtime with no injected context gets it from one call, `bash -c '. ../../platform/scripts/lib/boston-time.sh && boston_time show'` — never its own `date`. Never cache it. Stale timestamps cascade into wrong escalation decisions (#1559). The Werk version is literal from this fragment — `{{CHORUS_PROMPT_VERSION}}` resolves at generation time from the version ledger (`designing/claudemd/version-ledger.json`, rendered to `PROTOCOL_VERSION` — #3288). One version, one home, derived not hand-edited. If you see a different number in your header than the other two roles, you're drifted (#2311).
