@acp-skill
Feature: /acp skill — accept-commit-push behaviors
  /acp is the gate that decides what reaches main. It owns the commit + push +
  PR-merge + cards-done + branch-close transaction, the demo-evidence check
  (DEC-048), and the typed refusal taxonomy. These scenarios exercise the
  actual /acp execution path — chorus_acp service code, git-queue.sh, gh CLI
  (stubbed to a hermetic bare-repo fixture), cards CLI, chorus-log — no
  reimplementation, no mocks at the test-code layer. Filed as #2879 after
  #2877 + #2881 shipped with coverage gaps that bit production tonight.

  Background:
    Given the chorus.log spine is writable
    And a bare-repo origin fixture is available
    And the gh stub is on PATH

  # ============================================================
  # Refusal taxonomy — one scenario per typed refusal step.
  # Maps to chorus_acp.refused emissions in server.ts L1170+.
  # ============================================================

  # @wip @gap-2884 — card-mismatch is a pre-substrate intent-check inside the
  # MCP wrapper (server.ts L1170-1178). Substrate-only invocation (path D)
  # can't reach it. #2884 covers the direct-invocation harness for when the
  # classifier actually drifts.
  @wip @gap-2884
  Scenario: card-mismatch refusal when explicit card_id differs from HEAD branch
    Given a fixture card 99201 is in WIP owned by wren
    And the HEAD branch is "wren/99201"
    When chorus_acp is called with card_id 99202
    Then the call refuses with reason "card-mismatch"
    And the refusal names both the branch-derived id 99201 and the asserted id 99202
    And no commit was created

  Scenario: hook-fail refusal when pre-commit emits a block signature
    Given a fixture card 99209 is in WIP owned by wren
    And the HEAD branch is "wren/99209"
    And a staged change is present in the werk
    And a pre-commit hook is configured to print "🔴 BLOCKED" and exit non-zero
    When chorus_acp is called with card_id 99209
    Then the call refuses with reason "hook-fail"
    And no push reached the bare-repo origin

  Scenario: commit-fail refusal when pre-commit exits non-zero without block signature
    Given a fixture card 99210 is in WIP owned by wren
    And the HEAD branch is "wren/99210"
    And a staged change is present in the werk
    And a pre-commit hook is configured to exit non-zero with no signature output
    When chorus_acp is called with card_id 99210
    Then the call refuses with reason "commit-fail"

  Scenario: push-conflict refusal when remote diverges with same-file conflicting content
    Given a fixture card 99211 is in WIP owned by wren
    And the HEAD branch is "wren/99211"
    And the bare-repo origin has a commit on the same branch that modifies the same file with different content
    When chorus_acp is called with card_id 99211
    Then the call refuses with reason "push-conflict"
    And the refusal stderr matches "rebase" or "conflict" or "merge"
    And the refusal message names the rebase requirement

  Scenario: push-fail refusal on the #2881 malformed-lease signature
    Given a fixture card 99212 is in WIP owned by wren
    And the HEAD branch is "wren/99212" with no upstream ref yet
    And git-queue.sh is invoked with a force-with-lease that resolves to a malformed object id
    When chorus_acp is called with card_id 99212
    Then the call refuses with reason "push-fail"
    And the refusal stderr contains "cannot parse expected object name"

  Scenario: pr-create-fail refusal when gh pr create returns non-zero
    Given a fixture card 99213 is in WIP owned by wren
    And the HEAD branch is "wren/99213"
    And the gh stub is configured to fail "pr create" with exit code 1
    When chorus_acp is called with card_id 99213
    Then the call refuses with reason "pr-create-fail"

  Scenario: pr-merge-fail refusal when gh pr merge returns non-zero
    Given a fixture card 99214 is in WIP owned by wren
    And the HEAD branch is "wren/99214"
    And the gh stub is configured to succeed on "pr create" but fail "pr merge"
    When chorus_acp is called with card_id 99214
    Then the call refuses with reason "pr-merge-fail"

  Scenario: cards-done-fail refusal when cards CLI exits non-zero on done
    Given a fixture card 99215 is in WIP owned by wren
    And the HEAD branch is "wren/99215"
    And the gh stub succeeds on create + merge
    And the cards CLI is configured to fail "done" for card 99215
    When chorus_acp is called with card_id 99215
    Then the call refuses with reason "cards-done-fail"

  # #2882 filed for the doc-vs-code reconcile: server.ts L1414-1427 makes
  # branch-close intentionally non-fatal, but the MCP description names it
  # in the refusal taxonomy. This scenario asserts the actual behavior.
  Scenario: branch-close failure is non-fatal — result reports branch_closed=false
    Given a fixture card 99216 is in WIP owned by wren
    And the HEAD branch is "wren/99216"
    And the gh stub succeeds on create + merge
    And the cards CLI succeeds on done
    And chorus-werk close is configured to fail for the role
    When chorus_acp is called with card_id 99216
    Then the call returns successfully with branch_closed=false
    And a chorus_acp.branch-close.skipped or .failed step event was emitted
    And card.accepted still landed in chorus.log with card_id 99216

  # ============================================================
  # Success path — full transaction lands cleanly.
  # ============================================================

  Scenario: clean acp on a fresh-branch first commit lands all spine events
    Given a fixture card 99220 is in WIP owned by wren
    And the HEAD branch is "wren/99220" with a fresh first commit
    And demo evidence is present for the card
    And the gh stub is configured for the success path
    When chorus_acp is called with card_id 99220
    Then the result contains a non-empty sha
    And the result contains a pr_url matching the gh stub's create response
    And branch_closed is true
    And card.accepted lands in chorus.log with card_id 99220
    And card.branch.closed lands in chorus.log with card_id 99220
    And release.triggered lands in chorus.log with card_id 99220
    # trace_id propagation assertion deferred to #2884 — pure MCP-wrapper
    # concern (#2857), not reachable via substrate-only path D.

  # ============================================================
  # Idempotent re-run — safe to re-call after partial completion.
  # ============================================================

  Scenario: idempotent re-run when PR already merged proceeds to closure
    Given a fixture card 99221 is in WIP owned by wren
    And the HEAD branch is "wren/99221"
    And the gh stub reports the PR as already merged to main
    When chorus_acp is called with card_id 99221
    Then the call emits a chorus_acp.skip-to-closure step event
    And cards-done completes without error
    And card.branch.closed lands in chorus.log with card_id 99221

  # ============================================================
  # Demo-evidence gate (DEC-048) — DEMOed self-acp blocked.
  # Lives in /acp skill markdown Step 0; this scenario captures
  # the contract the skill enforces.
  # ============================================================

  Scenario: Demo-evidence gate blocks acp on a code card without demo:preflight-pass
    Given a fixture card 99222 is in WIP owned by kade
    And the card has no demo:preflight-pass comment
    And the spine has no demo.show.completed for the card
    And the HEAD branch is "kade/99222"
    When the demo-evidence pre-check runs for the card
    Then the pre-check refuses with a reason naming the missing demo evidence
    And the refusal references DEC-048 or "demo evidence" in the message

  # ============================================================
  # Regression guards — today's specific class (#2877 + #2881).
  # ============================================================

  Scenario: Fresh-branch first push succeeds (regression guard for #2881)
    Given a fixture card 99230 is in WIP owned by wren
    And the HEAD branch is "wren/99230" pushed for the first time
    And origin/<branch> ref does not exist yet
    When git-queue.sh push is invoked from the werk
    Then the push completes with exit code 0
    And git ls-remote origin "wren/99230" returns the local SHA

  Scenario: Squash-rewrite push lands clean via --force-with-lease (regression guard for #2877)
    Given a fixture card 99231 is in WIP owned by wren
    And the HEAD branch is "wren/99231" already pushed once
    And the local branch was rebased to a different SHA
    When git-queue.sh push is invoked with "--force-with-lease"
    Then the push completes with exit code 0
    And the resulting upstream SHA matches the local rebased SHA
    And the push command included a "--force-with-lease=<ref>:<sha>" form pinned to the pre-fetch SHA
