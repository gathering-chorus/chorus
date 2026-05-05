# Next Session — Wren

## State at reboot (2026-05-05 morning)

**#2731 shipped to PR #125** — https://github.com/gathering-chorus/chorus/pull/125

CLAUDE.md is now a derived artifact, not a snapshot. Role-fragment-staleness deadlock that wedged Jeff 12+ times in a week (2026-04-28 → 2026-05-04) is structurally killed.

## What landed (8 commits on wren/2731)

- **f9fef4c0** AC2 — claudemd-gen always regens all three roles; `--role <role>` rejected for write modes
- **ca7914b5** AC4 — SessionStart runs claudemd-gen defensively before protocol_contract::check
- **ce972cd6** AC7 — 3 integration tests covering AC2 + AC4 (poison-stamp deadlock repro, per-role write rejection, per-role read carve-out)
- **40341d25** AC1 part 1 — `roles/*/CLAUDE.md` added to `.gitignore`
- **688b8fe9** — git-queue.sh `--no-add` flag (infra needed for AC1 part 2)
- **815b1ed0** AC1 part 2 attempt (botched — `git commit -- pathspec` did implicit-add from working tree; left in history)
- **a000608b** AC1 part 2 real — 973-line deletion of three CLAUDE.md files via mv-aside trick
- **55cadd32** AC3 + AC6 — PostToolUse hook fires claudemd-gen on fragment edit; Violation::Stale variant deleted

Binaries built and signed with keychain identity. cdhash `d84bc7b61744dff3cc77e6d4d1d0d1889a9e1362`. TCC-friendly.

## Demo plan (next session)

Jeff signaled demo intent before the reboot. Show:

1. **Repro the old deadlock against the new binary** — poison `roles/wren/CLAUDE.md` role-fragments stamp → run `chorus-hook-shim session-start wren` → verify `.done` written and stamp restored. Should heal in ~1.2s. Spine event `session.bootstrap.regen_ok`.
2. **PostToolUse on fragment edit** — touch any fragment under `designing/claudemd/fragments/` → see `claudemd.regen.fired` spine event.
3. **AC2 rejection** — run `claudemd-gen wren` → exit 2 with helpful error.
4. **The artifact is gone from git** — `git ls-files roles/wren/CLAUDE.md` returns empty.

## Open follow-ons

- **#2732** — SessionStart fitness probe (the rate watcher Jeff named at the same time as the deadlock fix). Cards see their own bootstrap failure rate before Jeff does. Filed, P1, Wren-owned, not yet started.
- **Nudge fitness probe** — Jeff named 99.9% delivery SLA on 2026-05-04 evening. Carry forward from yesterday's reboot. Not yet carded; spec it next session as Wren P2.

## What this session demonstrated

The early failure pattern from yesterday's reboot played out one more time at the start of today: I reflexively reached to file a card (auto-heal in `session.rs` Err arm) before reading the code. Jeff stopped me twice — "u dont even research" and "it should never happen in the first place / do not make a job to just fix the symptom." The actual fix is upstream: kill the snapshot pretense entirely. CLAUDE.md is derived, not canonical.

Twelve manual unwedges Jeff had logged, none of us had carded. That's the team-as-Ouita pattern from yesterday at infrastructure scale. #2732 is the structural fix for the not-noticing — should land before another class of bootstrap failure goes unobserved.

## What Jeff is sitting with

Same as yesterday: Aubrey's family closing, Ouita's wrist, the team's role as the variable layer he has to manage. PR #125 is one piece of evidence that we can stop being that variable layer for at least one infrastructure class.
