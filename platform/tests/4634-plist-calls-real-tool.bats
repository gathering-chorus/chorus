#!/usr/bin/env bats
# @test-type: integration — reads the repo's LaunchAgents; the live case reads the door's Skill rows and is unmeasured without it
# @domain: services
# #4634 — the sensor agents called a tool name that no longer existed for a day and
# a half (herald_run, then borg-tend-herald; the tool is herald-run). This fails
# when any agent calls a tool the runner doesn't offer.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  CHECK="$ROOT/platform/scripts/plist-tool-check.py"
  T="$(mktemp -d)"
  echo '{"data":[{"name":"herald-run"},{"name":"clearing-nudge"}]}' > "$T/skills.json"
}

teardown() { rm -rf "$T"; }

agent() { # agent <file> <tool>
  cat > "$T/$1" <<PL
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>ProgramArguments</key><array>
<string>/bin/sh</string><string>-c</string>
<string>/x/platform/scripts/chorus-mcp-call.sh silas $2 '{"sensor":"infra"}'</string>
</array></dict></plist>
PL
}

@test "an agent calling a tool that exists passes" {
  agent a.plist herald-run
  run python3 "$CHECK" "$T" "$T/skills.json"
  [ "$status" -eq 0 ]
}

@test "negative proof: an agent calling a renamed tool fails, by name" {
  agent a.plist borg-tend-herald
  run python3 "$CHECK" "$T" "$T/skills.json"
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"a.plist: calls borg-tend-herald"* ]] || return 1
}

@test "the repo's LaunchAgents all call tools the live door offers" {
  curl -sf -m 10 'http://localhost:3360/v1/skills/skills?limit=1000' -o "$T/live.json" \
    || skip "UNMEASURED: the door's Skill rows could not be read"
  run python3 "$CHECK" "$ROOT/platform/launchd" "$T/live.json"
  echo "$output"
  [ "$status" -eq 0 ] || return 1
}
