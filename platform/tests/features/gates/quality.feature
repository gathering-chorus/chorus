@gate @quality
Feature: Quality gate
  The quality gate emits a spine event when /demo is invoked.
  The actual agent review runs inside the /demo skill (needs 30-50s,
  hook timeout is 5s). Quality gate is a pass-through that signals.
  Note: testing the demo skill path requires a real board card.
  The gate's signal behavior is verified by the Rust unit tests.

  Background:
    Given the hook binary is available

  Scenario: Non-demo skill passes through — allowed
    Given a role is building a new card
    When they try to run acp on the card
    Then the gate blocks with "demo"
