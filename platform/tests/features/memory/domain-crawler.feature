@memory @e2e
Feature: Domain crawler
  A role asks "tell me about the seeds domain" and gets a connected
  subgraph — cards, RDF triples, conversations, spine events, domain
  page context, and OWL class definitions. Linked, not listed.

  Background:
    Given the Chorus API is running on port 3340
    And Fuseki is running on port 3030

  @crawler @core @wip
  Scenario: Crawl a domain and return a connected subgraph
    When a role crawls the "seeds" domain
    Then the response contains cards tagged with that domain
    And the response contains RDF triples from Fuseki for that domain
    And the response contains conversation mentions from the Chorus index
    And the response contains spine events for cards in that domain
    And all sources are linked into a single connected subgraph

  @crawler @rdf @pending @wip
  Scenario: Crawler reaches Fuseki for domain triples
    When a role crawls the "seeds" domain
    Then the response includes RDF class definitions
    And the response includes instance counts per class
    And the response includes relationships to other domains

  @crawler @owl @pending @wip
  Scenario: OWL classes provide the schema for linking
    When a role crawls the "seeds" domain
    Then the response includes OWL properties for the domain class
    And properties link to code artifacts — handlers, routes, services
    And the OWL relationships connect seeds to related domains

  @crawler @code @wip
  Scenario: Crawler links cards to code files
    When a role crawls the "seeds" domain
    Then the response includes source files that implement the domain
    And source files are found via blast radius comments on cards
    And source files are found via git log for domain-tagged commits

  @crawler @infra
  Scenario: Crawler includes infrastructure context
    When a role crawls the "seeds" domain
    Then the response includes LaunchAgents related to the domain
    And the response includes API endpoints serving the domain
    And the response includes monitoring or alerting for the domain

  @crawler @links @wip
  Scenario: Link phase connects entities across layers
    When a role crawls the "seeds" domain
    Then cards reference code files they changed
    And code files map to OWL classes
    And conversations reference card numbers
    And spine events reference cards and roles
    And the subgraph has cross-layer connections — not isolated lists

  @crawler @related
  Scenario: Crawler surfaces related domains
    When a role crawls the "seeds" domain
    Then the response includes domains that share cards or conversations
    And related domains are ranked by connection strength
    And "photos" appears as a related domain — seed photo delivery

  @crawler @code-scan @wip
  Scenario: Crawler scans actual codebase for domain files
    When a role crawls the "seeds" domain
    Then the response includes code files found by directory scan
    And the code files are real paths — not just extracted from card descriptions
    And the code section distinguishes card-referenced files from scan-discovered files

  @crawler @logs @wip
  Scenario: Crawler includes recent Loki log entries for the domain
    When a role crawls the "seeds" domain
    Then the response includes recent log entries from Loki
    And logs are filtered by domain-relevant component or keyword
    And each log entry includes timestamp, level, and message
    And error-level logs appear before info-level logs

  @crawler @alerts @wip
  Scenario: Crawler includes Grafana alert rules for the domain
    When a role crawls the "seeds" domain
    Then the response includes alert rules from alerting/ directory
    And alert rules are matched by domain keyword in the YAML filename or content
    And each alert includes name, severity, and current state

  @crawler @history
  Scenario: Crawler shows the institutional memory
    When a role crawls the "seeds" domain
    Then the response includes a trust score or health summary
    And the response surfaces unresolved cards — open issues in the domain
    And the response includes Jeff's recurring feedback on this domain
