@clearing @e2e
Feature: Clearing access paths
  Jeff accesses the Clearing from three environments.
  Each path must load, authenticate, accept a name, send a message,
  and show the message in the feed. When a path breaks, the test
  names which path and which step failed.

  # #2617 (2026-04-30): the prior "nudge silas via --force" + "delivered" +
  # "silas responds" steps were retired. They invoked real nudges into a
  # live role's session as a side effect of running the test, which leaked
  # [e2e-test] noise into Jeff's view all morning. DEC-107 makes nudge
  # delivery non-hermetic by design (osascript + spine-tick-poller, both
  # always fire). There is no hermetic way to assert "nudge delivered"; the
  # right shape is to scope this feature to clearing-API behavior and let
  # nudge delivery be tested elsewhere (chorus-hooks/tests/nudge_suite.rs,
  # gated behind RUN_INTEGRATION per #2614).

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

  @lan @iphone
  Scenario: Jeff via LAN IP on iPhone wifi
    When Jeff loads "http://192.168.86.36:3470" without auth
    Then the page returns 200
    And the page contains "The Clearing"
    When Jeff enters the name "jeff" via LAN
    Then the name is accepted
    When Jeff sends a message "lan-probe" via the API from LAN
    Then the message "lan-probe" appears in the message feed

  @local @mac
  Scenario: Jeff via localhost on Library Mac Chrome
    When Jeff loads "http://localhost:3470" without auth
    Then the page returns 200
    And the page contains "The Clearing"
    When Jeff enters the name "jeff" via localhost
    Then the name is accepted
    When Jeff sends a message "local-probe" via the API from localhost
    Then the message "local-probe" appears in the message feed
