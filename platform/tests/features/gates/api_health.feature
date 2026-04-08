@gate @ops @wip
Feature: Chorus API health endpoint
  Jeff or a probe hits /api/chorus/health and gets real answers about
  whether the API is functional — database, vectors, uptime — not just
  whether a process has a PID.

  Scenario: Health endpoint returns JSON with db status and uptime
    When a probe hits GET /api/chorus/health
    Then the response is 200 with Content-Type application/json
    And the body contains "status", "db", "uptime", and "vectors" fields

  Scenario: Health endpoint reports vector count
    When a probe hits GET /api/chorus/health
    Then the "vectors" field is a number greater than zero

  Scenario: Health endpoint reports hook server status
    When a probe hits GET /api/chorus/health
    Then the body contains a "hooks" field with functional status

  Scenario: Probes can use health as liveness check
    When seed-probe or clearing-probe checks API liveness
    Then GET /api/chorus/health returning 200 confirms the API is functional
    And a non-200 response triggers an alert
