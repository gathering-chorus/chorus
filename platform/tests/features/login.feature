# @test-type: bdd — the login scenarios Jeff reads, run by cucumber-js (steps: step_definitions/login_steps.ts)
# @domain: identity
# login.feature — #4367. Jeff 2026-09-27: "start at the top (ie login) and work our way down".
# Wren + Silas. Actors are principals (identity domain):
#   human   jeff, debmajumdar, marknakib — attends a session, never logs in
#   agent   wren, silas, kade — logs in with `chorus-principal login <name>`
#   service bridge, crawler, nightly, chorus-sdk, flow-probe — a credential, no login
# Every step drives the real chorus-principal binary in the fixture world of
# lib/login-harness.bash (stub tmux, claude, token minter, curl; the "store" is the
# row bodies the stub curl keeps) — never prod.
# A scenario tagged @waiting-<card> has a step that card has not built yet; that
# step reports pending, and the scenario is RED in every report, named with its
# card — a held test is a red with another word on it (Jeff). It does not block a
# card that touches this file; a real failure anywhere does.

@login
Feature: A principal logs in, works, and logs out

  Background:
    Given a fixture world with the principals jeff (person), wren, silas and kade (agents), and bridge (service)

  Scenario: Getting in is one obvious step
    Given wren is logged out
    When Jeff runs "chorus-principal login wren"
    Then wren has one open session, one live run and one presence
    And the command says wren is logged in

  Scenario: Login records the conversation before the first prompt
    Given wren is logged out
    When Jeff runs "chorus-principal login wren"
    And Claude's SessionStart hook fires in wren's pane
    Then wren's conversation is recorded against the run
    And nobody is recorded as having spoken

  @waiting-4378
  Scenario: The role speaks first, naming where they left off
    Given wren is logged out
    When Jeff runs "chorus-principal login wren"
    Then wren's first line to Jeff names where wren left off, before Jeff types anything

  Scenario: A turn keeps the session current
    Given wren is logged in
    When wren takes a turn
    Then the same session row's last-seen time moves to the turn's time
    And no second session row is written

  Scenario: A live session never reads as expired
    Given wren is logged in
    And wren's login token has been renewed
    When wren takes a turn
    Then the session's expiry moves forward to the renewed token's expiry

  Scenario: Another role's token never moves a session's expiry
    Given wren is logged in
    And wren's token names kade
    When wren takes a turn
    Then the session's expiry does not move

  Scenario: An unchanged focus is still re-checked
    Given wren is logged in
    And wren's presence was checked more than ten minutes ago and the focus has not changed
    When wren takes a turn
    Then the presence's checked time moves to now

  Scenario: Jeff attends a session; he never logs in
    Given wren is logged in
    When Jeff types in wren's pane
    Then wren's session is attended by jeff, with the time he spoke
    And no session is owned by jeff

  Scenario: A peer message wakes the role, it does not speak for Jeff
    Given wren is logged in
    When silas sends wren a nudge
    Then wren's presence is reachable
    And nobody is recorded as having spoken

  Scenario: Text that only looks like a nudge label is not a delivery
    Given wren is logged in
    When a prompt arrives that starts with a nudge label but did not come from the relay
    Then wren's presence is not reachable
    And wren's session is attended by jeff, with the time he spoke

  Scenario: A nudge never lands in Jeff's half-typed prompt
    Given Jeff is typing in wren's pane
    When silas sends wren a nudge
    Then the nudge waits until Jeff's prompt is sent, and Jeff's text arrives whole

  Scenario: A message finds the role through its Presence
    Given wren is logged in
    When silas sends wren a nudge
    Then the relay found wren's pane through wren's Presence row, not a registry file

  Scenario: A second login makes no second session
    Given wren is logged in and talking
    When Jeff runs "chorus-principal login wren"
    Then the command says wren is already logged in
    And nothing is sent to wren's pane
    And no session row is written

  Scenario: A pane running as the wrong role is repaired
    Given wren's pane is running a process that carries kade's role
    When Jeff runs "chorus-principal login wren"
    Then that pane is ended and wren starts again
    And the command says wren is logged in

  Scenario: Login with identity down starts the role, degraded and loud
    Given the identity service gives no token
    When Jeff runs "chorus-principal login wren"
    Then wren starts anyway
    And the command says the login is pending, and why

  Scenario: A refused row write says why
    Given wren is logged in
    And the store refuses the next row write, naming a reason and a token
    When wren takes a turn
    Then the spine says the session row failed, with the store's reason
    And the token is not on the spine

  Scenario: A service principal has no login
    Given bridge's Principal row says it is a service
    When Jeff runs "chorus-principal login bridge"
    Then it is refused with "bridge is a service principal; it acts with its credential, it does not log in"
    And nothing is started

  Scenario: A name with no Principal row cannot log in
    When Jeff runs "chorus-principal login bob"
    Then it is refused with "no Principal row named bob. Who exists: chorus-principal census"
    And nothing is started

  Scenario: Logout closes what login opened
    Given wren is logged in
    When Jeff runs "chorus-principal logout wren"
    Then the run ends as logout, the presence is unreachable, and the session is closed, in that order

  Scenario: Leaving with /exit records exit, not logout
    Given wren is logged in
    When wren's session ends with /exit
    Then the run ends with the reason exit, not logout

  Scenario: A lapsed session is closed by the sweep
    Given wren is logged in
    And kade has an open session whose expiry has passed
    When the sweep runs
    Then kade's lapsed session is closed
    And wren's live session is left alone

  # Added with #4348 (Wren's web research, 2026-09-27): standard session controls.
  Scenario: A principal runs as its own account, so no other principal can read its token
    Given wren's Principal row names the Mac account chorus-wren
    When Jeff runs "chorus-principal login wren"
    Then wren starts as chorus-wren
    And wren's credentials are in chorus-wren's home, readable by that account alone

  @waiting-4384
  Scenario: A session has an absolute lifetime, even while it renews
    Given wren is logged in
    And wren's session has kept renewing past its absolute lifetime
    When wren takes a turn
    Then the session is closed and wren is asked to log in again

  Scenario: Revoking a principal ends its session on the next turn
    Given wren is logged in
    When wren's principal is revoked
    And wren takes a turn
    Then the turn is refused, wren's run ends as revoked and wren's session is closed

  Scenario: An event a logged-in role emits names its principal and session
    Given wren is logged in
    When wren's pane writes a spine event for wren
    And wren's pane writes a spine event for kade
    Then wren's event names principal-wren and wren's session
    And the event for kade names neither
