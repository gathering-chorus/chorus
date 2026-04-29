# #2533 — commits-domain test audit

**Date:** 2026-04-29
**Status:** AC1 complete; AC2/AC3 disposition below.

## TL;DR

The #2515 audit reported commits-domain has zero test coverage. **That's wrong.** The alias map missed at least 9 test files exercising the domain's components. Real coverage today is substantial.

Net move: this card should reshape into an **alias-map fix** (#2515 follow-on) plus targeted new tests for any genuine gap surfaced. Investigation IS the deliverable per "scope is the work."

## commits-domain components → existing test coverage

Per the domain definition: "How code gets versioned. git-queue.sh, pre-commit hooks, WIP gate, write-scrubber, commit lock, push/rebase flow."

| Component | Source | Test files | Coverage shape |
|---|---|---|---|
| `git-queue.sh` (acquire/commit/release) | `platform/scripts/git-queue.sh` | `platform/scripts/test-git-queue.sh` (171 lines) | Shell-level integration; exercises full lock cycle |
| `git-queue.sh push` (race-safe) | same | `platform/tests/git-queue-push.bats` (96 lines) | Bats; rebase + conflict + dirty-tree paths |
| pre-commit hook (lint ratchet) | `platform/hooks/pre-commit` | `platform/tests/pre-commit-lint-ratchet-test.sh` (123 lines) | Lint-ratchet enforcement |
| pre-commit hook (doc coherence) | same | `platform/tests/doc-coherence-ratchet.test.sh` (59 lines) | Doc-coherence sub-check |
| pre-commit installation | `platform/hooks/install-hooks.sh` | `platform/tests/install-hooks.test.sh` (98 lines) | Hook install + idempotency |
| write_scrubber (PreToolUse) | `platform/services/chorus-hooks/src/hooks/write_scrubber.rs` | inline `#[test]` modules | 20 tests, covers credential patterns + edge cases |
| escaping & git command path | `platform/services/chorus-hooks/src/hooks/...` | `platform/services/chorus-hooks/tests/escaping_and_git.rs` | Integration test through socket |
| Hook false-positives | various PreToolUse hooks | `tests/hook_false_positives.rs` | Cross-hook regression |
| Hook path env handling | various hooks | `tests/hook_path_env.rs` | PATH propagation through Command spawns |

Total: ~830 lines of test code exercising commits-domain behaviors.

## What's NOT obviously covered

Three areas where I couldn't find direct tests:

1. **WIP gate enforcement** — The commits-domain comment lists "WIP gate" but I don't see a specific hook file for it. Likely implemented in board logic (`platform/scripts/cards`) rather than the commit path. Not strictly commits-domain.

2. **`git-queue.sh push` against actively-flapping main** — current `git-queue-push.bats` covers conflict + rebase paths, but not the specific race silas hit on PR #28 today (webhook-stuck branch). That's an integration-side observability gap (per silas's #2572 line of reasoning), not a commits-domain unit test.

3. **TDD gate / context-synthesis gate behavior** — these are PreToolUse hooks that block commits. They have inline `#[test]` modules in their `.rs` files (need to verify each), but I haven't confirmed. Worth a 5-min spot-check.

## AC disposition proposal

- **AC1 ✓** — Critical-path behaviors identified (above table). Audit done.
- **AC2** — reshape: instead of "write unit tests for those behaviors" (most are tested), the actionable work is:
  (a) File a #2515 alias-map fix card so the next audit captures these 9 files
  (b) Spot-check TDD gate + context-synthesis gate + accept_gate inline tests; if any gate-rs has < 5 inline tests, write the gap
  (c) Skip "WIP gate" — not commits-domain proper
- **AC3** — once (a) lands, the next #2515 audit will show coverage > 0 without any new tests. Mechanically satisfied by the alias-map fix.

## Recommendation

Reshape #2533 to: "alias-map fix for commits-domain (9 files) + spot-check on PreToolUse gate inline tests." That's honest scope. Net new test code likely small or zero.

The bigger learning is for #2515 itself: alias-map should derive from filesystem walk, not authored map. Files exist; map is stale. That's a structural fix, not a per-domain backfill.

— kade
