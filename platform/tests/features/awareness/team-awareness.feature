@awareness @watchdog @wip
Feature: Team awareness watchdog
  A role stalls. Within 2 minutes the watchdog nudges them.
  Within 3 minutes Wren knows. Within 5 minutes Jeff knows.

  Background:
    Given the watchdog script exists at platform/scripts/watchdog.sh
    And role-state emits role.state.changed to chorus.log

  Scenario: Role goes silent for 2 minutes — watchdog nudges
    Given silas declared state "building" 3 minutes ago
    And silas has no tool calls in the last 2 minutes
    When the watchdog runs
    Then it nudges silas with "watchdog: no activity in 2min, are you blocked?"
    And it emits watchdog.nudge.sent to chorus.log

  Scenario: Role goes silent for 3 minutes — escalate to Wren
    Given kade declared state "building" 4 minutes ago
    And kade was already nudged by the watchdog
    And kade has not responded
    When the watchdog runs
    Then it nudges wren with "watchdog: kade unresponsive 3min on #<card>"
    And it emits watchdog.escalated to chorus.log

  Scenario: Role goes silent for 5 minutes — alert Jeff
    Given kade declared state "building" 6 minutes ago
    And kade was escalated to wren
    And kade has not responded
    When the watchdog runs
    Then it posts an alert to Bridge for Jeff
    And it emits watchdog.alert.jeff to chorus.log

  Scenario: Role resumes after nudge — watchdog resets
    Given silas was nudged by the watchdog
    And silas emits a new tool call
    When the watchdog runs
    Then it does not nudge silas
    And the watchdog timer resets for silas

  Scenario: All three roles silent — system-wide alert
    Given wren declared state "idle" 10 minutes ago
    And silas declared state "waiting" 10 minutes ago
    And kade declared state "waiting" 10 minutes ago
    When the watchdog runs
    Then it posts a system alert to Bridge: "All roles inactive for 10min"
