@build-deploy-skill
Feature: /build + /deploy pipeline — substrate behaviors
  The building-pipeline runs on every /acp: release-trigger fires, chorus-build
  signs binaries in target/release/, chorus-deploy atomically installs to
  ~/.chorus/bin/, kickstarts services. Six fixes in the last 30-40 cards
  on this pipeline (#2870 path-filter, #2876 card_id propagation, #2877
  force-with-lease, #2863 canonical-sync, #2734 deploy location, #2871
  crawler tunes) — every one caught by Jeff in a live session, not by tests.
  These scenarios exercise the actual substrate (chorus-log, chorus-build,
  chorus-bin-install, building-pipeline-health) — same convention as #2875.

  Background:
    Given the chorus.log spine is writable
    And the chorus bin install dir exists

  # --- Scenario 1: card_id propagates as integer through bash spine emit (#2876) ---

  Scenario: chorus-log emits card_id as unquoted integer for numeric-key allowlist
    Given a unique trace marker for this run
    When chorus-log emits build.push.completed with card_id=2876 and exit_code=0
    Then the build-deploy event lands in chorus.log within 5 seconds
    And the event JSON has card_id as an integer not a string
    And the event JSON has exit_code as an integer not a string

  Scenario: chorus-log emits non-numeric keys as string even when value is digits
    Given a unique trace marker for this run
    When chorus-log emits build.queue.acquired with title=12345 and card_id=2876
    Then the build-deploy event lands in chorus.log within 5 seconds
    And the event JSON has card_id as an integer
    And the event JSON has title as a string with value "12345"

  # --- Scenario 2: chorus-bin-install atomic move + binary.deployed event ---

  Scenario: chorus-bin-install atomically installs and emits binary.deployed
    Given a unique trace marker for this run
    And a fixture binary exists at a temp path
    When chorus-bin-install installs the fixture binary under a fixture name
    Then the binary lands in the chorus bin install dir under the fixture name
    And a binary.deployed spine event fires with the fixture name

  # --- Scenario 3: chorus-build canonical-sync invariant ---

  Scenario: chorus-build aborts loudly when CHORUS_HOME is not a git tree
    Given a temp dir that is not a git repo
    When chorus-build chorus-hooks runs against the temp dir as CHORUS_HOME
    Then the script exits non-zero
    And the stderr contains "ABORT" and mentions canonical-sync

  # --- Scenario 4: building-pipeline-health pairs acp ↔ trigger ↔ pipeline ---

  # The fitness function pairs events by trace_id + time window. These two
  # scenarios verify the pairing logic against synthesized fixture events.
  # Tests use direct chorus.log reads (CHORUS_LOG_DIRECT=1), not Loki, to
  # avoid index lag flakiness — script extension to support direct-read
  # mode tagged @gap-2881 below.

  @wip @gap-2881
  Scenario: building-pipeline-health passes when chain is complete
    Given a unique trace marker for this run
    And a synthesized chorus_acp.completed event for card 99100
    And a matching chorus_acp.release-trigger.completed event for card 99100
    And a matching deploy.completed event for card 99100
    When building-pipeline-health runs in direct-read mode against the trace marker
    Then the script exits 0
    And the JSON output has unpaired_release_trigger=0 and unpaired_pipeline_run=0

  @wip @gap-2881
  Scenario: building-pipeline-health flags when release-trigger missing
    Given a unique trace marker for this run
    And a synthesized chorus_acp.completed event for card 99200
    And no release-trigger event for card 99200
    When building-pipeline-health runs in direct-read mode against the trace marker
    Then the script exits 1
    And the JSON output has unpaired_release_trigger>=1
    And the affected card 99200 appears in the verbose output

  # --- Scenario 5: git-queue exports CHORUS_CARD_ID from branch (#2876) ---

  Scenario: git-queue export_card_id_from_branch sets env from role/N branch
    Given a temp git repo on branch silas/2876
    When the export_card_id_from_branch helper runs
    Then CHORUS_CARD_ID equals "2876"

  Scenario: git-queue export_card_id_from_branch leaves CHORUS_CARD_ID unset on main
    Given a temp git repo on branch main
    When the export_card_id_from_branch helper runs
    Then CHORUS_CARD_ID is unset
