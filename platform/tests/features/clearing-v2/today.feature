# @test-type: bdd — Gherkin scenarios for Clearing v2 Today page, run by cucumber-js
# @card: #4530
# @owner: abby-normal
# @domain: messages
@clearing-v2 @today
Feature: Today page: where Jeff lands
  As Jeff
  I want to see what needs my hand and what moved while I slept
  So that I land in the morning with zero friction

  Background:
    Given Clearing v2 is running on port 3472

  Scenario: Review and answer decisions owed grouped by meeting run
    When Jeff loads the Today page
    Then Jeff sees decisions owed grouped by meeting run
    And Jeff can accept or approve-all proposals with attached evidence

  Scenario: Review what changed overnight since last visit
    Given Jeff's last Clearing session ended at "23:45"
    When Jeff loads the Today page
    Then Jeff sees what changed since his last session ended with cards that landed, moved, or became blocked
    And any additional card movements beyond four are folded

  Scenario: Review new or changed alerts requiring attention
    When Jeff loads the Today page
    Then Jeff sees only alerts that are new or changed
    And each active alert links to its resolving card

  Scenario: See who is working right now and on what card
    When Jeff loads the Today page
    Then Jeff sees who is working right now with their active card and status dot
