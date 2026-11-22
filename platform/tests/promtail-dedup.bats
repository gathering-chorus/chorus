#!/usr/bin/env bats
# @test-type: integration — promtail syntax/route checks + live Loki :3102 stream dedup query
# @domain: logs — the product domain this suite guards (#4334)
load test_helper
# promtail-dedup.bats — verify no duplicate Promtail streams in Loki (#1986, #4925)
# What Jeff sees: Loki queries return one stream per log file, not duplicates.

REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
PROMTAIL_CONFIG="$REPO_ROOT/platform/config/promtail/promtail-native.yaml"
PROMTAIL_BIN="${PROMTAIL_BIN:-$(plutil -extract ProgramArguments.0 raw "$REPO_ROOT/platform/scripts/launchagents-canonical/com.gathering.promtail.plist" 2>/dev/null)}"
PROMTAIL_HOME="${PROMTAIL_HOME:-$(dirname "$(dirname "$PROMTAIL_BIN")")}"
LOKI_URL="${LOKI_URL:-http://localhost:3102}"
ROUTES="${BATS_TEST_DIRNAME}/fixtures/4336/promtail-routes.rb"
ANCHOR="${BATS_TEST_DIRNAME}/lib/live_anchor.py"
LOGS="${PROMTAIL_HOME}/Library/Logs"

routes() { ruby "$ROUTES" "$1" "$2" | paste -sd, -; }

loki_count_at() {
  local at="$1" logql="$2"
  curl -sfG --max-time 10 "${LOKI_URL}/loki/api/v1/query" \
    --data-urlencode "time=${at}" \
    --data-urlencode "query=sum(count_over_time(${logql}[1h]))" \
    | python3 -c 'import json,sys; r=json.load(sys.stdin)["data"]["result"]; print(int(float(r[0]["value"][1])) if r else 0)'
}

check_loki_dedup() {
  local target="$LOGS/Chorus/chorus-api.log" line at own dup
  line=$(LOKI_URL="$LOKI_URL" python3 "$ANCHOR" newest "{job=\"chorus-api\",filename=\"${target}\"}") || {
    echo "Loki anchor failed for {job=\"chorus-api\",filename=\"${target}\"}"
    return 1
  }
  at=$(python3 -c "import sys; print(int(float(sys.argv[1])) + 1)" "${line%%$'\t'*}")
  own=$(loki_count_at "$at" "{job=\"chorus-api\",filename=\"${target}\"}") || { echo "own query failed"; return 1; }
  dup=$(loki_count_at "$at" "{job=\"daemon-logs\",filename=\"${target}\"}") || { echo "dup query failed"; return 1; }
  echo "$own in job=chorus-api, $dup in job=daemon-logs (1h ending $at)"
  [ "$own" -gt 0 ] || { echo "vacuous window: 0 lines in job=chorus-api"; return 1; }
  [ "$dup" -eq 0 ] || { echo "duplicate scrape: $dup lines in job=daemon-logs"; return 1; }
}

@test "Promtail config is valid" {
  [ -f "$PROMTAIL_CONFIG" ]
  [ -x "$PROMTAIL_BIN" ]
  run "$PROMTAIL_BIN" -config.file="$PROMTAIL_CONFIG" -check-syntax
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "Valid config"
}

@test "chorus-api.log is scraped by its own job only, never also by the daemon-logs glob" {
  [ -f "$PROMTAIL_CONFIG" ]
  [ "$(routes "$PROMTAIL_CONFIG" "$LOGS/Chorus/some-other-daemon.log")" = "daemon-logs" ]
  [ "$(routes "$PROMTAIL_CONFIG" "$LOGS/Chorus/chorus-api.log")" = "chorus-api" ]
}

@test "cloudflared.log is scraped by its own job only, never also by the gathering daemon-logs glob" {
  [ -f "$PROMTAIL_CONFIG" ]
  [ "$(routes "$PROMTAIL_CONFIG" "$LOGS/Gathering/some-other-daemon.log")" = "daemon-logs" ]
  [ "$(routes "$PROMTAIL_CONFIG" "$LOGS/Gathering/cloudflared.log")" = "cloudflared" ]
}

@test "NEGATIVE PROOF: a config whose glob job lost its exclude reports the duplicate" {
  [ -f "$PROMTAIL_CONFIG" ]
  fixture="$BATS_TEST_TMPDIR/promtail.yaml"
  sed -E '/__path_exclude__:.*Logs\/(Chorus|Gathering)\/\{/d' "$PROMTAIL_CONFIG" > "$fixture"
  [ "$(routes "$fixture" "$LOGS/Chorus/chorus-api.log")" = "chorus-api,daemon-logs" ]
  [ "$(routes "$fixture" "$LOGS/Gathering/cloudflared.log")" = "cloudflared,daemon-logs" ]
}

@test "Loki ingests chorus-api.log under job=chorus-api only, never daemon-logs" {
  run check_loki_dedup
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "NEGATIVE PROOF: live Loki dedup check fails on daemon-logs duplicates, zero own lines, or unreachable Loki" {
  LOKI_URL="http://127.0.0.1:9" run check_loki_dedup
  [ "$status" -ne 0 ]

  cat > "$BATS_TEST_TMPDIR/stub.py" <<'PY'
import http.server, json, sys, urllib.parse
port_file, mode_file = sys.argv[1], sys.argv[2]
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        mode = open(mode_file).read().strip()
        q = urllib.parse.parse_qs(urllib.parse.urlparse(self.path).query).get("query", [""])[0]
        if "query_range" in self.path:
            res = [{"stream": {}, "values": [["1795350000000000000", "ok"]]}]
        else:
            val = ("2" if mode == "dup" else "0") if "daemon-logs" in q else ("0" if mode == "zero_own" else "5")
            res = [{"metric": {}, "value": [1795350001, val]}] if val != "0" else []
        body = json.dumps({"status": "success", "data": {"result": res}}).encode()
        self.send_response(200); self.send_header("Content-Type", "application/json"); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)
srv = http.server.HTTPServer(("127.0.0.1", 0), H)
open(port_file, "w").write(str(srv.server_port))
srv.serve_forever()
PY
  ( python3 "$BATS_TEST_TMPDIR/stub.py" "$BATS_TEST_TMPDIR/port" "$BATS_TEST_TMPDIR/mode" >/dev/null 2>&1 & echo $! > "$BATS_TEST_TMPDIR/pid" )
  srv_pid="$(cat "$BATS_TEST_TMPDIR/pid")"
  for _ in 1 2 3 4 5 6 7 8 9 10; do [ -s "$BATS_TEST_TMPDIR/port" ] && break; sleep 0.1; done
  stub="http://127.0.0.1:$(cat "$BATS_TEST_TMPDIR/port")"

  echo "dup" > "$BATS_TEST_TMPDIR/mode"
  LOKI_URL="$stub" run check_loki_dedup
  [ "$status" -ne 0 ]
  echo "$output" | grep -q "duplicate scrape" || { kill "$srv_pid" 2>/dev/null || true; echo "$output"; return 1; }

  echo "zero_own" > "$BATS_TEST_TMPDIR/mode"
  LOKI_URL="$stub" run check_loki_dedup
  [ "$status" -ne 0 ]
  echo "$output" | grep -q "vacuous window" || { kill "$srv_pid" 2>/dev/null || true; echo "$output"; return 1; }

  echo "ok" > "$BATS_TEST_TMPDIR/mode"
  LOKI_URL="$stub" run check_loki_dedup
  kill "$srv_pid" 2>/dev/null || true
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}
