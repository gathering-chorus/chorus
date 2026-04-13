#!/usr/bin/env bats
# Bridge subscriber path fix — #1964

SCRIPT="$BATS_TEST_DIRNAME/../bridge-subscriber.js"

@test "bridge-subscriber resolves socket.io-client from CHORUS_ROOT" {
  # The script must use CHORUS_ROOT, not a hardcoded absolute path
  grep -q 'CHORUS_ROOT' "$SCRIPT"
  ! grep -q "CascadeProjects/directing/clearing" "$SCRIPT"
}

@test "socket.io-client exists at the resolved path" {
  CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
  [ -d "$CHORUS_ROOT/directing/clearing/node_modules/socket.io-client" ]
}

@test "all 3 bridge-subscriber agents have a PID" {
  for role in silas wren kade; do
    pid=$(launchctl list 2>/dev/null | grep "bridge-subscriber-$role" | awk '{print $1}')
    [ "$pid" != "-" ]
  done
}
