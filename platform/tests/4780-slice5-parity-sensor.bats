#!/usr/bin/env bats
# @test-type: unit
# @domain: identity — per-role macOS accounts (Slice 5 of #4780): 5-account parity sensor (#4788)

load test_helper

setup() {
  membrane_world
  ROOT="$CHORUS_ROOT"
  PARITY_SENSOR="$ROOT/platform/scripts/chorus-account-parity"

  FIXTURE_DIR="$BATS_TEST_TMPDIR/fixture"
  HOMES_DIR="$FIXTURE_DIR/homes"
  LIVE_SOURCE="$FIXTURE_DIR/live-source"

  mkdir -p "$LIVE_SOURCE/memory" \
           "$LIVE_SOURCE/skills/nudge" \
           "$LIVE_SOURCE/skills/pair" \
           "$LIVE_SOURCE/chorus/bin" \
           "$LIVE_SOURCE/chorus/scripts"

  echo "# live nudge skill" > "$LIVE_SOURCE/skills/nudge/SKILL.md"
  echo "# live pair skill" > "$LIVE_SOURCE/skills/pair/SKILL.md"
  echo "live-memory" > "$LIVE_SOURCE/memory/project1.jsonl"
  echo "live-bin" > "$LIVE_SOURCE/chorus/bin/tool"
  echo "live-script" > "$LIVE_SOURCE/chorus/scripts/script.sh"

  # Start mock search API for data reads (#4784 AC3)
  MOCK_PORT=$((25000 + RANDOM % 10000))
  python3 -c "
import http.server, sys
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b'{\"results\":[]}')
    def log_message(self, *a): pass
http.server.HTTPServer(('127.0.0.1', int(sys.argv[1])), H).serve_forever()
" "$MOCK_PORT" &
  MOCK_PID=$!
  MOCK_API="http://127.0.0.1:$MOCK_PORT/api/chorus/search"

  ROLES=("wren" "silas" "kade" "abby-normal" "borg-queen")
  for role in "${ROLES[@]}"; do
    local home="$HOMES_DIR/chorus-$role"
    mkdir -p "$home/.claude" "$home/.chorus"

    # Link memory, skills, and chorus to live source
    ln -s "$LIVE_SOURCE/memory" "$home/.claude/memory"
    mkdir -p "$home/.claude/skills"
    ln -s "$LIVE_SOURCE/skills/nudge" "$home/.claude/skills/nudge"
    ln -s "$LIVE_SOURCE/skills/pair" "$home/.claude/skills/pair"
    ln -s "$LIVE_SOURCE/chorus/bin" "$home/.chorus/bin"
    ln -s "$LIVE_SOURCE/chorus/scripts" "$home/.chorus/scripts"

    # Valid settings with chorus-api MCP
    cat <<EOF > "$home/.claude/settings.json"
{
  "enabledMcpjsonServers": ["chorus-api"]
}
EOF

    # Git config with safe.directory
    cat <<EOF > "$home/.gitconfig"
[safe]
	directory = *
EOF
  done
}

teardown() {
  if [ -n "${MOCK_PID:-}" ]; then
    kill "$MOCK_PID" 2>/dev/null || true
    wait "$MOCK_PID" 2>/dev/null || true
  fi
}

# --- AC1 & AC2 Positive Tests ---

@test "AC1: One parity check per account reports zero differences across all 5 roles" {
  run "$PARITY_SENSOR" --homes-dir "$HOMES_DIR" --live-source "$LIVE_SOURCE" --search-api "$MOCK_API"
  echo "$output"
  [ "$status" -eq 0 ]
  [[ "$output" =~ "0 differences across all 5 roles" ]] || return 1
}

@test "AC2: Each role reads one live memory, skills, and .chorus source" {
  mkdir -p "$LIVE_SOURCE/skills/board-sweep"
  echo "# live board-sweep" > "$LIVE_SOURCE/skills/board-sweep/SKILL.md"
  for role in "${ROLES[@]}"; do
    ln -s "$LIVE_SOURCE/skills/board-sweep" "$HOMES_DIR/chorus-$role/.claude/skills/board-sweep"
  done

  run "$PARITY_SENSOR" --homes-dir "$HOMES_DIR" --live-source "$LIVE_SOURCE" --search-api "$MOCK_API"
  echo "$output"
  [ "$status" -eq 0 ]
  [[ "$output" =~ "0 differences across all 5 roles" ]] || return 1
}

@test "AC1: --self mode checks only the current account" {
  CURRENT_USER="$(id -un)"
  mkdir -p "$HOMES_DIR/$CURRENT_USER/.claude" "$HOMES_DIR/$CURRENT_USER/.chorus"
  ln -s "$LIVE_SOURCE/memory" "$HOMES_DIR/$CURRENT_USER/.claude/memory"
  mkdir -p "$HOMES_DIR/$CURRENT_USER/.claude/skills"
  ln -s "$LIVE_SOURCE/skills/nudge" "$HOMES_DIR/$CURRENT_USER/.claude/skills/nudge"
  ln -s "$LIVE_SOURCE/skills/pair" "$HOMES_DIR/$CURRENT_USER/.claude/skills/pair"
  ln -s "$LIVE_SOURCE/chorus/bin" "$HOMES_DIR/$CURRENT_USER/.chorus/bin"
  ln -s "$LIVE_SOURCE/chorus/scripts" "$HOMES_DIR/$CURRENT_USER/.chorus/scripts"
  cat <<EOF > "$HOMES_DIR/$CURRENT_USER/.claude/settings.json"
{
  "enabledMcpjsonServers": ["chorus-api"]
}
EOF
  cat <<EOF > "$HOMES_DIR/$CURRENT_USER/.gitconfig"
[safe]
	directory = *
EOF

  run "$PARITY_SENSOR" --homes-dir "$HOMES_DIR" --live-source "$LIVE_SOURCE" --search-api "$MOCK_API" --self
  echo "$output"
  [ "$status" -eq 0 ]
  [[ "$output" =~ "0 differences with live source" ]] || return 1
}

# --- Negative Proofs (#3734) ---

@test "NEGATIVE PROOF 1: a symlink pointing to a frozen copy outside live source fails and names account" {
  mkdir -p "$BATS_TEST_TMPDIR/frozen-copy"
  rm -f "$HOMES_DIR/chorus-kade/.claude/memory"
  ln -s "$BATS_TEST_TMPDIR/frozen-copy" "$HOMES_DIR/chorus-kade/.claude/memory"

  run "$PARITY_SENSOR" --homes-dir "$HOMES_DIR" --live-source "$LIVE_SOURCE" --search-api "$MOCK_API"
  echo "$output"
  [ "$status" -ne 0 ]
  [[ "$output" =~ "chorus-kade" ]] || return 1
  [[ "$output" =~ "memory" ]] || return 1
}

@test "NEGATIVE PROOF 2: missing live source directory fails loudly and never passes vacuously" {
  mkdir -p "$BATS_TEST_TMPDIR/empty-live"
  run "$PARITY_SENSOR" --homes-dir "$HOMES_DIR" --live-source "$BATS_TEST_TMPDIR/empty-live" --search-api "$MOCK_API"
  echo "$output"
  [ "$status" -eq 2 ]
  [[ "$output" =~ "[parity-error] live skills directory missing" ]] || return 1
}

@test "NEGATIVE PROOF 3: missing live scripts fails and names account" {
  rm -f "$HOMES_DIR/chorus-wren/.chorus/scripts"
  run "$PARITY_SENSOR" --homes-dir "$HOMES_DIR" --live-source "$LIVE_SOURCE" --search-api "$MOCK_API"
  echo "$output"
  [ "$status" -ne 0 ]
  [[ "$output" =~ "chorus-wren" ]] || return 1
  [[ "$output" =~ "scripts" ]] || return 1
}

@test "NEGATIVE PROOF 4: missing settings.json fails and names account" {
  rm -f "$HOMES_DIR/chorus-silas/.claude/settings.json"
  run "$PARITY_SENSOR" --homes-dir "$HOMES_DIR" --live-source "$LIVE_SOURCE" --search-api "$MOCK_API"
  echo "$output"
  [ "$status" -ne 0 ]
  [[ "$output" =~ "chorus-silas" ]] || return 1
  [[ "$output" =~ "settings.json" ]] || return 1
}

@test "NEGATIVE PROOF 5: settings with permission allow line but no chorus-api server fails and names account" {
  cat <<EOF > "$HOMES_DIR/chorus-silas/.claude/settings.json"
{
  "permissions": {
    "allow": ["mcp__chorus-api__*"]
  },
  "enabledMcpjsonServers": ["other-server"]
}
EOF
  run "$PARITY_SENSOR" --homes-dir "$HOMES_DIR" --live-source "$LIVE_SOURCE" --search-api "$MOCK_API"
  echo "$output"
  [ "$status" -ne 0 ]
  [[ "$output" =~ "chorus-silas" ]] || return 1
  [[ "$output" =~ "missing chorus-api server" ]] || return 1
}

@test "NEGATIVE PROOF 6: unreachable search API for data reads fails and names account (#4784 AC3)" {
  DEAD_PORT=$((36000 + RANDOM % 10000))
  run "$PARITY_SENSOR" --homes-dir "$HOMES_DIR" --live-source "$LIVE_SOURCE" --search-api "http://127.0.0.1:$DEAD_PORT/api/chorus/search"
  echo "$output"
  [ "$status" -ne 0 ]
  [[ "$output" =~ "data read failed: search API unreachable" ]] || return 1
}

@test "NEGATIVE PROOF 7: an unreadable home directory reports unreadable rather than missing" {
  chmod 000 "$HOMES_DIR/chorus-abby-normal"
  run "$PARITY_SENSOR" --homes-dir "$HOMES_DIR" --live-source "$LIVE_SOURCE" --search-api "$MOCK_API"
  chmod 755 "$HOMES_DIR/chorus-abby-normal"
  echo "$output"
  [ "$status" -ne 0 ]
  [[ "$output" =~ "unreadable" ]] || return 1
  [[ "$output" =~ "chorus-abby-normal" ]] || return 1
}
