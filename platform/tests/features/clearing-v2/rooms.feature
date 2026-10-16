# @test-type: bdd — Gherkin scenarios for Clearing v2 Rooms page, run by cucumber-js
# @card: #4530
# @owner: abby-normal
# @domain: messages
@clearing-v2 @rooms
Feature: Rooms page: talk with roles and people
  As Jeff
  I want to communicate with the team and individual roles
  So that working chatter stays contained in threads and delivery is confirmed

  Background:
    Given Clearing v2 is running on port 3472

  Scenario: Switch between team rooms and direct conversations
    When Jeff loads the Rooms page
    Then Jeff sees a list of rooms including "#team" and direct conversations
    And Jeff can switch to "#team" room

  Scenario: Post a message into a room and see delivery confirmation
    When Jeff posts a message "Morning team" into the "#team" room
    Then Jeff sees the message appear in the feed with a delivery confirmation tick

  Scenario: Keep role chatter organized in collapsible threads
    When role chatter occurs in response to a message
    Then the role responses are grouped in a thread under the message
    And the main room feed remains uncluttered

  Scenario: Open a card directly from any message or mention
    When Jeff views a message referencing a card "#4530"
    Then clicking the card reference opens the card in a side drawer
    And Jeff remains in the conversation view
