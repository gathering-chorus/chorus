#!/usr/bin/env bats
# @test-type: unit — process-memory-exporter.sh sub-50MB CPU capture, node/werk/service/role disaggregation, daily load peak watermark, and #3734 negative proofs
# @domain: metrics
# @card: #4992
# @owner: borg-queen
#
# #4992 AC3 — process-memory-exporter.sh captures high-CPU / sub-50MB processes (clang++, rustc,
# cargo, mds) without a 50 MB RSS blind spot, emits direct host load metrics alongside process
# buckets, and disaggregates node/service/werk/role processes (including promtail and orphaned werk
# servers) so :9090 / Grafana show what is driving Library CPU and RSS, with a negative proof (#3734).

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
EXP="$ROOT/proving/scripts/process-memory-exporter.sh"
FIX_PS="$ROOT/platform/tests/fixtures/4992/ps-load-spike.txt"
FIX_BEFORE="$ROOT/platform/tests/fixtures/4992/process-memory-before.prom"

setup() { T="$BATS_TEST_TMPDIR"; }

# Shared validator asserting every AC3 contract on a generated .prom file.
# Must PASS on #4992 output and FAIL on pre-#4992 output (FIX_BEFORE).
assert_ac3_prom() {
  python3 - "$1" <<'PY'
import re, sys

lines = [l.rstrip("\n") for l in open(sys.argv[1]) if l.strip()]
metrics = {}
for l in lines:
    if l.startswith("#"):
        assert re.match(r"^# (HELP|TYPE) [a-zA-Z_:][a-zA-Z0-9_:]* ", l), f"bad comment: {l}"
        continue
    m = re.match(r'^([a-zA-Z_:][a-zA-Z0-9_:]*)(?:\{([^}]*)\})?\s+(-?[0-9]+(?:\.[0-9]+)?)$', l)
    assert m, f"invalid exposition line: {l}"
    name, raw_labels, val = m.group(1), m.group(2) or "", float(m.group(3))
    labels = dict(re.findall(r'([a-zA-Z_][a-zA-Z0-9_]*)="([^"]*)"', raw_labels))
    key = (name, tuple(sorted(labels.items())))
    assert key not in metrics, f"duplicate series: {key}"
    metrics[key] = val

def get_val(metric_name, **want_labels):
    for (n, lbls), v in metrics.items():
        d = dict(lbls)
        if n == metric_name and all(d.get(k) == val for k, val in want_labels.items()):
            return v
    return None

# 1. Sub-50MB high-CPU processes MUST NOT vanish (clang++ 35MB @ 88.4%, rustc 42MB @ 64.0%, mds 24MB @ 15.3%)
cxx_cpu = get_val("host_workload_cpu_percent", workload="cxx-build")
rust_cpu = get_val("host_workload_cpu_percent", workload="rust-build")
spot_cpu = get_val("host_workload_cpu_percent", workload="macos-spotlight")
assert cxx_cpu is not None and cxx_cpu >= 88.0, f"missing sub-50MB cxx-build CPU: {cxx_cpu}"
assert rust_cpu is not None and rust_cpu >= 64.0, f"missing sub-50MB rust-build CPU: {rust_cpu}"
assert spot_cpu is not None and spot_cpu >= 15.0, f"missing sub-50MB macos-spotlight CPU: {spot_cpu}"

# 2. Disaggregated node/werk/service/role workloads (no single anonymous node blob hiding kade-4965)
werk_demo = get_val("host_workload_cpu_percent", workload="werk-demo-server", owner="kade-4965")
api_cpu = get_val("host_workload_cpu_percent", workload="chorus-api")
clearing_cpu = get_val("host_workload_cpu_percent", workload="chorus-clearing")
gathering_cpu = get_val("host_workload_cpu_percent", workload="gathering-app")
jest_cpu = get_val("host_workload_cpu_percent", workload="jest-tests", owner="chorus-wren")
promtail_cpu = get_val("host_workload_cpu_percent", workload="promtail")
assert werk_demo is not None and abs(werk_demo - 34.6) < 0.2, f"expected kade-4965 werk-demo-server=34.6, got {werk_demo}"
assert api_cpu is not None and api_cpu >= 8.0, f"missing chorus-api workload: {api_cpu}"
assert clearing_cpu is not None and clearing_cpu >= 10.0, f"missing chorus-clearing workload: {clearing_cpu}"
assert gathering_cpu is not None and gathering_cpu >= 27.0, f"missing gathering-app workload: {gathering_cpu}"
assert jest_cpu is not None and jest_cpu >= 41.0, f"missing chorus-wren jest-tests workload: {jest_cpu}"
assert promtail_cpu is not None and promtail_cpu >= 112.0, f"missing promtail workload: {promtail_cpu}"

# 3. Direct host load gauges + daily peak watermark
l1 = get_val("host_load_average", window="1m")
l5 = get_val("host_load_average", window="5m")
l15 = get_val("host_load_average", window="15m")
lmax = get_val("host_load1_daily_max")
assert l1 is not None and l5 is not None and l15 is not None, f"missing host_load_average: {l1},{l5},{l15}"
assert lmax is not None and lmax >= l1, f"missing/invalid host_load1_daily_max: {lmax} vs {l1}"
PY
}

@test "exporter captures sub-50MB compiler/system hogs, disaggregates node/werk/role workloads, and emits host load" {
  [ -x "$EXP" ] || { echo "exporter not executable: $EXP"; return 1; }
  TEXTFILE_DIR="$T" PS_INPUT_FILE="$FIX_PS" LOADAVG_OVERRIDE="86.40 42.10 19.85" TODAY_OVERRIDE="2026-11-17" \
    run "$EXP"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ -s "$T/process_memory.prom" ] || return 1
  run assert_ac3_prom "$T/process_memory.prom"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "host_load1_daily_max preserves intra-day load spikes across later calm ticks and resets on a new day" {
  # Tick 1 at 09:15: load spikes to 86.40 (even if Prometheus misses scraping this exact tick)
  TEXTFILE_DIR="$T" PS_INPUT_FILE="$FIX_PS" LOADAVG_OVERRIDE="86.40 42.10 19.85" TODAY_OVERRIDE="2026-11-17" \
    run "$EXP"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  grep -qx "host_load1_daily_max 86.40" "$T/process_memory.prom" || { grep host_load "$T/process_memory.prom"; return 1; }

  # Tick 2 later same day: load drops to 3.25 — daily max MUST stay 86.40
  TEXTFILE_DIR="$T" PS_INPUT_FILE="$FIX_PS" LOADAVG_OVERRIDE="3.25 8.10 12.00" TODAY_OVERRIDE="2026-11-17" \
    run "$EXP"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  grep -qx 'host_load_average{window="1m"} 3.25' "$T/process_memory.prom" || return 1
  grep -qx "host_load1_daily_max 86.40" "$T/process_memory.prom" || { grep host_load "$T/process_memory.prom"; return 1; }

  # Tick 3 next day: resets to the new day's load
  TEXTFILE_DIR="$T" PS_INPUT_FILE="$FIX_PS" LOADAVG_OVERRIDE="4.50 4.00 3.80" TODAY_OVERRIDE="2026-11-18" \
    run "$EXP"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  grep -qx "host_load1_daily_max 4.50" "$T/process_memory.prom" || { grep host_load "$T/process_memory.prom"; return 1; }
}

@test "negative proof (#3734): pre-#4992 process_memory.prom fails the AC3 validator on missing sub-50MB hogs and lumped node" {
  run assert_ac3_prom "$FIX_BEFORE"
  [ "$status" -ne 0 ] || { echo "validator passed pre-#4992 output!"; return 1; }
  [[ "$output" == *"missing sub-50MB cxx-build CPU"* ]] || { echo "$output"; return 1; }
}

@test "negative proof (#3734): failed ps command exits non-zero without clobbering prior process_memory.prom" {
  [ -x "$EXP" ] || { echo "exporter not executable: $EXP"; return 1; }
  printf 'KEEP_PRIOR\n' > "$T/process_memory.prom"
  TEXTFILE_DIR="$T" PS_CMD="/usr/bin/false" LOADAVG_OVERRIDE="2.00 2.00 2.00" \
    run "$EXP"
  [ "$status" -eq 1 ] || { echo "expected exit 1 when PS_CMD fails, got $status ($output)"; return 1; }
  [[ "$output" == *"ps failed"* ]] || { echo "$output"; return 1; }
  [ "$(cat "$T/process_memory.prom")" = "KEEP_PRIOR" ] || { cat "$T/process_memory.prom"; return 1; }
}
