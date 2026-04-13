# Wren — Next Session

## What happened
Jeff's "drain briefs" session turned into a deep investigation of skill reliability. The demo skill hadn't been loading — preflight hook was blocking every /demo invocation due to missing PATH in Rust Command spawns. Same bug in 9 more hooks. TDD gate was blocking /acp (wrong session scope). All fixed, tested, deployed. Added spine logging to gate scripts so we can now see mechanically whether gates fire. Retagged 15 infrastructure cards to chorus domain. Drained 65 stale demo briefs. RCA'd app-down alerts (execSync blocking event loop 797s). 5 cards shipped, 6 carded.

## Key lessons this session
- When Jeff reports something broken, investigate the observation — don't theorize why it can't be true. He said "/demo doesn't load" and I spent 20 minutes on wrong theories before listening.
- Skills are product (Wren's domain), not infrastructure (Silas's). I own coordination.
- The coverage script had a 1-hour window — useless at 5am. Verify the tool before trusting its output.
- AC checkbox pattern: every builder ships work but doesn't check boxes. Gate:product fails on first run every time. Automation gap.
- execSync on the main thread caused 797s of event loop lag. The health check was causing the problem it detects.
- Always invoke skills through the Skill tool — never manually replicate steps.

## WIP
- None — clean slate

## Next
- Skill execution reliability is still open. Model sometimes follows skill steps, sometimes doesn't. Documented Claude Code bugs #13919 and #182117. Frontmatter hook on /demo is new — watch /tmp/demo-trace.jsonl.
- #1999 execSync audit — Kade, P1
- #2000 gate:code execSync lint — Kade, P2
- #2001 LaunchAgent plist PATH audit — Silas, P1
- #1869 (Tests sub-domain) — gate:product passed, needs Jeff /acp
- #1868 (Code sub-domain) — gate:product passed, needs Jeff /acp

## Pending
- Vikunja token — 401s in Loki
- Bridge subscribers — socket.io-client missing (#1964, Silas finishing)
- Alert cooldown verified (#1966 shipped)

## For next session
- Check /tmp/demo-trace.jsonl for frontmatter hook data
- Close-out should include qualitative reflection — Jeff noticed this gap. Not just what happened, but what it meant.
- The preflight test (demo_preflight_env.rs) hardcodes a WIP card ID that goes stale. Needs a better approach.
