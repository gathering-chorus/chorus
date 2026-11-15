#!/usr/bin/env bats
# @test-type: integration — boots a scratch Loki (the agent's own binary) on free ports with an empty store; never touches :3102
# @domain: logs
# @card: #4983 · owner: silas
# Checks platform/config/loki/loki.yaml under platform/scripts/loki-replay.
#
# #4983 AC3: under a replay of the 2026-11-15 15:32 load (17 reads over 24h, one three-day read and
# 10 instant reads, all at once) Loki refuses 0 queries and the slowest read answers; the same
# replay against the old limits refuses at least one. Wren 18:36: the two scratch Lokis run one
# after the other, never together (parallel load crashed Library on 10-31), with compactor and
# retention off. The refusals are Loki's queue arithmetic (sub-queries per read against the queue),
# so an empty store reproduces them; no copy of the 25 GB store is made.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  PLIST="$ROOT/platform/scripts/launchagents-canonical/com.gathering.loki.plist"
  LOKI_BIN="${LOKI_BIN:-$(plutil -extract ProgramArguments.0 raw -o - "$PLIST")}"
  [ -x "$LOKI_BIN" ] || { echo "no loki binary at $LOKI_BIN"; return 1; }
}

teardown() {
  [ -n "${PID:-}" ] && kill "$PID" 2>/dev/null; wait "$PID" 2>/dev/null || true
}

free_port() { python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1])'; }

# the repo config, moved to a scratch home and free ports; compactor and retention off.
# $1 = new|old: old puts back the four values #4983 changed (queue 100, split 30m, sharding on, no cache)
scratch_config() {
  local home="$BATS_TEST_TMPDIR/$1" http="$2" grpc="$3"
  mkdir -p "$home"
  python3 - "$ROOT/platform/config/loki/loki.yaml" "$home" "$http" "$grpc" "$1" > "$home/loki.yaml" <<'PY'
import re, sys
src, home, http, grpc, which = sys.argv[1:]
s = open(src).read()
s = re.sub(r"/\S*/data/loki", home, s)
s = re.sub(r"http_listen_port: \d+", f"http_listen_port: {http}", s)
s = re.sub(r"grpc_listen_port: \d+", f"grpc_listen_port: {grpc}", s)
s = s.replace("retention_enabled: true", "retention_enabled: false")
s = s.replace("retention_deletes_enabled: true", "retention_deletes_enabled: false")
if which == "old":
    for new, old in (("max_outstanding_requests_per_tenant: 1024", "max_outstanding_requests_per_tenant: 100"),
                     ("split_queries_by_interval: 2h", "split_queries_by_interval: 30m"),
                     ("parallelise_shardable_queries: false", "parallelise_shardable_queries: true"),
                     ("cache_results: true", "cache_results: false")):
        assert new in s, new
        s = s.replace(new, old)
print(s)
PY
  grep -q "$home" "$home/loki.yaml" && ! grep -q "shared-observability/data" "$home/loki.yaml" || { echo "scratch config still names the live store"; return 1; }
}

# boot one scratch Loki, push a little recent data, replay, stop it; prints the replay line
replay_on() {
  local http grpc; http=$(free_port); grpc=$(free_port)
  scratch_config "$1" "$http" "$grpc" || return 1
  "$LOKI_BIN" -config.file="$BATS_TEST_TMPDIR/$1/loki.yaml" > "$BATS_TEST_TMPDIR/$1/loki.log" 2>&1 &
  PID=$!
  for _ in $(seq 1 90); do [ "$(curl -s --max-time 1 "http://127.0.0.1:$http/ready")" = ready ] && break; sleep 1; done
  [ "$(curl -s --max-time 1 "http://127.0.0.1:$http/ready")" = ready ] || { tail -5 "$BATS_TEST_TMPDIR/$1/loki.log"; return 1; }
  python3 - "$http" <<'PY'
import json, sys, time, urllib.request
now = time.time_ns()
values = [[str(now - i * 60 * 10**9), '{"event":"card.moved","n":%d}' % i] for i in range(120)]
body = json.dumps({"streams": [{"stream": {"job": "replay"}, "values": values}]}).encode()
urllib.request.urlopen(urllib.request.Request(f"http://127.0.0.1:{sys.argv[1]}/loki/api/v1/push", body,
                                              {"Content-Type": "application/json"}), timeout=10)
PY
  python3 "$ROOT/platform/scripts/loki-replay" --url "http://127.0.0.1:$http" --timeout 120
  kill "$PID"; wait "$PID" 2>/dev/null || true; PID=
}

field() { printf '%s\n' "$1" | tr ' ' '\n' | sed -n "s/^$2=//p"; }

@test "the 15:32 replay: the new limits refuse none and answer every read; the old limits refuse" {
  new="$(replay_on new)" || { echo "$new"; return 1; }
  echo "new: $new"
  [ "$(field "$new" refused)" = 0 ] && [ "$(field "$new" failed)" = 0 ] && [ "$(field "$new" answered)" = 28 ] || return 1
  # the slowest read answers well inside the replay's 120 s wait: a queue that never refuses but
  # never answers cannot pass
  python3 -c 'import sys; sys.exit(0 if float(sys.argv[1]) < 30 else 1)' "$(field "$new" slowest)" || return 1
  # NEGATIVE PROOF: the same replay, run after the first Loki has stopped, against the old limits
  old="$(replay_on old)" || { echo "$old"; return 1; }
  echo "old: $old"
  [ "$(field "$old" refused)" -ge 1 ] || return 1
}
