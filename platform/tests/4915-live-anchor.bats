#!/usr/bin/env bats
# @test-type: unit — the live-anchor helper and the anchored tests against a stub Loki and a dead pulse; no live services
# @domain: tests
# @card: #4915 · owner: kade
# What Jeff sees: the 04:00 nightly stops reading red only because nobody was active. A live test's
# window ends at the newest event at the SOURCE (Loki), never at now and never at the surface under
# test; no source event in 24h is UNMEASURED (exit 2 / skip), never green. And a live READ of pulse
# takes PULSE_LIVE_URL, because werk-test cages PULSE_URL to a dead port (Wren 15:23).
# Covers: platform/tests/lib/live_anchor.py, platform/tests/4807-inject-unanswered-prod.test.sh,
#         platform/tests/4766-streams-tab-live.bats, platform/tests/4991-streams-every-role-live.bats,
#         platform/tests/alert-loudest-three-are-rows-live.bats, platform/tests/4895-every-role-reachable.bats,
#         platform/tests/4985-quiet-role-keeps-tile-live.bats

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
LIB="$ROOT/platform/tests/lib/live_anchor.py"

# a stub Loki: query_range answers the lines in $T/lines.json (newest-first values); query answers
# the instant result in $T/instant.json and records the time= it was asked at
stub_loki() {
  T="$BATS_TEST_TMPDIR"; port="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')"
  cat > "$T/loki.py" <<'PY'
import http.server, json, sys, urllib.parse
port, d = int(sys.argv[1]), sys.argv[2]
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        u = urllib.parse.urlparse(self.path); q = urllib.parse.parse_qs(u.query)
        if u.path.endswith("/query_range"):
            body = {"data": {"result": json.load(open(d + "/lines.json"))}}
        else:
            open(d + "/asked_at", "w").write(q.get("time", [""])[0])
            body = {"data": {"result": json.load(open(d + "/instant.json"))}}
        b = json.dumps(body).encode(); self.send_response(200); self.end_headers(); self.wfile.write(b)
http.server.HTTPServer(("127.0.0.1", port), H).serve_forever()
PY
  echo '[]' > "$T/lines.json"; echo '[]' > "$T/instant.json"
  python3 "$T/loki.py" "$port" "$T" >/dev/null 2>&1 & LOKI_PID=$!
  for i in $(seq 1 40); do curl -s "http://127.0.0.1:$port/loki/api/v1/query" >/dev/null && break; sleep 0.1; done
  export LOKI_URL="http://127.0.0.1:$port"
}
teardown() { [ -n "${LOKI_PID:-}" ] && { kill "$LOKI_PID" 2>/dev/null; wait "$LOKI_PID" 2>/dev/null; }; true; }

@test "#4915 round 5: the anchor is the newest source line" {
  stub_loki
  echo '[{"stream":{},"values":[["1795255800000000000","new"],["1795252200000000000","old"]]}]' > "$T/lines.json"
  run python3 "$LIB" newest '{job="x"}'
  [ "$status" -eq 0 ] && [ "$output" = $'1795255800.000\tnew' ] || { echo "$output"; return 1; }
}

# NEGATIVE PROOF: nothing at the source is UNMEASURED (2), never green; a dead source is never green
@test "#4915 round 5 NEGATIVE: no source event in 24h, or no source at all, is UNMEASURED (exit 2)" {
  stub_loki
  run python3 "$LIB" newest '{job="x"}'
  [ "$status" -eq 2 ] || return 1
  LOKI_URL=http://127.0.0.1:9 run python3 "$LIB" newest '{job="x"}'
  [ "$status" -eq 2 ] || return 1
}

@test "#4915 round 5: 4807 is measured at the newest prompt, not at now; a quiet night is not red" {
  stub_loki
  echo '[{"stream":{},"values":[["1795255800000000000","{\"event\":\"context.inject.request\"}"]]}]' > "$T/lines.json"
  echo '[{"metric":{"role":"wren","event":"context.inject.request"},"value":[0,"3"]},{"metric":{"role":"wren","event":"context.inject.response"},"value":[0,"3"]}]' > "$T/instant.json"
  run bash "$ROOT/platform/tests/4807-inject-unanswered-prod.test.sh"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(cat "$T/asked_at")" = "1795255860" ]   # the newest request plus a minute, never now
}

@test "#4915 round 5 NEGATIVE: 4807 with lost prompts at the anchor is red; with no prompt in 24h it is UNMEASURED" {
  stub_loki
  echo '[{"stream":{},"values":[["1795255800000000000","r"]]}]' > "$T/lines.json"
  echo '[{"metric":{"role":"wren","event":"context.inject.request"},"value":[0,"3"]},{"metric":{"role":"wren","event":"context.inject.response"},"value":[0,"1"]}]' > "$T/instant.json"
  run bash "$ROOT/platform/tests/4807-inject-unanswered-prod.test.sh"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo '[]' > "$T/lines.json"
  run bash "$ROOT/platform/tests/4807-inject-unanswered-prod.test.sh"
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
}

# NEGATIVE PROOF (Wren 15:23): a live pulse READ takes PULSE_LIVE_URL; pointed at a dead port it is red,
# never a skip and never green — and the werk-test cage on PULSE_URL no longer touches it
@test "#4915 round 5 NEGATIVE: 4895 with its live pulse at a dead port reads red, not skipped" {
  run env PULSE_LIVE_URL=http://127.0.0.1:9 bats -f "every agent role resolves" "$ROOT/platform/tests/4895-every-role-reachable.bats"
  [[ "$output" == *"not ok 1"* ]] || { echo "$output"; return 1; }
  [[ "$output" != *"# skip"* ]] || { echo "$output"; return 1; }
}

# the test files that read pulse through the caged PULSE_URL's live default
# (this guard file is left out: its NEGATIVE case writes the very pattern it looks for, #3725)
caged_reads() { grep -lE '\$\{PULSE_URL:-https?://(localhost|127\.0\.0\.1):3475' "$1"/*.bats "$1"/*.sh 2>/dev/null | grep -v '/4915-live-anchor\.bats$' || true; }

# #4925 round 6: nudge-health.bats left this list in #4925 round 1, when its live cases moved
# to 4895 and it became hermetic (a curl stub, no pulse read); the list held a file that no
# longer reads pulse, and the check went red on it. The live readers are named; every other
# test file is swept for the caged default, so a new live reader cannot slip past the list.
@test "#4915 round 5: no live suite reads pulse through the caged PULSE_URL" {
  local f
  for f in 4895-every-role-reachable.bats 4985-quiet-role-keeps-tile-live.bats; do
    [ -f "$ROOT/platform/tests/$f" ] || { echo "missing $f: the check would pass on nothing"; return 1; }
    grep -q 'PULSE_LIVE_URL' "$ROOT/platform/tests/$f" || { echo "$f does not read PULSE_LIVE_URL"; return 1; }
  done
  out="$(caged_reads "$ROOT/platform/tests")"
  [ -z "$out" ] || { echo "reads pulse through the caged PULSE_URL: $out"; return 1; }
}

@test "#4925 round 6 NEGATIVE: a test that reads pulse through the caged PULSE_URL is found" {
  d="$BATS_TEST_TMPDIR/tests"; mkdir -p "$d"
  echo 'curl -sf "${PULSE_URL:-http://localhost:3475}/api/where/wren"' > "$d/new-live.bats"
  echo 'curl -sf "${PULSE_LIVE_URL:-http://localhost:3475}/api/where/wren"' > "$d/good-live.bats"
  echo 'PULSE_URL=http://127.0.0.1:9 run thing' > "$d/caged-on-purpose.bats"
  test "$(caged_reads "$d")" = "$d/new-live.bats"
}
