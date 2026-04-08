@gate @memory
Feature: Context synthesis gate (memory gate)
  The memory gate blocks code edits until the builder has searched for
  prior work AND demonstrated understanding of what they found. Searching
  without synthesizing is the same as not searching.

  Background:
    Given the hook binary is available

  # --- Fix card scenarios ---
  # Note: memory_gate checks git history on the target file BEFORE search/synthesis.
  # To test search/synthesis behavior, scenarios must first satisfy the git history check.

  Scenario: Fix card edit without log evidence — blocked by log-first gate
    Given a role is building a fix card
    And they have searched "chorus-query.sh search seeds"
    And they have produced a synthesis with "Prior work: checked history"
    But they have not run git log on the target file
    When they try to edit a cross-domain code file
    Then the gate blocks with "Log-first gate"

  Scenario: Fix card edit with git history but no synthesis — blocked
    Given a role is building a fix card
    And they have git history for the target file
    And they have not produced a context synthesis
    When they try to edit a cross-domain code file
    Then the gate blocks with "searched but didn't synthesize"

  Scenario: Fix card edit with git history and search but no synthesis — blocked
    Given a role is building a fix card
    And they have git history for the target file
    And they have searched "chorus-query.sh search seeds"
    And they have not produced a context synthesis
    When they try to edit a cross-domain code file
    Then the gate blocks with "searched but didn't synthesize"

  Scenario: Fix card edit with all evidence — allowed
    Given a role is building a fix card
    And they have edited a test file
    And they have git history for the target file
    And they have read "chorus.log"
    And they have searched "chorus-query.sh search seeds"
    And they have produced a synthesis with "Prior work: Kade shipped #1794. Log evidence: seed.received events present but no routing within 30s"
    When they try to edit a cross-domain code file
    Then the gate allows the edit

  # --- Own domain scenarios ---

  Scenario: Enhance card edit in own domain without synthesis — allowed
    Given a role is building an enhance card
    And they have edited a test file
    And they have not searched Chorus or memory
    When they try to edit a file in their own domain
    Then the gate allows the edit

  Scenario: Fix card edit cross-domain without context — blocked by context synthesis gate
    Given a role is building a fix card
    And they have not searched Chorus or memory
    And they have not produced a context synthesis
    When they try to edit a cross-domain code file
    Then the gate blocks with "Context synthesis gate"

  # --- Non-fix card scenarios ---

  Scenario: New card edit cross-domain without synthesis — blocked
    Given a role is building a new card
    And they have not searched Chorus or memory
    And they have not produced a context synthesis
    When they try to edit a cross-domain code file
    Then the gate blocks with "no search AND no synthesis"

  Scenario: Chore card edit without synthesis — allowed
    Given a role is building a chore card
    And they have not searched Chorus or memory
    When they try to edit a file in their own domain
    Then the gate allows the edit

  Scenario: SWAT card edit in own domain without synthesis — allowed
    Given a role is building a swat card
    And they have not searched Chorus or memory
    When they try to edit a file in their own domain
    Then the gate allows the edit
