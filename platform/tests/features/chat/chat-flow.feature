@chat @e2e
Feature: Chat flow — two-role direct channel
  Two roles open a direct channel, exchange messages, and close
  with a summary. Jeff can read the transcript at any time.
  When the chat ends, the closer owns the summary.

  Background:
    Given the chat script is available

  Scenario: Start a chat and get a session ID
    When wren starts a chat with kade about "test audit"
    Then a CHAT_ID is returned
    And the transcript file exists

  Scenario: Send and receive messages
    Given wren started a chat with kade about "gate review"
    When wren says "How's the test coverage looking?"
    Then the message appears in the transcript with speaker "wren"
    When kade says "546 tests, 76% mocked"
    Then the message appears in the transcript with speaker "kade"
    And the transcript has 2 messages

  Scenario: Read since a line number returns only new messages
    Given wren started a chat with kade about "incremental read"
    And wren says "first message"
    And kade says "second message"
    When wren reads since line 5
    Then only the new message is returned

  Scenario: Jeff reads the full transcript
    Given wren started a chat with kade about "jeff visibility"
    And wren says "opening"
    And kade says "reply"
    When jeff reads the chat
    Then the transcript shows both messages with timestamps and speakers

  Scenario: End chat saves transcript and emits event
    Given wren started a chat with kade about "close test"
    And wren says "done here"
    When wren ends the chat
    Then the chat is marked ended
    And the transcript is saved to /tmp/chorus-chat/

  Scenario: Either role can end the chat
    Given wren started a chat with kade about "either-end test"
    And wren says "question"
    And kade says "answer"
    When kade ends the chat
    Then the chat is marked ended

  @smoke @real-delivery
  Scenario: Real delivery smoke test — one chat hits the full nudge path
    When kade starts a real chat with wren about "smoke test"
    And kade says "integration smoke" with real delivery
    Then the nudge was delivered — not dry-run
