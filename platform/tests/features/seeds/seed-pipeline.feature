@seed @pipeline
Feature: Seed pipeline
  Jeff sends a seed from his phone. It arrives, persists,
  and Jeff sees proof. No silent drops. No briefs.

  Engage and close-the-loop are role behavior — tested by
  observation and policy, not automation.

  # --- ARRIVE ---

  Scenario: Content then hashtag — seed correlates and routes
    Given Jeff sends "Kitchen reno concept" from his phone
    And Jeff sends "#idea" 30 seconds later
    When the seed pipeline processes both messages
    Then the content is routed to ideas in Chorus
    And the hashtag message does not create a capture

  Scenario: Hashtag-only message — not a seed
    Given Jeff sends "#wren" from his phone
    And no content message preceded it
    Then no seed record is created

  Scenario: Content without hashtag — routes to Wren by default
    Given Jeff sends "Random thought about gardens" from his phone
    And no hashtag follows within 120 seconds
    Then the seed routes to wren by default

  # --- ANTI-PATTERNS ---

  Scenario: Hashtag displayed as seed on triage page — wrong
    Given Jeff sends "Cool article" then "#wren"
    When the triage page loads
    Then only the content message appears as a seed
    And "#wren" does not appear as a separate pending seed
