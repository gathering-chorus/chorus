@gate @log_first @building
Feature: Log-first gate
  The log-first gate blocks code edits on fix cards until the builder
  has checked the logs AND stated what they found. Opening a log file
  is not enough — the role must produce a "Log evidence:" statement
  connecting findings to the problem. No performative log checks.

  Background:
    Given the hook binary is available

  # --- Fix card: both read AND synthesis required ---

  Scenario: Fix card edit without checking logs — blocked
    Given a role is building a fix card
    And they have context synthesis but no log evidence
    When they try to edit a code file in their own domain
    Then the gate blocks with "haven't checked the logs"

  Scenario: Fix card opened a log but didn't say what they found — blocked
    Given a role is building a fix card
    And they have read "chorus.log"
    And they have not stated what the logs revealed
    And they have full context synthesis for a fix
    When they try to edit a code file in their own domain
    Then the gate blocks with "haven't checked the logs"

  Scenario: Fix card stated findings but never read a log — blocked
    Given a role is building a fix card
    And they have stated log findings without reading logs
    And they have full context synthesis for a fix
    When they try to edit a code file in their own domain
    Then the gate blocks with "haven't checked the logs"

  Scenario: Fix card read logs AND stated what they found — allowed
    Given a role is building a fix card
    And they have read "chorus.log"
    And they have stated "Log evidence: seed.received fired but no seed.routed within 30s"
    And they have full context synthesis for a fix
    When they try to edit a code file in their own domain
    Then the gate allows the edit

  # --- Non-fix cards don't need log evidence ---

  Scenario: New card edit in own domain without logs — allowed
    Given a role is building a new card
    And they have not read any log files
    When they try to edit a file in their own domain
    Then the gate allows the edit

  Scenario: Enhance card edit in own domain without logs — allowed
    Given a role is building an enhance card
    And they have not read any log files
    When they try to edit a file in their own domain
    Then the gate allows the edit

  Scenario: Chore card edit in own domain without logs — allowed
    Given a role is building a chore card
    And they have not read any log files
    When they try to edit a file in their own domain
    Then the gate allows the edit

  Scenario: SWAT card edit in own domain without logs — allowed
    Given a role is building a swat card
    And they have not read any log files
    When they try to edit a file in their own domain
    Then the gate allows the edit

  # --- Edge cases ---

  Scenario: Fix card editing non-code file without logs — allowed
    Given a role is building a fix card
    And they have not read any log files
    When they try to edit a markdown file
    Then the gate allows the edit

  Scenario: Fix card reading a file without logs — allowed
    Given a role is building a fix card
    And they have not read any log files
    When they try to read a code file
    Then the gate allows the edit
