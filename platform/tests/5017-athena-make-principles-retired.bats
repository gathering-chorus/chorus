#!/usr/bin/env bats
# @test-type: integration — serves the card's athena-make build on a free port, reading the pod
# @domain: principles
# @card: 5017 · owner: kade
# What Jeff sees: one door for principles. principles-api serves them; athena-make,
# which served them until #5017, no longer does, and nothing in the tree reads
# principles from athena-make's port.
#
# AC3 NEGATIVE: athena-make's /v1/principles/principles answers 404 (the principles
# Domain row carries its own server.bind Property, so athena-make does not mount its
# classes), while its other routes still answer; no reader calls that path on :3360.
# Each check is also shown red: on a door that still serves principles, and on a
# fixture tree with a direct reader.

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"

# The card's athena-make: ATHENA_MAKE_BIN, else the workspace build (built once if
# missing; a failed build fails the test, never skips it).
card_make() {
  local c="${ATHENA_MAKE_BIN:-${CARGO_TARGET_DIR:-$REPO/target}/release/athena-make}"
  [ -x "$c" ] || (cd "$REPO" && cargo build --release --quiet -p athena-make) >&2 || return 1
  [ -x "$c" ] && echo "$c"
}

free_port() { python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])'; }

code() { curl -s -o /dev/null -w '%{http_code}' --max-time 20 "$1"; }

# The AC's door check: principles 404 on both spellings, and the door is up (domains 200).
retired() {
  local base="$1"
  [ "$(code "$base/v1/domains/domains")" = 200 ] &&
  [ "$(code "$base/v1/principles/principles")" = 404 ] &&
  [ "$(code "$base/principles/principles")" = 404 ]
}

# The AC's tree check: every tracked file outside tests, fixtures and prose that reads
# principles from athena-make's port. Prints them; empty = none.
direct_readers() {
  (cd "$1" && git ls-files -z | xargs -0 grep -lE '(:3360|127\.0\.0\.1:3360|localhost:3360)/(v1/)?principles' 2>/dev/null) |
    grep -vE '(^|/)tests?/|\.test\.|_test\.|fixtures/|\.md$|\.html$|^designing/(docs|schemas)/' || true
}

teardown() { [ -n "${PID:-}" ] && { kill "$PID" 2>/dev/null; wait "$PID" 2>/dev/null; }; true; }

wait_up() {
  local i
  for i in $(seq 1 120); do
    [ "$(code "$1/v1/domains/domains")" = 200 ] && return 0
    sleep 1
  done
  return 1
}

@test "AC3: the card's athena-make serves domains but not principles (404)" {
  local bin port
  bin="$(card_make)" || { echo "athena-make not built"; false; }
  port="$(free_port)"
  "$bin" serve --port "$port" >"$BATS_TEST_TMPDIR/make.log" 2>&1 &
  PID=$!
  wait_up "http://127.0.0.1:$port" || { tail -20 "$BATS_TEST_TMPDIR/make.log"; false; }
  retired "http://127.0.0.1:$port" || {
    echo "principles: $(code "http://127.0.0.1:$port/v1/principles/principles")"; false; }
  # it said so at boot: Principle is not among the classes it mounted
  ! grep -q '+ Principle API' "$BATS_TEST_TMPDIR/make.log" || return 1
}

@test "AC3 NEGATIVE: a door that still serves principles fails the check" {
  local port dir
  port="$(free_port)"; dir="$BATS_TEST_TMPDIR/door"
  mkdir -p "$dir/v1/domains" "$dir/v1/principles"
  echo '{"data":[]}' >"$dir/v1/domains/domains"
  echo '{"data":[{"name":"hemenway-observe"}]}' >"$dir/v1/principles/principles"
  python3 -m http.server "$port" --bind 127.0.0.1 --directory "$dir" >/dev/null 2>&1 &
  PID=$!
  local i; for i in $(seq 1 40); do [ "$(code "http://127.0.0.1:$port/v1/domains/domains")" = 200 ] && break; sleep 0.25; done
  ! retired "http://127.0.0.1:$port" || return 1
}

@test "AC3: no reader in the tree calls principles on :3360" {
  run direct_readers "$REPO"
  [ -z "$output" ] || { echo "direct readers of athena-make principles:"; echo "$output"; false; }
}

@test "AC3 NEGATIVE: a fixture tree with a direct reader is caught" {
  local t="$BATS_TEST_TMPDIR/tree"
  mkdir -p "$t/platform/scripts" && cd "$t" && git init -q .
  echo 'curl -s http://localhost:3360/v1/principles/principles' >platform/scripts/reader.sh
  echo 'curl -s http://localhost:3393/principles/principles' >platform/scripts/fine.sh
  git add -A
  run direct_readers "$t"
  [ "$output" = "platform/scripts/reader.sh" ]
}
