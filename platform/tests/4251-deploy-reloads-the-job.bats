#!/usr/bin/env bats
# @test-type: unit — reads the installer's text and exercises the plist finder against fixture plists in a tmpdir; no launchctl, no live job
# @domain: deploys — the product domain this suite guards (#4334)
# #4251 — a deploy that replaces a binary a launchd job runs must reload that
# job. On 2026-09-21 the 03:00 nightly never launched: the #4247 land replaced
# werk-test-bin at 21:10, launchd held the previous code signature for
# com.chorus.nightly-suites and refused to start it (OS_REASON_CODESIGNING).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  INSTALL="$ROOT/platform/scripts/chorus-bin-install"
  TMP="$BATS_TEST_TMPDIR"
  mkdir -p "$TMP/agents"
}

# The finder: the plist that NAMES the destination path is the one to reload,
# whatever the job is called. The old block derived the unit from the binary
# name, which is why a job named after its schedule was never found.
plists_naming() {
  local dest="$1" dir="$2" out=""
  for p in "$dir"/*.plist; do
    [ -e "$p" ] || continue
    grep -q "$dest" "$p" 2>/dev/null || continue
    out="$out $(basename "$p" .plist)"
  done
  echo "$out" | tr -s ' ' | sed 's/^ //;s/ $//'
}

@test "#4251 a job named after its SCHEDULE is found by the path it runs" {
  cat > "$TMP/agents/com.chorus.nightly-suites.plist" <<PLIST
<plist><dict>
<key>Label</key><string>com.chorus.nightly-suites</string>
<key>ProgramArguments</key><array>
  <string>/Users/x/.chorus/bin/werk-test-bin</string><string>--nightly</string>
</array>
</dict></plist>
PLIST
  run plists_naming "/Users/x/.chorus/bin/werk-test-bin" "$TMP/agents"
  [ "$status" -eq 0 ]
  [ "$output" = "com.chorus.nightly-suites" ]
}

@test "#4251 NEGATIVE PROOF: deriving the unit from the binary name finds nothing" {
  cat > "$TMP/agents/com.chorus.nightly-suites.plist" <<PLIST
<plist><dict><key>ProgramArguments</key><array>
  <string>/Users/x/.chorus/bin/werk-test-bin</string></array></dict></plist>
PLIST
  # what the pre-#4251 code looked for
  [ ! -e "$TMP/agents/com.chorus.werk-test-bin.plist" ]
  [ ! -e "$TMP/agents/com.chorus.werk-test.plist" ]
  # and what it would therefore have reloaded: nothing
  run plists_naming "/Users/x/.chorus/bin/chorus-hooks" "$TMP/agents"
  [ "$status" -eq 0 ]
  [ -z "$output" ]
}

@test "#4251 a plist that does not name the path is left alone" {
  cat > "$TMP/agents/com.chorus.other.plist" <<PLIST
<plist><dict><key>ProgramArguments</key><array>
  <string>/Users/x/.chorus/bin/chorus-api</string></array></dict></plist>
PLIST
  run plists_naming "/Users/x/.chorus/bin/werk-test-bin" "$TMP/agents"
  [ "$status" -eq 0 ]
  [ -z "$output" ]
}

@test "#4251 the installer reloads by path, and never kickstarts a scheduled job" {
  grep -q 'grep -q "\$DEST" "\$plist"' "$INSTALL"
  grep -q 'launchctl bootout' "$INSTALL"
  grep -q 'launchctl bootstrap' "$INSTALL"
  # kickstart would RUN the nightly at deploy time — a different bug.
  # Match the CALL, not the word: the block's comments name kickstart to say
  # why it is wrong here, and grepping the word passed on those comments alone.
  run bash -c "sed -n '/4251/,/^fi\$/p' '$INSTALL' | grep -v '^[[:space:]]*#' | grep -c 'launchctl kickstart'"
  [ "$output" = "0" ]
}

@test "#4251 a failed reload is LOUD and names the consequence" {
  grep -q "could NOT be reloaded" "$INSTALL"
  grep -q "launchd still holds the PREVIOUS signature" "$INSTALL"
  grep -q "binary.job.reloaded" "$INSTALL"
}
