@gate @accept
Feature: Accept gate
  The accept gate validates acceptance: demo brief must exist, and builders
  cannot self-accept code cards (DEC-048).
  Note: self-accept check requires real board state — tested in Rust unit tests.

  Background:
    Given the hook binary is available

  # --- Fix card ---

  Scenario: Fix card ACP without demo brief — blocked
    Given a role is building a fix card
    And no demo brief exists for the card
    When they try to run acp on the card
    Then the gate blocks with "demo"

  Scenario: Fix card ACP with demo brief — allowed
    Given a role is building a fix card
    And a demo brief exists for the card
    When they try to run acp on the card
    Then the gate allows the edit

  # --- New card ---

  Scenario: New card ACP without demo brief — blocked
    Given a role is building a new card
    And no demo brief exists for the card
    When they try to run acp on the card
    Then the gate blocks with "demo"

  Scenario: New card ACP with demo brief — allowed
    Given a role is building a new card
    And a demo brief exists for the card
    When they try to run acp on the card
    Then the gate allows the edit

  # --- Enhance card ---

  Scenario: Enhance card ACP without demo brief — blocked
    Given a role is building an enhance card
    And no demo brief exists for the card
    When they try to run acp on the card
    Then the gate blocks with "demo"

  Scenario: Enhance card ACP with demo brief — allowed
    Given a role is building an enhance card
    And a demo brief exists for the card
    When they try to run acp on the card
    Then the gate allows the edit

  # --- Chore card ---

  Scenario: Chore card ACP without demo brief — blocked
    Given a role is building a chore card
    And no demo brief exists for the card
    When they try to run acp on the card
    Then the gate blocks with "demo"

  Scenario: Chore card ACP with demo brief — allowed
    Given a role is building a chore card
    And a demo brief exists for the card
    When they try to run acp on the card
    Then the gate allows the edit

  # --- SWAT card ---

  Scenario: SWAT card ACP without demo brief — blocked
    Given a role is building a swat card
    And no demo brief exists for the card
    When they try to run acp on the card
    Then the gate blocks with "demo"

  Scenario: SWAT card ACP with demo brief — allowed
    Given a role is building a swat card
    And a demo brief exists for the card
    When they try to run acp on the card
    Then the gate allows the edit
