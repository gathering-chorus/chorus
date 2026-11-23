#!/usr/bin/env bats
# @test-type: smoke — live: after the land, a real demo runs its three reviews together
# @domain: pipelines
# @needs: machine — prod after the land and the 3 StepSkill rows
# @card: #4953 · owner: wren
# What Jeff sees: the demo's review phase lasts as long as its slowest review, not the sum.
# Read from prod dagu's API: the newest demo run whose reviews all finished. Red before the land
# (11-11 15:43 run: quality 15:43:06, arch 15:44:56, code 15:45:47 — in turn).
DAGU="${DAGU_URL:-http://localhost:8526}"

# prints "spread_s phase_s slowest_s" for the newest finished demo run, or why it cannot
measure() {
  python3 - "$DAGU" <<'PY'
import json, os, sys, time, urllib.request
from datetime import datetime
base = sys.argv[1]
# DAGU_FIXTURE: one JSON file {path: answer} stands in for dagu (the negative proof); its keys
# carry no fromDate, the one value that changes every run
fx = os.environ.get("DAGU_FIXTURE")
get = (lambda p: json.load(open(fx))[p.split("&fromDate=")[0]]) if fx else (lambda p: json.load(urllib.request.urlopen(base + p, timeout=10)))
reviews = ["skill-werk-demo-gate-quality", "skill-werk-demo-gate-arch", "skill-werk-demo-gate-code"]
# #4925 round 6: dagu lists only TODAY's runs unless told a fromDate. At 03:00 no demo has run
# yet today, so the nightly read "no finished demo run" every night (11-23: 0 of 0; with a
# 7-day window, 20). The window is the last 14 days.
since = int(time.time()) - 14 * 86400
for run in get(f"/api/v1/dag-runs?name=demo&limit=20&fromDate={since}")["dagRuns"]:
    d = get(f"/api/v1/dag-runs/demo/{run['dagRunId']}")
    nodes = {n["step"]["name"]: n for n in d.get("dagRunDetails", d)["nodes"]}
    t = lambda s: datetime.fromisoformat(s) if s and s != "-" else None
    rs = [nodes.get(r) for r in reviews]
    if not all(n and t(n.get("startedAt")) and t(n.get("finishedAt")) for n in rs):
        continue
    starts = [t(n["startedAt"]) for n in rs]; ends = [t(n["finishedAt"]) for n in rs]
    spread = (max(starts) - min(starts)).total_seconds()
    phase = (max(ends) - min(starts)).total_seconds()
    slowest = max((e - s).total_seconds() for s, e in zip(starts, ends))
    print(f"{int(spread)} {int(phase)} {int(slowest)} {run['dagRunId']}")
    sys.exit(0)
print("no finished demo run with all three reviews"); sys.exit(1)
PY
}

# the AC2 verdict on measure's output: silent when the reviews ran together, else why not
verdict() {
  read -r spread phase slowest id <<<"$1"
  [ "$spread" -le 30 ] || echo "run $id: reviews started ${spread}s apart (in turn)"
  [ "$phase" -le $((slowest + 60)) ] || echo "run $id: review phase ${phase}s, slowest review ${slowest}s"
  return 0
}

@test "AC2: the newest real demo starts its three reviews within 30s, and its review phase is no longer than the slowest review plus 1 min" {
  run measure
  [ "$status" -eq 0 ] || { echo "unmeasured: $output"; return 1; }
  why="$(verdict "$output")"
  [ -z "$why" ] || { echo "$why"; return 1; }
}

@test "AC2 negative proof: the 11-11 15:43 demo, reviews in turn, is red" {
  f="$BATS_TEST_TMPDIR/dagu.json"
  n() { printf '{"step":{"name":"%s"},"startedAt":"%s","finishedAt":"%s"}' "$1" "$2" "$3"; }
  printf '{"/api/v1/dag-runs?name=demo&limit=20":{"dagRuns":[{"dagRunId":"r1"}]},"/api/v1/dag-runs/demo/r1":{"nodes":[%s,%s,%s]}}' \
    "$(n skill-werk-demo-gate-quality 2026-11-11T15:43:06-05:00 2026-11-11T15:44:56-05:00)" \
    "$(n skill-werk-demo-gate-arch 2026-11-11T15:44:56-05:00 2026-11-11T15:45:47-05:00)" \
    "$(n skill-werk-demo-gate-code 2026-11-11T15:45:47-05:00 2026-11-11T15:48:50-05:00)" > "$f"
  DAGU_FIXTURE="$f" run measure
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  why="$(verdict "$output")"
  [[ "$why" == *"started 161s apart"* && "$why" == *"review phase 344s"* ]] || { echo "serial run not caught: $output / $why"; return 1; }
}

@test "#4925 round 6 NEGATIVE: the run list asks dagu for a window, so a day with no demo yet still measures" {
  # a stub dagu that, like the real one, lists only today's runs unless fromDate is given
  T="$BATS_TEST_TMPDIR"; port="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])')"
  cat > "$T/dagu.py" <<'PY'
import http.server, json, sys
n = lambda s, a, b: {"step": {"name": s}, "startedAt": a, "finishedAt": b}
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path.startswith("/api/v1/dag-runs?"):
            body = {"dagRuns": [{"dagRunId": "y1"}] if "fromDate=" in self.path else []}
        else:
            body = {"nodes": [n("skill-werk-demo-gate-quality", "2026-11-22T10:00:00-05:00", "2026-11-22T10:02:00-05:00"),
                              n("skill-werk-demo-gate-arch", "2026-11-22T10:00:05-05:00", "2026-11-22T10:01:00-05:00"),
                              n("skill-werk-demo-gate-code", "2026-11-22T10:00:09-05:00", "2026-11-22T10:03:00-05:00")]}
        b = json.dumps(body).encode(); self.send_response(200); self.end_headers(); self.wfile.write(b)
    def log_message(self, *a): pass
http.server.HTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
PY
  python3 "$T/dagu.py" "$port" & pid=$!
  for _ in 1 2 3 4 5 6 7 8 9 10; do curl -s "http://127.0.0.1:$port/x" >/dev/null 2>&1 && break; sleep 0.2; done
  DAGU_URL="http://127.0.0.1:$port"; DAGU="$DAGU_URL" run measure
  kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null || true
  [ "$status" -eq 0 ] || { echo "yesterday's demo was not found: $output"; return 1; }
  echo "$output" | grep -q " y1$" || { echo "$output"; return 1; }
}

@test "AC1 model-first (Kade 10:11, row 84): chorus-make graph demo, from the prod rows, makes exactly demo.yaml" {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  command -v chorus-make >/dev/null || { echo "unmeasured: no chorus-make on PATH"; return 1; }
  run chorus-make graph demo
  [ "$status" -eq 0 ] || { echo "$output" | tail -3; return 1; }
  diff <(printf '%s\n' "$output") "$ROOT/platform/pipelines/demo.yaml" || { echo "the prod rows and demo.yaml disagree"; return 1; }
}
