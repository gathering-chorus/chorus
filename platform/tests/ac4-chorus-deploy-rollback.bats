#!/usr/bin/env bats
# #2925 AC4 — chorus-deploy chorus-api --rollback + wrapper auto-rollback.
#
# Rollback contract: chorus-api npm-builds into dist/. Before each new build,
# the existing dist/ is preserved as dist.prev/. --rollback swaps dist.prev/
# back into dist/ and kickstarts the service. One-shot — second rollback in a
# row fails with no-prior-deploy message.
#
# Wrapper change: deploy-daemon-card.sh on probe failure invokes
# `chorus-deploy chorus-api --rollback` and exits non-zero.

DEPLOY_SCRIPT="$BATS_TEST_DIRNAME/../scripts/chorus-deploy"
WRAPPER="$BATS_TEST_DIRNAME/../scripts/deploy-daemon-card.sh"

setup() {
  MOCK_ROOT=$(mktemp -d -t ac4-rollback.XXXXXX)
  mkdir -p "$MOCK_ROOT/platform/api/dist"
  echo "OLD_BUILD" > "$MOCK_ROOT/platform/api/dist/server.js"

  STUBDIR=$(mktemp -d -t ac4-stubs.XXXXXX)
  CALLS="$STUBDIR/calls.log"
  : > "$CALLS"

  # Stub launchctl so kickstart doesn't try to touch real launchd.
  cat > "$STUBDIR/launchctl" <<EOF
#!/bin/bash
echo "launchctl \$*" >> "$CALLS"
exit 0
EOF
  chmod +x "$STUBDIR/launchctl"

  # Stub npm — pretend the build succeeds and writes new content to dist/.
  cat > "$STUBDIR/npm" <<EOF
#!/bin/bash
echo "npm \$*" >> "$CALLS"
if [ "\$1" = "run" ] && [ "\$2" = "build" ]; then
  # Simulate build replacing dist/ contents
  rm -rf "$MOCK_ROOT/platform/api/dist"
  mkdir -p "$MOCK_ROOT/platform/api/dist"
  echo "NEW_BUILD" > "$MOCK_ROOT/platform/api/dist/server.js"
fi
exit \${STUB_npm_EXIT:-0}
EOF
  chmod +x "$STUBDIR/npm"

  export PATH="$STUBDIR:$PATH"
  export CHORUS_ROOT="$MOCK_ROOT"
  export DEPLOY_ROLE="silas"
}

teardown() {
  rm -rf "$MOCK_ROOT" "$STUBDIR"
}

@test "deploy chorus-api: preserves prior dist as dist.prev before build" {
  run "$DEPLOY_SCRIPT" chorus-api
  [ "$status" -eq 0 ]
  [ -d "$MOCK_ROOT/platform/api/dist.prev" ]
  [ -f "$MOCK_ROOT/platform/api/dist.prev/server.js" ]
  grep -q OLD_BUILD "$MOCK_ROOT/platform/api/dist.prev/server.js"
  grep -q NEW_BUILD "$MOCK_ROOT/platform/api/dist/server.js"
}

@test "rollback: --rollback restores dist from dist.prev + kickstarts" {
  # First do a deploy to create dist.prev
  "$DEPLOY_SCRIPT" chorus-api >/dev/null
  # Now rollback
  run "$DEPLOY_SCRIPT" chorus-api --rollback
  [ "$status" -eq 0 ]
  grep -q OLD_BUILD "$MOCK_ROOT/platform/api/dist/server.js"
  # dist.prev should be consumed (one-shot)
  [ ! -d "$MOCK_ROOT/platform/api/dist.prev" ]
  # launchctl was kickstarted
  grep -q 'kickstart.*com.chorus.api' "$CALLS"
}

@test "rollback: refuses if no dist.prev exists" {
  run "$DEPLOY_SCRIPT" chorus-api --rollback
  [ "$status" -ne 0 ]
  [[ "$output" == *"prev"* ]] || [[ "$output" == *"rollback"* ]] || [[ "$output" == *"prior"* ]]
}

@test "rollback: second rollback in a row fails (dist.prev consumed)" {
  "$DEPLOY_SCRIPT" chorus-api >/dev/null
  "$DEPLOY_SCRIPT" chorus-api --rollback >/dev/null
  run "$DEPLOY_SCRIPT" chorus-api --rollback
  [ "$status" -ne 0 ]
}

@test "wrapper: probe-fail invokes chorus-deploy chorus-api --rollback" {
  # Stub chorus-deploy + cards + chorus-werk-sync for the wrapper test
  WRAP_STUBS=$(mktemp -d -t ac4-wrap.XXXXXX)
  WRAP_CALLS="$WRAP_STUBS/calls.log"
  : > "$WRAP_CALLS"
  for cmd in chorus-werk-sync chorus-deploy cards; do
    cat > "$WRAP_STUBS/$cmd" <<EOF
#!/bin/bash
echo "$cmd \$*" >> "$WRAP_CALLS"
exit 0
EOF
    chmod +x "$WRAP_STUBS/$cmd"
  done
  PATH="$WRAP_STUBS:$PATH" run "$WRAPPER" 2925 --probe "exit 1"
  [ "$status" -ne 0 ]
  grep -q 'chorus-deploy chorus-api --rollback' "$WRAP_CALLS"
  # cards done should NOT fire after rollback
  ! grep -q 'cards done' "$WRAP_CALLS"
  rm -rf "$WRAP_STUBS"
}
