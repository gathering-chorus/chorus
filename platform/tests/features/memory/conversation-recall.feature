@memory @e2e @wip
Feature: Conversation recall
  A role asks "what did Jeff and Wren talk about this morning?"
  and gets a readable conversation thread — not 20 disconnected hits.
  Jeff's words are included as first-class messages.
  Time ranges work in Boston time.

  Background:
    Given the Chorus API is running on port 3340

  @conversation @core
  Scenario: Retrieve a conversation between two participants
    When a role requests the conversation between "jeff" and "wren" from today
    Then the response contains a conversation thread
    And each message has a speaker, text, and timestamp
    And messages are ordered chronologically
    And both Jeff's messages and Wren's messages appear in the thread

  @conversation @jeff-voice
  Scenario: Jeff's words are first-class — not buried in system tags
    When a role requests the conversation between "jeff" and "wren" from today
    Then Jeff's messages appear with speaker "jeff"
    And Jeff's messages contain his actual words — not skill loads or system reminders
    And Jeff's messages are not reconstructed from assistant context

  @conversation @time
  Scenario: Time ranges work in Boston time
    When a role requests the conversation between "jeff" and "wren" from "10:00" to "14:00" today
    Then all returned messages fall within 10:00 AM and 2:00 PM Boston time
    And timestamps display in Boston time — not UTC

  @conversation @thread
  Scenario: Results are a thread — not disconnected search hits
    When a role requests the conversation between "jeff" and "wren" from today
    Then the response is a single ordered thread — not ranked search results
    And there are no relevance scores or snippets
    And consecutive messages from the same speaker are not deduplicated

  @conversation @session
  Scenario: Conversation spans a full session — not fragments
    When a role requests the conversation between "jeff" and "wren" from today
    Then the thread includes the full session — not just keyword matches
    And messages that don't match a search term are still included
    And the conversation reads as a continuous exchange

  @conversation @empty
  Scenario: No conversation in time range returns empty — not an error
    When a role requests the conversation between "jeff" and "silas" from "03:00" to "04:00" today
    Then the response contains an empty thread
    And the response status is 200
