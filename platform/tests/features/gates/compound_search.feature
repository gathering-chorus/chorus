@gate @compound-search
Feature: Four-layer compound search (#2004)
  Every search gets context from all four layers before the code search runs:
  Chorus (team memory), logs (system state), git (change history), code (current state).
  All layers inject via deny message so roles see the context.

  Background:
    Given the hook binary is available

  Scenario: Search with Chorus results gets denied with context
    Given a role is building a new card
    When they grep for a term with Chorus results "seed pipeline"
    Then the gate blocks with "Compound context"
    And the deny message contains "Chorus found"

  Scenario: Retry after deny goes through clean
    Given a role is building a new card
    When they grep for a term with Chorus results "seed pipeline"
    Then the gate blocks with "Compound context"
    When they retry the same grep "seed pipeline"
    Then the gate allows the search

  Scenario: Search with no Chorus results is allowed
    Given a role is building a new card
    When they grep for a term with no Chorus results "bdd-unmatchable-z9x8w7"
    Then the gate allows the search

  Scenario: Loki layer returns results from multi-day lookback
    Given a role is building a new card
    When they grep for a term with Chorus results "SPARQL"
    Then the gate blocks with "Compound context"
    And the deny message contains "Loki logs"
