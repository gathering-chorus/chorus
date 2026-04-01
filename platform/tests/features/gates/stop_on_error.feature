@gate @stop_on_error
Feature: Stop-on-error gate
  The stop-on-error gate blocks after errors. PostToolUse hook — checks
  tool_response for non-zero exit codes. Exempts benign commands
  (grep, diff, test runners).
  Note: PostToolUse behavior tested via Rust unit tests. BDD tests verify
  the exemption list behavior through the full gate chain.

  Background:
    Given the hook binary is available

  Scenario: Edit after benign grep in session — allowed
    Given a role is building a new card
    And they have edited a test file
    And a pair session is active
    When they try to edit a file in their own domain
    Then the gate allows the edit
