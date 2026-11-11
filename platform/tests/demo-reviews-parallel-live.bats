#!/usr/bin/env bats
# @test-type: smoke — live: after the land, a real demo runs its three reviews together
# @domain: pipelines
# @card: #4953 · owner: wren
# What Jeff sees: the demo's review phase lasts as long as its slowest review, not the sum.
# Read from prod dagu's API: the newest demo run whose reviews all finished. Red before the land
# (11-11 15:43 run: quality 15:43:06, arch 15:44:56, code 15:45:47 — in turn).
DAGU="${DAGU_URL:-http://localhost:8526}"

# prints "spread_s phase_s slowest_s" for the newest finished demo run, or why it cannot
measure() {
  python3 - "$DAGU" <<'PY'
import json, sys, urllib.request
from datetime import datetime
base = sys.argv[1]
get = lambda p: json.load(urllib.request.urlopen(base + p, timeout=10))
reviews = ["skill-werk-demo-gate-quality", "skill-werk-demo-gate-arch", "skill-werk-demo-gate-code"]
for run in get("/api/v1/dag-runs?name=demo&limit=20")["dagRuns"]:
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

@test "AC2: the newest real demo starts its three reviews within 30s, and its review phase is no longer than the slowest review plus 1 min" {
  run measure
  [ "$status" -eq 0 ] || { echo "unmeasured: $output"; return 1; }
  read -r spread phase slowest id <<<"$output"
  [ "$spread" -le 30 ] || { echo "run $id: reviews started ${spread}s apart (in turn)"; return 1; }
  [ "$phase" -le $((slowest + 60)) ] || { echo "run $id: review phase ${phase}s, slowest review ${slowest}s"; return 1; }
}

@test "AC1 model-first (Kade 10:11, row 84): chorus-make graph demo, from the prod rows, makes exactly demo.yaml" {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  command -v chorus-make >/dev/null || { echo "unmeasured: no chorus-make on PATH"; return 1; }
  run chorus-make graph demo
  [ "$status" -eq 0 ] || { echo "$output" | tail -3; return 1; }
  diff <(printf '%s\n' "$output") "$ROOT/platform/pipelines/demo.yaml" || { echo "the prod rows and demo.yaml disagree"; return 1; }
}
