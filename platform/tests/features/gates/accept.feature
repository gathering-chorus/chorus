@gate @accept
Feature: Accept gate
  The accept gate validates acceptance: demo brief must exist, and builders
  cannot self-accept code cards (DEC-048).
  Note: self-accept check requires real board state — tested in Rust unit tests.
  Note: accept_gate requires demo brief for ALL card types (unlike demo_gate
  which exempts chore/swat). This may be a gap worth a fix card.

  Background:
    Given the hook binary is available

  # --- Demo brief required for acp ---

  Scenario: ACP without demo brief — blocked
    Given a role is building a fix card
    And no demo brief exists for the card
    When they try to run acp on the card
    Then the gate blocks with "demo"

  Scenario: ACP with demo brief — allowed
    Given a role is building a new card
    And a demo brief exists for the card
    When they try to run acp on the card
    Then the gate allows the edit

  # --- Chore/SWAT still need demo brief for acp (accept_gate gap) ---

  Scenario: ACP on chore card without demo — blocked by accept gate
    Given a role is building a chore card
    And no demo brief exists for the card
    When they try to run acp on the card
    Then the gate blocks with "demo"

  Scenario: ACP on chore card with demo — allowed
    Given a role is building a chore card
    And a demo brief exists for the card
    When they try to run acp on the card
    Then the gate allows the edit
