#!/usr/bin/env bats
# @test-type: unit — pulse's cooldown is a property: no constant left, the land writes the row
# @domain: messages
# @card: #4889 · owner: silas
# What Jeff sees: how long pulse holds back a repeated machine alert is a row in the graph
# (pulse.announce.machine_cooldown_ms), read through the shared TS properties reader
# (platform/chorus-sdk/properties/global.js), not a number in platform/pulse/src/announce-boundary.ts.
# The hard-coded MACHINE_COOLDOWN_MS (30 min) is retired in the same land, and
# platform/scripts/land-4889-rows writes the row. The reader's behaviour (TTL, unread said
# once, no default) is proven in platform/pulse/src/wake-cooldown-4889.test.ts.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
SRC="$ROOT/platform/pulse/src"

# production pulse sources that still hold a cooldown number of their own, one per line
constant_left() {
  grep -rlE 'MACHINE_COOLDOWN_MS|COOLDOWN_MS *= *[0-9]' "$1" --include='*.ts' | grep -v '\.test\.ts$' || true
}

@test "no pulse source holds the cooldown as a constant" {
  out="$(constant_left "$SRC")"
  [ -z "$out" ] || { echo "still a constant in: $out"; return 1; }
}

@test "negative proof: a planted constant is found" {
  d="$BATS_TEST_TMPDIR/src"; mkdir -p "$d"
  echo 'export const MACHINE_COOLDOWN_MS = 30 * 60_000;' > "$d/announce-boundary.ts"
  [ "$(constant_left "$d")" = "$d/announce-boundary.ts" ] || return 1
}

@test "the boundary reads the property key through the shared reader, and the worker watches it" {
  grep -q "COOLDOWN_KEY = 'pulse.announce.machine_cooldown_ms'" "$SRC/announce-boundary.ts" || return 1
  grep -q "from '../../chorus-sdk/properties/global'" "$SRC/delivery-worker.ts" || return 1
  grep -q "worker.watchCooldown()" "$SRC/service.ts" || return 1
}

@test "the land writes the cooldown row: global, an int, no default" {
  run python3 - "$ROOT/platform/scripts/land-4889-rows" <<'PY'
import importlib.util, importlib.machinery, sys, json
loader = importlib.machinery.SourceFileLoader("land", sys.argv[1])
spec = importlib.util.spec_from_loader("land", loader); m = importlib.util.module_from_spec(spec); loader.exec_module(m)
rows = [w for c, n, w in m.plan() if c == "properties/properties" and w and w.get("propertyKey") == "pulse.announce.machine_cooldown_ms"]
print(json.dumps(rows))
PY
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *'"propertyScope": "global"'* && "$output" == *'"propertyValueType": "int"'* && "$output" == *'"propertyValue": "1800000"'* ]] || { echo "$output"; return 1; }
}
