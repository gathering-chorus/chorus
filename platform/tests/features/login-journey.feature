# @test-type: bdd
# @domain: identity
# login-journey.feature — #4409. login.feature checks one rule per scenario.
# This file follows a role through a day. Jeff, 2026-09-30: "our bdd for login
# is organized around specific requirements but doesnt really cover the overall
# flow". A leg whose fix has not landed carries @waiting-<card>: it reads RED by
# name in every report and does not block a land.
@login
Feature: A role's day, from login to logout and back

  Background:
    Given a fixture world with the principals jeff (person), wren, silas and kade (agents), and bridge (service)

  # Silas navigating, 11:27: "logged in → takes a turn → idle → Jeff sends from
  # Clearing → typed at once → reply reaches Clearing → exits Claude and Jeff
  # logs in again → same session, new run"
  Scenario: A day in the room, end to end
    Given wren is logged in
    When wren takes a turn
    And wren goes idle with a grey suggestion on its input line
    And Jeff sends wren a message from the Clearing
    Then the message is typed into wren's pane at once, whole
    When wren replies
    Then the reply shows in Jeff's Clearing
    Given wren's session is written down
    When wren exits Claude and Jeff logs wren in again
    Then wren is back in the same session with a new run

  # ---- identity (AC2), Silas navigating ---------------------------------

  # The presence must follow the NEW run after /exit → login (#4406 resume).
  # pulse writes "[logged out — no live Presence]" when no presence belongs to
  # a live run (pulse/src/service.ts).
  Scenario: After /exit and login again, pulse still finds wren through its Presence
    Given wren is logged in
    And wren takes a turn
    When wren exits Claude and Jeff logs wren in again
    And wren takes a turn
    Then pulse resolves wren to its pane from the Presence row

  # Loud, not a refusal: Jeff decides. Login checks identity, chorus-api and
  # athena-make (lifecycle.rs DEFAULT_SERVICES), and the hooks daemon's socket (#4409).
  Scenario: Logging in with the hooks daemon down says so
    Given wren is logged out
    And the hooks daemon is not answering
    When Jeff runs "chorus-principal login wren"
    Then wren starts anyway
    And the login output says the hooks daemon is down

  # OPEN (#4400): which row is the grant — a Permission row or holdsRole?
  # chorus-principal's login verdict (rows.rs) checks only principalKind.
  # Commented out (Jeff, 2026-10-04): the feature is unbuilt, so this read red every night. Restore when #4400 lands.
  # @waiting-4400
  # Scenario: A principal with no role grant cannot log in as a role
  #   Given wren's Principal row grants it no role
  #   When Jeff runs "chorus-principal login wren"
  #   Then the login is refused, naming the missing role grant, and no session row is written

  # ---- the live reply gap (Silas navigating, 11:49) ---------------------
  # 09-30: 2 of 9 finals never shown in the Clearing; it showed a line written
  # partway through the turn ("Now the journey, per Silas's direction.") as
  # the answer, and dropped the real one.
  # In-process this is GREEN (11:55): the tailer shows the final even when a
  # tool outlasts the quiet window. Kept as a guard; the 2 live cases were
  # turns a peer's nudge started, not Jeff.
  Scenario: A turn that narrates, runs tools, then answers shows the answer in the Clearing
    Given Jeff has asked wren something from the Clearing
    When wren writes a line partway through the turn and runs a tool
    And wren finishes the turn with its answer
    Then the Clearing shows wren's answer

  # Jeff, 04:57: "im not seeing messages from any of u in clearing". Since
  # #4362 (11:26) pulse types the nudge itself into the pane, header included.
  @waiting-4409
  Scenario: A reply to a peer's nudge shows in the Clearing
    Given silas's nudge is typed into wren's pane
    When wren finishes the turn with its answer
    Then the Clearing shows wren's answer
