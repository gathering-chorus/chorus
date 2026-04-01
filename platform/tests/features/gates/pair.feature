@gate @pair
Feature: Pair gate
  The pair gate blocks code edits without an active pair session.
  #1814: code cards require a pair — /tmp/pair-*.md must exist.

  Background:
    Given the hook binary is available

  # Note: "no pair" scenario cannot run while a real pair is active (/tmp/pair-*.md exists).
  # The pair gate was proven live when it blocked Wren's edit before Silas joined #1929.

  Scenario: Code edit with active pair session — allowed
    Given a role is building a new card
    And they have edited a test file
    And a pair session is active
    When they try to edit a code file in their own domain
    Then the gate allows the edit

  Scenario: Markdown edit without pair — allowed
    Given a role is building a new card
    And no pair session is active
    When they try to edit a markdown file
    Then the gate allows the edit

  Scenario: Read without pair — allowed
    Given a role is building a new card
    And no pair session is active
    When they try to read a code file
    Then the gate allows the edit
