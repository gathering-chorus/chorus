# @test-type: bdd — the login scenarios Jeff reads; run by login-feature.test.sh
# @domain: identity
# login.feature — #4367. Jeff 2026-09-27: "start at the top (ie login) and work our way down".
# Wren + Silas. Actors are principals (identity domain):
#   human   jeff, debmajumdar, marknakib — attends a session, never logs in
#   agent   wren, silas, kade — logs in with `chorus-principal login <name>`
#   service bridge, crawler, nightly, chorus-sdk, flow-probe — a credential, no login
# Every scenario has Given/When/Then steps. The bats cases that prove a scenario are
# named from it: `@test "login: <scenario title>"`, and a second case for the same
# scenario adds " — <what it checks>". Rename a scenario and its cases stop matching:
# the runner reds it. A case named "login: …" that matches no scenario is red too.
# A scenario waiting on a card says `# waiting on: #N` and runs RED with that card
# number — a held test is a red with another word on it (Jeff). Cases run on the
# fixture harness (stub tmux/claude/curl, fixture store), never prod.
# Runner: platform/tests/login-feature.test.sh. Cucumber steps come with #4388.

Feature: A principal logs in, works, and logs out

  Background:
    Given a fixture store with the principals jeff (person), wren, silas and kade (agents), and bridge (service)

  Scenario: Getting in is one obvious step
    Given wren is logged out
    When Jeff runs "chorus-principal login wren"
    Then wren has one open session, one live run and one presence
    And the command's last line says wren is logged in

  Scenario: Login records the conversation before the first prompt
    Given wren is logged out
    When Jeff runs "chorus-principal login wren"
    Then the SessionStart hook names wren's conversation on the session row before any prompt

  Scenario: The role speaks first, naming where they left off
    # waiting on: #4378
    Given wren is logged out and wren's last session left off on a card
    When Jeff runs "chorus-principal login wren"
    Then wren's first line to Jeff names where wren left off, before Jeff types anything

  Scenario: A turn keeps the session current
    Given wren is logged in
    When wren takes a turn
    Then the same session row's last-seen time moves to the turn's time
    And no second session row is written

  Scenario: A live session never reads as expired
    Given wren is logged in and wren's login token has been renewed
    When wren takes a turn
    Then the session's expiry moves forward to the renewed token's expiry
    And another role's token does not move wren's expiry

  Scenario: An unchanged focus is still re-checked
    Given wren's presence was checked more than ten minutes ago and wren's focus has not changed
    When wren takes a turn
    Then the presence's checked time moves to now

  Scenario: Jeff attends a session; he never logs in
    Given wren is logged in
    When Jeff types in wren's pane
    Then wren's session is attended by jeff, with the time he spoke
    And no session is owned by jeff

  Scenario: A peer message wakes the role, it does not speak for Jeff
    Given wren and silas are logged in
    When silas sends wren a nudge
    Then wren's presence is reachable
    And an ordinary turn does not mark the presence reachable

  Scenario: Text that only looks like a nudge label is not a delivery
    Given wren is logged in
    When a prompt arrives that starts with a nudge label but did not come from the relay
    Then it is not recorded as a delivery
    And the session is attended by jeff

  Scenario: A nudge never lands in Jeff's half-typed prompt
    # #4362 landed; its proof is pulse's delivery-worker and pane-input tests (TypeScript),
    # which this bats runner cannot run. They join when #4388 moves these steps onto cucumber.
    # waiting on: #4388
    Given Jeff is typing in wren's pane
    When silas sends wren a nudge
    Then the nudge waits until Jeff's prompt is sent
    And Jeff's text arrives whole

  Scenario: A message finds the role through its Presence
    # waiting on: #4361
    Given wren is logged in and wren's presence names wren's pane
    When silas sends wren a nudge
    Then the relay reads the pane from wren's Presence row, not from a registry file

  Scenario: A second login makes no second session
    Given wren is logged in and talking
    When Jeff runs "chorus-principal login wren" again
    Then nothing is sent to wren's pane
    And wren still has one open session

  Scenario: A pane running as the wrong role is repaired
    Given wren's pane is running a process that carries another role
    When Jeff runs "chorus-principal login wren"
    Then that pane's process ends
    And wren starts again, logged in as wren

  Scenario: Login with identity down starts the role, degraded and loud
    Given the identity service gives no token
    When Jeff runs "chorus-principal login wren"
    Then wren starts anyway
    And the command says the login is degraded and why

  Scenario: A refused row write says why
    Given the store refuses the session row write
    When wren takes a turn
    Then the spine event for the failure carries the store's reason, with any token masked

  Scenario: A service principal has no login
    # waiting on: #4368
    Given bridge is a service principal
    When Jeff runs "chorus-principal login bridge"
    Then it is refused with "bridge is a service principal; it acts with its credential, it does not log in"

  Scenario: Logout closes what login opened
    Given wren is logged in
    When Jeff runs "chorus-principal logout wren"
    Then the run ends as logout, the presence is unreachable, and the session is closed

  Scenario: Leaving with /exit records exit, not logout
    Given wren is logged in
    When wren's session ends with /exit
    Then the run ends with the reason exit, not logout

  Scenario: A lapsed session is closed by the sweep
    Given one open session whose expiry has passed and one live login
    When the sweep runs
    Then the lapsed session is closed
    And the live login is left alone

  # Added with #4348 (Wren's web research, 2026-09-27): standard session controls.
  Scenario: A login copied from another pane is refused
    # waiting on: #4383
    Given wren is logged in in wren's own pane
    When wren's session token is used from another pane
    Then the call is refused and names the run the token belongs to

  Scenario: A session has an absolute lifetime, even while it renews
    # waiting on: #4384
    Given wren's session has kept renewing past its absolute lifetime
    When wren takes a turn
    Then the session is closed and wren is asked to log in again

  Scenario: Revoking a principal ends its session on the next turn
    # waiting on: #4385
    Given wren is logged in
    When wren's principal is revoked
    Then wren's next turn is refused and wren's session is closed

  Scenario: Every login event names the principal, session and run
    # waiting on: #4369
    Given wren is logged in
    When wren's login writes a spine event
    Then the event carries principal wren, wren's session and wren's run, read from the login
