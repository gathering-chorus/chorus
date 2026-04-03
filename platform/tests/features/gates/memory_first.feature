@gate @memory-first
Feature: Memory-first search gate (#1951)
  Roles grep the filesystem for card and session context instead of using
  the card-story and conversation API endpoints. The endpoints exist but
  roles default to old habits. This gate enforces DEC-074: Chorus first.

  Background:
    Given the hook binary is available

  # --- Context searches without memory query — blocked ---

  Scenario: Grep for a card number without querying card-story — blocked
    Given a role is building a new card
    And they have not queried memory endpoints
    When they grep for card context "#1951"
    Then the gate blocks with "Memory-first search gate"

  Scenario: Grep in briefs directory without querying conversation — blocked
    Given a role is building a new card
    And they have not queried memory endpoints
    When they grep in a session context path "briefs/"
    Then the gate blocks with "DEC-074"

  Scenario: Grep for session transcripts without memory query — blocked
    Given a role is building a new card
    And they have not queried memory endpoints
    When they grep in a session context path "messages/"
    Then the gate blocks with "card-story"

  # --- Context searches after memory query — allowed ---

  Scenario: Grep for card context after querying card-story — allowed
    Given a role is building a new card
    And they have queried card-story endpoint
    When they grep for card context "bdd-zxy-nomatch-99997"
    Then the gate allows the search

  Scenario: Grep in briefs after chorus search — allowed
    Given a role is building a new card
    And they have run a chorus search
    When they grep in a session context path "briefs/"
    Then the gate allows the search

  # --- Code searches — always allowed ---

  Scenario: Grep for code pattern without memory query — allowed
    Given a role is building a new card
    And they have not queried memory endpoints
    When they grep for a code pattern "bddZxyNomatch789"
    Then the gate allows the search

  Scenario: Grep in src directory without memory query — allowed
    Given a role is building a new card
    And they have not queried memory endpoints
    When they grep for a code pattern in "/src/services/"
    Then the gate allows the search

  # --- Bash context searches ---

  Scenario: Bash grep on session files without memory query — blocked
    Given a role is building a new card
    And they have not queried memory endpoints
    When they bash grep session context "grep -r 'alert' briefs/"
    Then the gate blocks with "Memory-first"

  Scenario: Bash grep on session files after memory query — allowed
    Given a role is building a new card
    And they have queried card-story endpoint
    When they bash grep session context "grep -r 'alert' briefs/"
    Then the gate allows the search
