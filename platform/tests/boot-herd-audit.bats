#!/usr/bin/env bats
# @test-type: unit
# Subject: the boot-herd audit (launchagent-boot-audit.sh) and the canonical
# plists it guards. #4027 named the class: an interval agent with RunAtLoad=true
# fires at boot on top of every service. 18 of them put the Library at load 527.
#
# 2026-09-04: my own com.chorus.log-harvest.plist shipped with RunAtLoad=true AND
# StartInterval — the audit flagged it, deep-health died on the finding, and four
# suites went red in the nightly. The plist was fixed in the INSTALLED copy at
# 04:33; this suite is what stops the repo copy from putting it back at the next
# deploy.
load test_helper

AUDIT="$CHORUS_ROOT/platform/scripts/launchagent-boot-audit.sh"
CANONICAL="$CHORUS_ROOT/platform/scripts/launchagents-canonical"

@test "no canonical interval LaunchAgent fires at boot" {
  run "$AUDIT" "$CANONICAL"
  [ "$status" -eq 0 ] || {
    echo "$output"
    false
  }
}

@test "NEGATIVE PROOF: an interval plist WITH RunAtLoad still trips the audit" {
  # Without this, the test above is green whenever the audit has stopped being
  # able to see a violation at all — a deleted plist dir, a renamed key, a
  # broken plutil call all read as "clean". Show the audit going red on the
  # exact shape it exists to catch.
  local dir="$BATS_TEST_TMPDIR/herd"
  mkdir -p "$dir"
  cat > "$dir/com.chorus.fixture-herd.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>com.chorus.fixture-herd</string>
    <key>ProgramArguments</key><array><string>/bin/true</string></array>
    <key>StartInterval</key><integer>3600</integer>
    <key>RunAtLoad</key><true/>
</dict>
</plist>
PLIST
  run "$AUDIT" "$dir"
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'boot-herd: com.chorus.fixture-herd'
}

@test "NEGATIVE PROOF: a KeepAlive service with RunAtLoad is NOT a violation" {
  # The other half of the discrimination: the audit must separate an interval
  # job from a service. A check that flags both cannot tell us anything.
  local dir="$BATS_TEST_TMPDIR/svc"
  mkdir -p "$dir"
  cat > "$dir/com.chorus.fixture-svc.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>com.chorus.fixture-svc</string>
    <key>ProgramArguments</key><array><string>/bin/true</string></array>
    <key>KeepAlive</key><true/>
    <key>RunAtLoad</key><true/>
</dict>
</plist>
PLIST
  run "$AUDIT" "$dir"
  [ "$status" -eq 0 ]
}

@test "log-harvest keeps its interval — the fix removed RunAtLoad, not the schedule" {
  # Deleting StartInterval would also make the audit green, and would silently
  # stop the hourly harvest. Pin the half that must survive.
  run plutil -extract StartInterval raw "$CANONICAL/com.chorus.log-harvest.plist"
  [ "$status" -eq 0 ]
  [ "$output" = "3600" ]
}
