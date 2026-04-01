@gate @demo
Feature: Demo gate
  The demo gate blocks cards from being marked Done without demo evidence.
  DEC-048: deploy, demo to Jeff, then accept. No self-service Done.

  Background:
    Given the hook binary is available

  # --- Requires demo evidence ---

  Scenario: Fix card done without demo brief — blocked
    Given a role is building a fix card
    And no demo brief exists for the card
    When they try to mark the card done
    Then the gate blocks with "demo gate"

  Scenario: New card done without demo brief — blocked
    Given a role is building a new card
    And no demo brief exists for the card
    When they try to mark the card done
    Then the gate blocks with "demo gate"

  Scenario: Enhance card done without demo brief — blocked
    Given a role is building an enhance card
    And no demo brief exists for the card
    When they try to mark the card done
    Then the gate blocks with "demo gate"

  # --- Demo evidence present ---

  Scenario: Fix card done with demo brief — allowed
    Given a role is building a fix card
    And a demo brief exists for the card
    When they try to mark the card done
    Then the gate allows the edit

  Scenario: New card done with demo brief — allowed
    Given a role is building a new card
    And a demo brief exists for the card
    When they try to mark the card done
    Then the gate allows the edit

  # --- Exempt card types ---

  Scenario: Chore card done without demo — allowed
    Given a role is building a chore card
    And no demo brief exists for the card
    When they try to mark the card done
    Then the gate allows the edit

  Scenario: SWAT card done without demo — allowed
    Given a role is building a swat card
    And no demo brief exists for the card
    When they try to mark the card done
    Then the gate allows the edit

  # --- ACP skill path ---

  Scenario: ACP without demo brief — blocked
    Given a role is building a fix card
    And no demo brief exists for the card
    When they try to run acp on the card
    Then the gate blocks with "demo"
