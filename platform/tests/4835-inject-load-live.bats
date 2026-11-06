#!/usr/bin/env bats
# @test-type: integration:perf — times 40 prompts at once through the running daemon
# @domain: memory — per-prompt context, the domain context-inject-latency-spec guards (#4334)
# @needs: machine — times the running chorus-hooks daemon (#4539: card runs leave it to the nightly)
# #4835 AC1. A card's own run would time the daemon from BEFORE its change, so this
# runs on main after the land, where the number is the change's.

# per-lookup max ms and how often each was the slowest, over the inject events since $1 (ns)
slow_lookups() {
  local end; end=$(python3 -c 'import time; print(int(time.time()*1e9)+2*10**9)')
  curl -sf -m 10 -G "${LOKI_URL:-http://localhost:3102}/loki/api/v1/query_range" \
    --data-urlencode 'query={job="platform-chorus"} |= "context.inject.response"' \
    --data-urlencode "start=$1" --data-urlencode "end=$end" --data-urlencode "limit=500" \
  | python3 -c '
import json, sys, collections
d = json.load(sys.stdin); mx = collections.Counter(); slow = collections.Counter(); n = 0
for s in d["data"]["result"]:
    for _, line in s["values"]:
        try: e = json.loads(line)
        except ValueError: continue
        n += 1; slow[e.get("slowest", "?")] += 1
        for k, v in (e.get("lookups_ms") or {}).items(): mx[k] = max(mx[k], v)
if not n: sys.exit(1)
print(f"lookups over {n} injects (max ms, times slowest):", " ".join(f"{k}={mx[k]}/{slow[k]}" for k, _ in mx.most_common()))
'
}

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
}

# #4835 AC1 — 40 prompts at once through the running daemon, p95 under 2 s.
# Needs the daemon and a quiet box: at or above the core count the number measures
# the box, so it reports UNMEASURED (skip), never a pass (#4071's rule).
@test "#4839 AC2/AC3: 40 concurrent request-shaped prompts, p95 under 2s at the box's real load" {
  sock="${CHORUS_HOOKS_SOCK:-$HOME/.chorus/run/chorus-hooks.sock}"
  # a role account reaches the daemon through its agent socket dir
  [ -S "$sock" ] || sock="${CHORUS_HOOKS_RUN_DIR:-$HOME/.chorus/run}/chorus-hooks.sock"
  [ -S "$sock" ] || skip "UNMEASURED: no hooks daemon socket on this box"
  # #4839 AC3 — measured at the box's real load (it sat near 65 all day, so a
  # quiet-box gate never ran: hollow, Silas 10-31 12:28). The load is printed with the number.
  load=$(sysctl -n vm.loadavg | awk '{print $2}')
  out="$BATS_TEST_TMPDIR/times"
  t0=$(python3 -c 'import time; print(int(time.time()*1e9))')
  for i in $(seq 1 40); do
    ( p=$(printf '{"hook_event_name":"UserPromptSubmit","prompt":"how do we get the crawler in sync, 4839 load probe %s","session_id":"bats-4835-%s","cwd":"%s"}' "$i" "$i" "$ROOT")
      curl -s -o /dev/null -m 15 -w '%{time_total}\n' --unix-socket "$sock" -H 'Content-Type: application/json' --data "$p" http://localhost/user-prompt-submit ) &
  done > "$out"
  wait
  n=$(wc -l < "$out" | tr -d ' ')
  [ "$n" -eq 40 ] || return 1
  p95=$(sort -n "$out" | sed -n '38p')
  echo "p95=${p95}s at load ${load}"
  # #4915 (Silas) — a red must say which lookup is slow. The daemon's own
  # context.inject.response events carry lookups_ms and `slowest`; read the burst's
  # window from Loki (the injects of other sessions in that window take the same path).
  slow_lookups "$t0" || echo "lookups: UNMEASURED (Loki did not answer for the burst window)"
  awk -v p="$p95" 'BEGIN { exit !(p < 2.0) }' || return 1
}
