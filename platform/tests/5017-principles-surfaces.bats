#!/usr/bin/env bats
# @test-type: integration — reads the generated surfaces; serves the card's athena-make build on a free port
# @domain: principles
# @card: 5017 · owner: kade
# What Jeff sees: principles' MCP tools, OpenAPI document and page come from one place,
# principles-api's generated surfaces/, and name its routes; athena-make no longer
# generates or serves any principles surface.
#
# AC4: the three surfaces are principles-api's surfaces/ files, each naming
# /principles/principles; athena-make's principles OpenAPI answers 404.
# NEGATIVE: a surface naming athena-make's /v1 path, and a door still serving the
# principles OpenAPI, each fail the checks.

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
SURF="$REPO/shaping/domains/principles/api/surfaces"

# The surface check: all three files exist, each names /principles/principles, none
# names athena-make's /v1/principles path or its port.
surfaces_ok() {
  local d="$1" f
  for f in Principle.mcp.json Principle.openapi.json Principle.html; do
    [ -s "$d/$f" ] || { echo "missing $f"; return 1; }
    grep -q '/principles/principles' "$d/$f" || { echo "$f names no principles-api route"; return 1; }
    ! grep -qE '/v1/principles|:3360' "$d/$f" || { echo "$f names athena-make"; return 1; }
  done
  python3 - "$d" <<'PY' || return 1
import json, sys
d = sys.argv[1]
paths = set(json.load(open(f"{d}/Principle.openapi.json"))["paths"])
assert paths == {"/principles/principles", "/principles/principles/{name}"}, paths
routes = {t["route"].split(" ", 1)[1] for t in json.load(open(f"{d}/Principle.mcp.json"))["tools"]}
assert routes <= {"/principles/principles", "/principles/principles/:name"}, routes
PY
}

card_make() {
  local c="${ATHENA_MAKE_BIN:-${CARGO_TARGET_DIR:-$REPO/target}/release/athena-make}"
  [ -x "$c" ] || (cd "$REPO" && cargo build --release --quiet -p athena-make) >&2 || return 1
  [ -x "$c" ] && echo "$c"
}

free_port() { python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])'; }
code() { curl -s -o /dev/null -w '%{http_code}' --max-time 20 "$1"; }

# athena-make's principles surfaces are gone: its OpenAPI for principles, both spellings, 404
make_surfaces_gone() {
  [ "$(code "$1/v1/domains/domains")" = 200 ] &&
  [ "$(code "$1/principles/principles/openapi.json")" = 404 ] &&
  [ "$(code "$1/v1/principles/principles/openapi.json")" = 404 ]
}

teardown() { [ -n "${PID:-}" ] && { kill "$PID" 2>/dev/null; wait "$PID" 2>/dev/null; }; true; }

@test "AC4: principles' MCP binding, OpenAPI and page are principles-api's surfaces/ files" {
  run surfaces_ok "$SURF"
  [ "$status" -eq 0 ] || { echo "$output"; false; }
}

@test "AC4 NEGATIVE: a surface naming athena-make's /v1 path fails" {
  local d="$BATS_TEST_TMPDIR/surfaces"
  cp -R "$SURF" "$d"
  sed -i '' 's#"/principles/principles"#"/v1/principles/principles"#' "$d/Principle.openapi.json"
  run surfaces_ok "$d"
  [ "$status" -ne 0 ]
}

@test "AC4: the card's athena-make serves no principles OpenAPI" {
  local bin port i
  bin="$(card_make)" || { echo "athena-make not built"; false; }
  port="$(free_port)"
  "$bin" serve --port "$port" >"$BATS_TEST_TMPDIR/make.log" 2>&1 &
  PID=$!
  for i in $(seq 1 120); do [ "$(code "http://127.0.0.1:$port/v1/domains/domains")" = 200 ] && break; sleep 1; done
  make_surfaces_gone "http://127.0.0.1:$port" || { tail -5 "$BATS_TEST_TMPDIR/make.log"; false; }
}

@test "AC4 NEGATIVE: a door still serving the principles OpenAPI fails" {
  local port dir i
  port="$(free_port)"; dir="$BATS_TEST_TMPDIR/door"
  mkdir -p "$dir/v1/domains" "$dir/principles/principles"
  echo '{}' >"$dir/v1/domains/domains"; echo '{}' >"$dir/principles/principles/openapi.json"
  python3 -m http.server "$port" --bind 127.0.0.1 --directory "$dir" >/dev/null 2>&1 &
  PID=$!
  for i in $(seq 1 40); do [ "$(code "http://127.0.0.1:$port/v1/domains/domains")" = 200 ] && break; sleep 0.25; done
  ! make_surfaces_gone "http://127.0.0.1:$port" || return 1
}
