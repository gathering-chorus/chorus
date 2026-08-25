#!/usr/bin/env bats
# @test-type: unit — hermetic: fixture allowlist + fixture ui-pages JSON via a local file URL
# #4003 — the entrance-coverage check. Jeff's experience: a link rendered on his
# /chorus entrance that the governed allowlist does not carry must RED here,
# naming the path — instead of being discovered by clicking a dead tile (the
# #4002 Borg case, invisible in the guard's own log because every refusal reads
# as a plain 404).

CHECK="$BATS_TEST_DIRNAME/../scripts/check-entrance-coverage.sh"

setup() {
  ALLOW="$BATS_TEST_TMPDIR/allow.txt"
  UI="$BATS_TEST_TMPDIR/ui.json"
  # #4004 — the check now also reads the entrance HTML for the files it pulls in.
  # Default fixture: a page whose one asset IS carried, so the link tests below
  # keep measuring what they were written to measure.
  HTML="$BATS_TEST_TMPDIR/entrance.html"
  printf '<link rel="stylesheet" href="/css/system.css">' > "$HTML"
}

# file:// keeps the fixture hermetic — no port, no server process to leak.
# The effective allowlist is the test's own plus /css, so the default entrance
# fixture is fully covered and the LINK tests keep measuring only links.
run_check() {
  local eff="$BATS_TEST_TMPDIR/effective-allow.txt"
  cat "$ALLOW" > "$eff"
  printf '\n/css\n' >> "$eff"
  SHARE_ALLOW_FILE="$eff" UI_PAGES_URL="file://$UI" ENTRANCE_URL="file://$HTML" bash "$CHECK"
}

@test "covered: every rendered link carried by a section prefix or exact file → exit 0" {
  printf '/borg\n/loom\n/security.html\n' > "$ALLOW"
  printf '{"claimed":{"borg":[{"href":"/borg/index.html"}]},"misc":[{"href":"/loom/decisions.html"},{"href":"/security.html"}]}' > "$UI"
  run run_check
  [ "$status" -eq 0 ]
  [[ "$output" == *"3/3 rendered links covered"* ]]
}

@test "NEGATIVE PROOF: a rendered-but-unlisted link REDS and names the path (#3734)" {
  printf '/borg\n' > "$ALLOW"
  printf '{"claimed":{},"misc":[{"href":"/borg/index.html"},{"href":"/attention-analytics.html"}]}' > "$UI"
  run run_check
  [ "$status" -eq 1 ]
  [[ "$output" == *"/attention-analytics.html"* ]]
  [[ "$output" == *"UNCOVERED"* ]]
}

@test "NEGATIVE PROOF: prefix matching respects path boundaries — /borgX is not covered by /borg" {
  printf '/borg\n' > "$ALLOW"
  printf '{"claimed":{},"misc":[{"href":"/borgX/sneaky.html"}]}' > "$UI"
  run run_check
  [ "$status" -eq 1 ]
  [[ "$output" == *"/borgX/sneaky.html"* ]]
}

# --- #4004: the ASSET pass. The link pass reads ui-pages, a list of LINKS, so a
# file the page pulls in is invisible to it. #4001 shipped /ui-inventory.js at the
# root; the tunnel 404'd it, the script tag failed publicly, and the public
# entrance rendered 2 tiles where localhost showed 10 — while this check happily
# reported every link covered, because it never looked at the page.

@test "NEGATIVE PROOF: a script the entrance pulls in but the allowlist misses REDS (#3734)" {
  printf '/borg\n' > "$ALLOW"
  printf '{"claimed":{},"misc":[{"href":"/borg/index.html"}]}' > "$UI"
  printf '<script src="/ui-inventory.js"></script>' > "$HTML"
  run run_check
  [ "$status" -eq 1 ]
  [[ "$output" == *"/ui-inventory.js"* ]]
  [[ "$output" == *"UNCOVERED ASSETS"* ]]
}

@test "an asset carried by the allowlist passes — the check can tell the two states apart" {
  printf '/borg\n/ui-inventory.js\n' > "$ALLOW"
  printf '{"claimed":{},"misc":[{"href":"/borg/index.html"}]}' > "$UI"
  printf '<script src="/ui-inventory.js"></script>' > "$HTML"
  run run_check
  [ "$status" -eq 0 ]
  [[ "$output" == *"1/1 referenced assets covered"* ]]
}

@test "navigation links are NOT re-measured by the asset pass (the link pass owns them)" {
  # /flow is a nav link, not a file. Re-reporting it here would red on paths the
  # authoritative first pass already governs, and turn a precise check into noise.
  printf '/borg\n' > "$ALLOW"
  printf '{"claimed":{},"misc":[{"href":"/borg/index.html"}]}' > "$UI"
  printf '<a href="/flow">flow</a><link href="/css/system.css">' > "$HTML"
  run run_check
  [ "$status" -eq 0 ]
  [[ "$output" != *"/flow"* ]]
}

@test "UNMEASURABLE, never vacuous green: an unreachable ENTRANCE → exit 2" {
  printf '/borg\n' > "$ALLOW"
  printf '{"claimed":{},"misc":[{"href":"/borg/index.html"}]}' > "$UI"
  local eff="$BATS_TEST_TMPDIR/eff2.txt"; cat "$ALLOW" > "$eff"
  run env SHARE_ALLOW_FILE="$eff" UI_PAGES_URL="file://$UI" \
      ENTRANCE_URL="http://127.0.0.1:9/nope.html" bash "$CHECK"
  [ "$status" -eq 2 ]
  [[ "$output" == *"UNMEASURABLE"* ]]
}

@test "UNMEASURABLE, never vacuous green: unreachable ui-pages → exit 2" {
  printf '/borg\n' > "$ALLOW"
  run env SHARE_ALLOW_FILE="$ALLOW" UI_PAGES_URL="http://127.0.0.1:9/nope.json" bash "$CHECK"
  [ "$status" -eq 2 ]
  [[ "$output" == *"UNMEASURABLE"* ]]
}

@test "NEGATIVE PROOF: a bare / entry is the ROOT only — it must not score every link covered" {
  # This exact hole made the first version of this check report 77/77 while 33
  # links were genuinely dead: root-as-prefix is how a coverage check goes hollow.
  printf '/\n' > "$ALLOW"
  printf '{"claimed":{},"misc":[{"href":"/"},{"href":"/attention-analytics.html"}]}' > "$UI"
  run run_check
  [ "$status" -eq 1 ]
  [[ "$output" == *"/attention-analytics.html"* ]]
  [[ "$output" == *"1/2 rendered links covered"* ]]
}
