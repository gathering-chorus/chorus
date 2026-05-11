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

  # --- Scenario 7: Gate-chain skip on type:chore is a typed refusal, not silent ---

  # @wip until #2893 lands the chain-orchestration substrate hook. Today
  # type:chore / type:swat skips all five gates silently — mistagging a
  # feature card as chore bypasses the entire chain. The substrate must
  # refuse with a typed reason that names the skip, so it shows up in
  # spine and review.
  @wip @gap-2893
  Scenario: Mistagged type:chore card is refused when scope warrants gates
    Given a fixture card exists with type:chore AND files-changed > N lines
    When /demo runs against the fixture card
    Then the gate-chain hook refuses with a typed reason naming gate-chain-skip-not-authorized
    And a demo.gate-chain.refused spine event fires with reason=mistagged-chore
    And no gate:product-pass / gate:code-pass / gate:quality-pass / gate:arch-pass / gate:ops-pass comment is bypassed silently

  # --- Scenario 8: AC-derived happy-path check is enforced ---

  # @wip until #2893 lands gates/happy-path.sh. Today the model improvises
  # curl/browser checks per AC; a card whose AC references an endpoint
  # that 404s can still demo successfully because no substrate runs the
  # derived check.
  @wip @gap-2893
  Scenario: AC referencing an unreachable endpoint blocks demo signal
    Given a fixture card exists with all AC checked
    And one AC item references an endpoint that returns 404
    When /demo runs against the fixture card
    Then the happy-path hook refuses with the failing AC item named
    And no demo.signal.completed event with result=pass fires for the card

  # --- Scenario 9: Stakes-brief lint refuses mechanics-first openings ---

  # @wip until #2893 lands gates/stakes-brief-lint.sh. Today the editorial
  # gate ("leading with mechanics fails") is self-policed prose; a brief
  # opening "I built a function that..." passes despite skill anti-pattern
  # rules.
  @wip @gap-2893
  Scenario: Stakes brief without "Why this matters" is refused
    Given a fixture card exists with all AC checked
    When /demo emits a stakes brief whose body lacks "Why this matters"
    Then the stakes-brief-lint hook refuses with reason=missing-why-this-matters
    And no demo.stakes.completed event with result=pass fires for the card

  @wip @gap-2893
  Scenario: Stakes brief opening with mechanics-first anti-pattern is refused
    Given a fixture card exists with all AC checked
    When /demo emits a stakes brief starting with "I built a function that"
    Then the stakes-brief-lint hook refuses with reason=mechanics-first-opening
    And the offending phrase is reported in the refusal message
    And no demo.stakes.completed event with result=pass fires for the card
