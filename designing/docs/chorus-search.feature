# Chorus Search — behavioral acceptance scenarios (BDD)
#
# Owner: Wren (search / Knowledge domain).
# Run against the LIVE index unless tagged @fixture. A search/relevance change is
# "done" when its affected scenarios pass live. Add a scenario any time a query
# should work and doesn't — that's the living gate.
#
# Tags: @gate = gates a current change · @live = runs on the real index ·
#       @fixture = needs a seeded hermetic index · @pending = blocked on other work
#
# Implementation: step definitions hit /api/chorus/search; lands in
# platform/api/tests/features/ when Silas's werk-build cascade fix clears.

Feature: Chorus search surfaces the right thing
  As someone asking Chorus a question
  I want search to return authoritative knowledge and the recent thread
  So that answers are grounded in what's true, not drowned in chatter

  @authority @gate @live
  Scenario: a knowledge query returns the doc, not chatter
    Given the live chorus index
    When I search "heidegger" in relevance mode
    Then a result whose source is doc, decision, artifact, or memory and mentions "versammlung" appears in the top 5
    And the top result is not session chatter (source claude or clearing)

  @recency @live
  Scenario: session recall returns the recent thread in order
    Given the live chorus index
    When I request recent context for channel "session:wren" with order recent and limit 30
    Then at most 30 results are returned
    And every result is from that channel
    And results are in descending timestamp order

  @echo @fixture
  Scenario: search does not echo my own prompt back as context
    Given an index containing my most recent prompt
    When I search using my own recent prompt text
    Then my own current prompt is not the top result

  @dedup @fixture
  Scenario: duplicate content does not consume result slots
    Given an index where the same line is recorded as several rows
    When I search a term that matches those rows in relevance mode
    Then no two results in the top 5 share identical content

  @absence @live
  Scenario: a genuinely absent term returns nothing
    Given the live chorus index
    When I search a token that exists nowhere in the corpus
    Then exactly zero results are returned
    And no full-table-scan fallback runs

  @structured @live
  Scenario: a structured fact surfaces, not only text hits
    Given the live chorus index
    When I search a known entity in unified mode
    Then the structured fact about that entity is among the results

  @metadata @live
  Scenario: search reports its own quality
    Given the live chorus index
    When I run any search
    Then the response carries _meta with stale, domain_coverage, and newest_result_age_s

  @semantic @pending
  Scenario: knowledge is findable without naming the term
    # PENDING — blocked until the knowledge corpus is embedded (today only `messages`
    # is in LanceDB; this is the projection contract from chorus-search-tobe.svg)
    Given the knowledge docs are embedded in the semantic index
    When I search "the German philosophy the product is named after" in semantic mode
    Then the Versammlung research doc appears in the top results
