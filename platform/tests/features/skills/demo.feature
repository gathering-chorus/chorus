@demo-skill
Feature: /demo skill — proving gate behaviors
  /demo is the proving gate (DEC-048). It runs dozens of times a week and owns
  the contract between "builder thinks they're done" and "Jeff sees it work."
  These scenarios exercise the actual skill execution path — the same CLI and
  MCP surfaces /demo invokes — not a reimplementation. If /demo's contract
  drifts, a scenario fails. Filed as #2875 after recurring regressions caught
  only by Jeff in live sessions.

  Background:
    Given the chorus board is reachable
    And the chorus.log spine is writable

  # --- Scenario 1: AC pre-flight blocks unchecked AC ---

  Scenario: AC pre-flight blocks when card has unchecked AC items
    Given a fixture card exists with 3 AC items, 1 checked
    When the demo AC pre-flight gate runs against the fixture card
    Then the gate blocks with "BLOCKED: #" and "unchecked AC items"
    And the unchecked AC items are listed in the block message
    And no demo.preflight.completed event with result=pass fires for the card

  # --- Scenario 2: Provenance brief written to correct directory ---

  Scenario: Multi-card demo writes consolidated brief to roles/wren/briefs/
    Given two fixture cards exist with all AC checked
    When a multi-card demo brief is generated for both cards
    Then a brief file appears under roles/wren/briefs/ with both card IDs in the name
    And the brief contains an AC Status section for each card

  # --- Scenario 3: Spine events fire with card_id and trace_id ---

  Scenario: demo.preflight.completed fires with card_id propagated
    Given a fixture card exists with all AC checked
    When demo.preflight.completed is emitted via chorus-log for the card
    Then the spine event lands in chorus.log within 5 seconds
    And the event JSON contains the card_id field matching the fixture

  Scenario: card.demo.started fires with card_id propagated
    Given a fixture card exists with all AC checked
    When card.demo.started is emitted via chorus-log for the card
    Then the spine event lands in chorus.log within 5 seconds
    And the event JSON contains the card_id field matching the fixture

  # --- Scenario 4: Team-nudge step fires nudges to all three roles ---

  Scenario: Demo signal nudges all three roles (minus the caller)
    Given a fixture card exists with all AC checked owned by silas
    When the demo signal step fires nudges from silas
    Then a [demo] nudge is recorded for wren
    And a [demo] nudge is recorded for kade
    And no [demo] nudge is recorded for silas as recipient

  # --- Scenario 5: Builder cannot self-accept ---

  # @wip until #2878 lands the substrate gate. Today the rule lives only in
  # the /demo skill markdown; cards CLI doesn't enforce builder identity.
  @wip @gap-2878
  Scenario: Builder attempting to mark own card Done is refused
    Given a fixture card exists with all AC checked owned by kade
    When kade attempts to accept the card via cards done
    Then the acceptance is refused with a separation-of-duties reason
    And the card remains in WIP or Now status

  # --- Scenario 6: Smoke-check failure blocks demo.completed ---

  Scenario: Smoke-check non-zero blocks demo.signal.completed emission
    Given a fixture card exists with all AC checked
    And smoke-check.sh exits non-zero for the fixture card
    When the demo signal step is attempted for the card
    Then no demo.signal.completed event with result=pass fires for the card
    And the smoke-check failure is reported on the card or in stdout
