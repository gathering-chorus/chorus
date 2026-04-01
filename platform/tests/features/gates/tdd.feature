@gate @tdd
Feature: TDD gate
  The TDD gate enforces test-first development. Two enforcement points:
  1. Production code edits blocked until a test file is written first
  2. Demo/done/acp blocked until tests have been run
  fix/enhance/new cards MUST follow TDD. chore/swat cards skip.

  Background:
    Given the hook binary is available

  # --- Gate 1: Test file before production code ---

  Scenario: Fix card edits production code without writing tests first — blocked
    Given a role is building a fix card
    And they have full context synthesis for a fix
    And they have not edited any test files
    When they try to edit a code file in their own domain
    Then the gate blocks with "haven't written a test yet"

  Scenario: Fix card edits production code after writing a test — allowed
    Given a role is building a fix card
    And they have full context synthesis for a fix
    And they have edited a test file
    When they try to edit a code file in their own domain
    Then the gate allows the edit

  Scenario: New card edits production code without writing tests first — blocked
    Given a role is building a new card
    And they have not edited any test files
    When they try to edit a code file in their own domain
    Then the gate blocks with "haven't written a test yet"

  Scenario: Enhance card edits production code without writing tests first — blocked
    Given a role is building an enhance card
    And they have not edited any test files
    When they try to edit a code file in their own domain
    Then the gate blocks with "haven't written a test yet"

  Scenario: Enhance card edits production code after writing a test — allowed
    Given a role is building an enhance card
    And they have edited a test file
    When they try to edit a code file in their own domain
    Then the gate allows the edit

  # --- Gate 1: Test file edits always allowed ---

  Scenario: Fix card edits test file without prior tests — allowed
    Given a role is building a fix card
    And they have full context synthesis for a fix
    And they have not edited any test files
    When they try to edit a test file
    Then the gate allows the edit

  # --- Gate 2: Tests must run before demo/done ---

  Scenario: Fix card demo without running tests — blocked
    Given a role is building a fix card
    And they have not run any tests in the session
    When they try to run demo on the card
    Then the gate blocks with "test"

  Scenario: Fix card done after running tests with demo — allowed
    Given a role is building a fix card
    And a demo brief exists for the card
    And they have run "cargo test" in the session
    When they try to mark the card done
    Then the gate allows the edit

  Scenario: New card done without tests — blocked
    Given a role is building a new card
    And they have not run any tests in the session
    When they try to mark the card done
    Then the gate blocks with "test"

  # --- Chore/SWAT exemptions ---

  Scenario: Chore card edits code without tests — allowed
    Given a role is building a chore card
    And they have not edited any test files
    When they try to edit a code file in their own domain
    Then the gate allows the edit

  Scenario: SWAT card edits code without tests — allowed
    Given a role is building a swat card
    And they have not edited any test files
    When they try to edit a code file in their own domain
    Then the gate allows the edit

  Scenario: Chore card done without tests — allowed
    Given a role is building a chore card
    And they have not run any tests in the session
    When they try to mark the card done
    Then the gate allows the edit

  Scenario: SWAT card done without tests — allowed
    Given a role is building a swat card
    And they have not run any tests in the session
    When they try to mark the card done
    Then the gate allows the edit
