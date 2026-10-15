# @test-type: bdd — Gherkin scenarios for Clearing v2 Roles page, run by cucumber-js
# @card: #4530
# @owner: abby-normal
# @domain: messages
@clearing-v2 @roles
Feature: Roles page: who is here and what they are doing
  As Jeff
  I want to see the status, current card, and pane for every principal
  So that I never have to ask if someone is still working

  Background:
    Given Clearing v2 is running on port 3472

  Scenario: View all principals with their login and active session status
    When Jeff loads the Roles page
    Then Jeff sees all five principals listed with login status and active session
    And each row displays the principal's current card and last heard time

  Scenario: Track five distinct role states across the team
    When viewing the roles status table
    Then each principal shows one of "working", "waiting on you", "blocked", "stale", or "off"
    And "waiting on you" clearly indicates what decision or input is owed

  Scenario: Automatically mark a silent role as stale after thirty minutes
    Given a role has sent no events for more than 30 minutes
    When Jeff loads the Roles page
    Then the role status displays as "stale" with the elapsed silence duration

  Scenario: See current focus and terminal pane location for each role and Jeff
    # Note: Pane focus history ("you looked HH:MM") is deferred to Part 2 per Jeff 10:12
    When Jeff loads the Roles page
    Then each role indicates its active host and terminal pane
    And Jeff's row shows a "you are here" marker indicating his current surface
