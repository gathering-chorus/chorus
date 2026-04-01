@gate @tdd
Feature: TDD gate
  The TDD gate blocks demo and done without test evidence in the session.
  DEC-1674: no card is done without tests covering every AC item.
  Note: "allowed" demo scenarios require a real board card — tested via
  the full /demo skill flow, not isolated here.

  Background:
    Given the hook binary is available

  Scenario: Demo without running tests — blocked
    Given a role is building a fix card
    And they have not run any tests in the session
    When they try to run demo on the card
    Then the gate blocks with "test"

  Scenario: Done without tests — blocked
    Given a role is building a new card
    And they have not run any tests in the session
    When they try to mark the card done
    Then the gate blocks with "test"

  Scenario: Done after running tests with demo — allowed
    Given a role is building a new card
    And a demo brief exists for the card
    And they have run "cargo test" in the session
    When they try to mark the card done
    Then the gate allows the edit
