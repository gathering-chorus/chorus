@seed @alert
Feature: Seed failure alerting
  When a seed write fails, Jeff sees an alert on the Bridge
  within 60 seconds. The system catches the problem before
  Jeff does. No silent drops.

  # --- ALERT ON FAILURE ---

  Scenario: Seed write fails — Bridge gets alerted within 60 seconds
    Given the seed pipeline is running
    And a seed SPARQL write fails with "SPARQL update failed"
    When the alert runner checks seed-write-failure
    Then the Bridge receives a critical alert
    And the alert says seeds are not landing

  Scenario: App is down — Bridge gets alerted within 2 minutes
    Given the gathering app is not responding on localhost:3000
    When the alert runner checks app-down
    Then the Bridge receives a critical alert
    And the alert includes Fuseki and tunnel status

  Scenario: Pipeline healthy — no false alerts
    Given the seed pipeline is running
    And no errors in the last 90 seconds
    When the alert runner checks seed-write-failure
    Then no alert is sent to the Bridge

  # --- CHECK-SEEDS WRITE PROBE ---

  Scenario: /cs detects write failure even when health returns 200
    Given Fuseki health returns 200
    But Fuseki rejects SPARQL INSERT with 400
    When Jeff runs /cs
    Then pipeline status shows "DOWN" not "healthy"
    And the output says "fuseki: reads OK, writes FAIL"
