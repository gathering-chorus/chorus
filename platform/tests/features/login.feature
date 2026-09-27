# @test-type: bdd — the login scenarios Jeff reads; run by login-feature.test.sh
# @domain: identity
# login.feature — #4367. Jeff 2026-09-27: "start at the top (ie login) and work our way down".
# Wren + Silas. Actors are principals (identity domain):
#   human   jeff, debmajumdar, marknakib — attends a session, never logs in
#   agent   wren, silas, kade — logs in with `chorus-principal login <name>`
#   service bridge, crawler, nightly, chorus-sdk, flow-probe — a credential, no login
# Each scenario names the bats case that proves it (`# proven by: <file> :: <title>`)
# or the card it waits on (`# waiting on: #N`). A waiting scenario runs RED with its
# card number — a held test is a red with another word on it (Jeff). Steps run on
# the fixture harness (stub tmux/claude/curl, fixture store), never prod.
# Runner: platform/tests/login-feature.test.sh

Feature: A principal logs in, works, and logs out

  Scenario: Getting in is one obvious step
    # proven by: 4328-session-rows.bats :: login writes the session with its role and start, then a run, a presence and a boot context
    # proven by: 4295-role-login.bats :: on: a stopped role starts logged in, and the line says logged in

  Scenario: Login records the conversation before the first prompt
    # proven by: 4328-session-rows.bats :: #4345 login includes session start: the SessionStart hook names the conversation before any prompt

  Scenario: The role speaks first, naming where they left off
    # waiting on: #4378

  Scenario: A turn keeps the session current
    # proven by: 4328-session-rows.bats :: each turn updates last-seen on the same session row, and names the conversation

  Scenario: A live session never reads as expired
    # proven by: 4328-session-rows.bats :: #4377 a renewed login moves the session's expiry; a live session never reads expired

  Scenario: An unchanged focus is still re-checked
    # proven by: 4328-session-rows.bats :: #4367 an unchanged focus is re-checked after ten minutes

  Scenario: Jeff attends a session; he never logs in
    # proven by: 4328-session-rows.bats :: #4367 Jeff attends a session and never owns one

  Scenario: A peer message wakes the role, it does not speak for Jeff
    # proven by: 4328-session-rows.bats :: a delivered nudge makes the presence reachable; an ordinary turn does not

  Scenario: Text that only looks like a nudge label is not a delivery
    # proven by: 4328-session-rows.bats :: #4367 a prompt that only looks like a nudge label is not a delivery

  Scenario: A nudge never lands in Jeff's half-typed prompt
    # waiting on: #4362

  Scenario: A message finds the role through its Presence
    # waiting on: #4361

  Scenario: A second login makes no second session
    # proven by: 4202-principal-login.bats :: idempotent: a role that is awake AND talking is not logged in again (nothing sent)

  Scenario: A pane running as the wrong role is repaired
    # proven by: 4337-role-carries-its-role.bats :: a process carrying another role is repaired by on itself: that pane ends and the role starts again, logged in

  Scenario: Login with identity down starts the role, degraded and loud
    # proven by: 4202-principal-login.bats :: #4215 no token → the role STARTS anyway, degraded and loud

  Scenario: A refused row write says why
    # proven by: 4328-session-rows.bats :: #4367 a refused row write says why on the spine

  Scenario: A service principal has no login
    # waiting on: #4368

  Scenario: Logout closes what login opened
    # proven by: 4328-session-rows.bats :: off ends the run as logout, the presence goes unreachable, then the session closes

  Scenario: Leaving with /exit records exit, not logout
    # proven by: 4328-session-rows.bats :: #4367 a session ended by /exit records exit, not logout

  Scenario: A lapsed session is closed by the sweep
    # proven by: 4328-session-rows.bats :: sweep closes an expired open session and leaves the live login alone

  # Added with #4348 (Wren's web research, 2026-09-27): standard session controls.
  Scenario: A login copied from another pane is refused
    # waiting on: #4383

  Scenario: A session has an absolute lifetime, even while it renews
    # waiting on: #4384

  Scenario: Revoking a principal ends its session on the next turn
    # waiting on: #4385

  Scenario: Every login event names the principal, session and run
    # waiting on: #4369
