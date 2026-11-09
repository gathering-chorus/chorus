#!/usr/bin/env bats
# @test-type: integration
# @domain: werk
# #4773 AC7: werk-deploy-crate rollback / chorus-bin-install --rollback
# reinstalls the kept binary, kickstarts the service.
# Negative proof: a rollback with no kept binary refuses naming the crate,
# never installs nothing and reports ok.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  SCRIPT="$ROOT/platform/scripts/chorus-bin-install"
  TEST_DIR="$(mktemp -d)"
  export HOME="$TEST_DIR"
  export CHORUS_BIN_NO_KICKSTART=1
  export CHORUS_BIN_SKIP_SMOKE=1
  export CHORUS_BIN_SKIP_SIGCHECK=1
  export CHORUS_BIN_SPINE_LOG="$TEST_DIR/spine.log"
  mkdir -p "$TEST_DIR/src" "$TEST_DIR/.chorus/bin"
}

teardown() {
  rm -rf "$TEST_DIR"
}

@test "4773 AC7 negative proof: rollback with no kept binary refuses naming binary" {
  run "$SCRIPT" --rollback "non-existent-tool"
  [ "$status" -ne 0 ]
  [[ "$output" =~ "refused" ]] || [[ "$output" =~ "no previous binary" ]]
  [[ "$output" =~ "non-existent-tool" ]] || return 1
}

@test "4773 AC7: chorus-bin-install --rollback restores .prev binary" {
  cat <<'EOF' > "$TEST_DIR/src/my-tool-v1"
#!/usr/bin/env bash
echo "v1-original"
EOF
  chmod +x "$TEST_DIR/src/my-tool-v1"

  cat <<'EOF' > "$TEST_DIR/src/my-tool-v2"
#!/usr/bin/env bash
echo "v2-broken"
EOF
  chmod +x "$TEST_DIR/src/my-tool-v2"

  # Install v1 then v2
  "$SCRIPT" "$TEST_DIR/src/my-tool-v1" "my-tool"
  "$SCRIPT" "$TEST_DIR/src/my-tool-v2" "my-tool"

  [ "$("$TEST_DIR/.chorus/bin/my-tool")" = "v2-broken" ]

  # Now trigger rollback
  run "$SCRIPT" --rollback "my-tool"
  [ "$status" -eq 0 ]
  [ "$("$TEST_DIR/.chorus/bin/my-tool")" = "v1-original" ]
}

@test "4773 AC7: werk-deploy exact argv --target canonical --rollback <name> restores v1" {
  cat <<'EOF' > "$TEST_DIR/src/my-tool-v1"
#!/usr/bin/env bash
echo "v1-target-test"
EOF
  chmod +x "$TEST_DIR/src/my-tool-v1"

  cat <<'EOF' > "$TEST_DIR/src/my-tool-v2"
#!/usr/bin/env bash
echo "v2-target-test"
EOF
  chmod +x "$TEST_DIR/src/my-tool-v2"

  # Install v1 then v2
  "$SCRIPT" --target canonical "$TEST_DIR/src/my-tool-v1" "my-target-tool"
  "$SCRIPT" --target canonical "$TEST_DIR/src/my-tool-v2" "my-target-tool"
  [ "$("$TEST_DIR/.chorus/bin/my-target-tool")" = "v2-target-test" ]

  # Rollback with werk-deploy's exact argv
  run "$SCRIPT" --target canonical --rollback "my-target-tool"
  [ "$status" -eq 0 ]
  [ "$("$TEST_DIR/.chorus/bin/my-target-tool")" = "v1-target-test" ]
}

@test "4773 AC6: failed .prev copy warns and emits binary.prev.failed" {
  cat <<'EOF' > "$TEST_DIR/src/my-tool-v1"
#!/usr/bin/env bash
echo "v1"
EOF
  chmod +x "$TEST_DIR/src/my-tool-v1"

  cat <<'EOF' > "$TEST_DIR/src/my-tool-v2"
#!/usr/bin/env bash
echo "v2"
EOF
  chmod +x "$TEST_DIR/src/my-tool-v2"

  "$SCRIPT" "$TEST_DIR/src/my-tool-v1" "read-only-prev-tool"

  # Create a directory with mode 555 where .prev would go, to force copy/move failure
  mkdir -p "$TEST_DIR/.chorus/bin/read-only-prev-tool.prev"
  chmod 555 "$TEST_DIR/.chorus/bin/read-only-prev-tool.prev"

  run "$SCRIPT" "$TEST_DIR/src/my-tool-v2" "read-only-prev-tool"

  # Clean up directory mode so teardown can remove it
  chmod 755 "$TEST_DIR/.chorus/bin/read-only-prev-tool.prev"

  [ "$status" -eq 0 ]
  [[ "$output" =~ "WARNING" ]] || return 1
  [[ "$output" =~ "failed to preserve previous binary" ]] || return 1
  grep -q "binary.prev.failed" "$TEST_DIR/spine.log"
}

