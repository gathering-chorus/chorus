@clearing @e2e
Feature: Clearing access paths
  Jeff accesses the Clearing from three environments.
  Each path must load, authenticate, accept a name, send a message,
  and show the message in the feed. When a path breaks, the test
  names which path and which step failed.

  Background:
    Given the Clearing is running on port 3470
    And the auth token is read from ~/.chorus/bridge-auth-token

  @public @iphone
  Scenario: Jeff via public URL on iPhone (wifi or 5G)
    When Jeff loads "https://clearing.lightlifeurbangardens.com" with token cookie
    Then the page returns 200
    And the page contains "The Clearing"
    When Jeff enters the name "jeff" via the public URL with token auth
    Then the name is accepted
    When Jeff sends a message "public-probe" via the API with token auth
    Then the message "public-probe" appears in the message feed
    When Jeff nudges silas with "e2e-public" via --force
    Then the nudge is delivered
    Then silas responds via the Clearing within 30 seconds

  @lan @iphone
  Scenario: Jeff via LAN IP on iPhone wifi
    When Jeff loads "http://192.168.86.36:3470" without auth
    Then the page returns 200
    And the page contains "The Clearing"
    When Jeff enters the name "jeff" via LAN
    Then the name is accepted
    When Jeff sends a message "lan-probe" via the API from LAN
    Then the message "lan-probe" appears in the message feed
    When Jeff nudges silas with "e2e-lan" via --force
    Then the nudge is delivered
    Then silas responds via the Clearing within 30 seconds

  @local @mac
  Scenario: Jeff via localhost on Library Mac Chrome
    When Jeff loads "http://localhost:3470" without auth
    Then the page returns 200
    And the page contains "The Clearing"
    When Jeff enters the name "jeff" via localhost
    Then the name is accepted
    When Jeff sends a message "local-probe" via the API from localhost
    Then the message "local-probe" appears in the message feed
    When Jeff nudges silas with "e2e-local" via --force
    Then the nudge is delivered
    Then silas responds via the Clearing within 30 seconds
