# Silas — Next Session

## What happened
6 cards shipped, all ops/gates hardening. Theme: less noise, less friction, fewer ghost alerts.

## Shipped (6)
#1915, #1916, #1885, #1977, #2053, #2056

- #1915 — TDD gate skips for non-building roles (acceptance exempt)
- #1916 — Demo gate --proven bypass for retroactive card closure
- #1885 — Per-domain crawler error tracking + alert at 3 consecutive failures
- #1977 — Pre-commit WIP gate bypasses acp commits (no more role-state workaround)
- #2053 — Watchdog disabled (zero signal, all noise)
- #2056 — Startup alert rewrite (Fuseki-first check, dynamic error from Loki)

## RCA: startup-sync alert
False FAILED alert triggered 30min RCA. Data was fine (121 seeds, 13GB TDB2). Alert had hardcoded message from fixed April 3 bug. Real issue was Twilio 401 (credential expiry). Fixed: alert now checks Fuseki health first, pulls actual error from logs.

## WIP / Parked
- None in WIP
- #2055 Won't Do — namespace issue was misdiagnosis from the false alert

## Open follow-ons
- Pre-commit hook sync: canonical copy in platform/scripts/ with symlink setup (Kade's feedback on #1977)
- /acp SKILL.md: remove role-state workaround now that #1977 shipped (Wren's feedback)
- Twilio credential refresh: #1499 tracks the 401 auth failure
- Namespace convergence: #1772 tracks urn:gathering → urn:jb migration (not urgent, data is fine)

## Feedback learned
- Alerts with zero signal should be disabled, not tuned — ask "what value does this provide?" before fixing thresholds
- RCA before restarting services — the data investigation on Fuseki revealed the namespace structure even though the alert was wrong
- Jeff is tired by 6pm — chat with Wren to offload analysis when Jeff needs a break

## Stale briefs
- namespace-move-silas.md (Wren, 6+ days)
- git-queue-dirty-tree.md (Kade, 4+ days)
- reindex-gap.md (Wren, 2+ days)
