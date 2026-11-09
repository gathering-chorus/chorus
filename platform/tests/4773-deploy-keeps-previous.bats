#!/usr/bin/env bats
# @test-type: integration
# @domain: cicd
# #4773 AC6: every deploy keeps the previous installed binary per crate
# (path + sha recorded). Regression suite for 2026-11-09 rollback outage.

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

@test "4773 AC6: first install places binary, no .prev created" {
  cat <<'EOF' > "$TEST_DIR/src/my-tool-v1"
#!/usr/bin/env bash
echo "v1"
EOF
  chmod +x "$TEST_DIR/src/my-tool-v1"

  run "$SCRIPT" "$TEST_DIR/src/my-tool-v1" "my-tool"
  [ "$status" -eq 0 ]
  [ -f "$TEST_DIR/.chorus/bin/my-tool" ]
  [ ! -f "$TEST_DIR/.chorus/bin/my-tool.prev" ]
  [ "$("$TEST_DIR/.chorus/bin/my-tool")" = "v1" ]
}

@test "4773 AC6: second install moves existing binary to <name>.prev and records sha" {
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

  # First install v1
  "$SCRIPT" "$TEST_DIR/src/my-tool-v1" "my-tool"

  V1_SHA=$(shasum -a 256 "$TEST_DIR/.chorus/bin/my-tool" | awk '{print $1}')

  # Second install v2
  run "$SCRIPT" "$TEST_DIR/src/my-tool-v2" "my-tool"
  [ "$status" -eq 0 ]
  [ -f "$TEST_DIR/.chorus/bin/my-tool" ]
  [ -f "$TEST_DIR/.chorus/bin/my-tool.prev" ]

  # Current is v2, prev is v1
  [ "$("$TEST_DIR/.chorus/bin/my-tool")" = "v2" ]
  [ "$("$TEST_DIR/.chorus/bin/my-tool.prev")" = "v1" ]

  PREV_SHA=$(shasum -a 256 "$TEST_DIR/.chorus/bin/my-tool.prev" | awk '{print $1}')
  [ "$PREV_SHA" = "$V1_SHA" ]

  # Output logs the kept prev binary sha
  [[ "$output" =~ "prev=" ]] || [[ "$output" =~ "$PREV_SHA" ]]
}

@test "4773 AC6: third install advances <name>.prev to the superseded binary" {
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

  cat <<'EOF' > "$TEST_DIR/src/my-tool-v3"
#!/usr/bin/env bash
echo "v3"
EOF
  chmod +x "$TEST_DIR/src/my-tool-v3"

  "$SCRIPT" "$TEST_DIR/src/my-tool-v1" "my-tool"
  "$SCRIPT" "$TEST_DIR/src/my-tool-v2" "my-tool"
  run "$SCRIPT" "$TEST_DIR/src/my-tool-v3" "my-tool"

  [ "$status" -eq 0 ]
  [ "$("$TEST_DIR/.chorus/bin/my-tool")" = "v3" ]
  [ "$("$TEST_DIR/.chorus/bin/my-tool.prev")" = "v2" ]
}
