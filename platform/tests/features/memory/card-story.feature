@memory @e2e
Feature: Card story
  Jeff asks "what happened on card #1930?" and gets a timeline
  from six data sources — not scattered fragments across six systems.
  The card becomes the thread that connects conversations, events,
  nudges, and domain context into one story.

  Background:
    Given the Chorus API is running on port 3340

  @card-story @core
  Scenario: Retrieve the story of a real card
    When a role requests the card story for card 1946
    Then the response contains a timeline
    And each entry has a timestamp, source, and text
    And entries are ordered chronologically
    And the response includes the card title and domain

  @card-story @sources
  Scenario: Timeline includes multiple data sources
    When a role requests the card story for card 1946
    Then the timeline includes entries from at least 2 different sources
    And possible sources are "vikunja", "chorus-index", "spine", "nudge", "domain"

  @card-story @metadata
  Scenario: Card metadata is included
    When a role requests the card story for card 1946
    Then the response includes the card owner
    And the response includes the card status
    And the response includes the card domain

  @card-story @spine
  Scenario: Spine events appear in timeline
    When a role requests the card story for card 1946
    Then the timeline includes at least one spine event
    And spine events show the event type and role

  @card-story @domain
  Scenario: Domain story — all cards and conversations for a domain
    When a role requests the domain story for "seeds"
    Then the response contains cards tagged with that domain
    And the response contains conversation mentions from the Chorus index
    And cards and mentions are combined into a single timeline
    And the timeline spans the full history — not just recent cards

  @card-story @empty
  Scenario: Nonexistent card returns empty story — not an error
    When a role requests the card story for card 99999
    Then the response status is 200
    And the timeline is empty or contains only metadata
