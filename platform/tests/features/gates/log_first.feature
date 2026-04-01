@gate @log_first
Feature: Log-first gate
  The log-first gate blocks code edits on fix cards until the builder
  has checked the logs. Logs reveal root cause in seconds — reading code
  without them leads to wrong theories.

  Background:
    Given the hook binary is available

  # --- Fix card scenarios ---

  Scenario: Fix card edit without checking logs — blocked
    Given a role is building a fix card
    And they have context synthesis but no log evidence
    When they try to edit a code file in their own domain
    Then the gate blocks with "check the logs"

  Scenario: Fix card edit after checking chorus.log — allowed
    Given a role is building a fix card
    And they have read "chorus.log"
    And they have full context synthesis for a fix
    When they try to edit a code file in their own domain
    Then the gate allows the edit

  Scenario: Fix card edit after checking hooks.log — allowed
    Given a role is building a fix card
    And they have read "hooks.log"
    And they have full context synthesis for a fix
    When they try to edit a code file in their own domain
    Then the gate allows the edit

  Scenario: Fix card edit after using Loki — allowed
    Given a role is building a fix card
    And they have read "localhost:3102"
    And they have full context synthesis for a fix
    When they try to edit a code file in their own domain
    Then the gate allows the edit

  # --- Non-fix card scenarios (own domain, so memory gate doesn't fire) ---

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
