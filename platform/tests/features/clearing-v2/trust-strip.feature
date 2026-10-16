# @test-type: bdd — Gherkin scenarios for Clearing v2 Trust Strip, run by cucumber-js
# @card: #4530
# @owner: abby-normal
# @domain: messages
@clearing-v2 @trust-strip
Feature: Trust strip: calm freshness on every page
  As Jeff
  I want a calm trust strip showing last write time per source
  So that I know how fresh my data is without false alarms

  Background:
    Given Clearing v2 is running on port 3472

  Scenario: View data freshness and last write timestamp for each source
    Given the trust strip endpoint returns source, last successful write, and status for each source
    When Jeff loads any Clearing page
    Then Jeff sees a trust strip with chips for board, rooms, decisions, and roles
    And each chip displays its source name and "as of" timestamp

  Scenario: Display source lag as elapsed time without false alarms
    Given a backend source has not written for 34 minutes
    When Jeff looks at the trust strip
    Then the lag is displayed as elapsed time with a neutral grey indicator
    And the indicator is not colored red

  Scenario: Highlight red only when a specific named source fails
    Given a backend source has experienced an unrecovered error
    When Jeff looks at the trust strip
    Then only the failing source is highlighted red
    And the chip clearly names the failed source
