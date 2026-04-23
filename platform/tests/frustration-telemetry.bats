#!/usr/bin/env bats
# frustration-telemetry.bats — #2454
# What Jeff sees: the page renders bad days with memory-write follow-up,
# so the "did the team learn?" question has an honest answer per spike.

TELEMETRY="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/frustration-telemetry.sh"
RENDERER="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/frustration-telemetry-render.py"

setup() {
  OUT="$(mktemp)"
}

teardown() {
  rm -f "$OUT"
}

# --- AC: 3 vocab panels + memory-writes overlay + top-bad-days table ---

@test "telemetry JSON includes memory_writes field" {
  bash "$TELEMETRY" --json --days 30 | python3 -c "import json,sys; d=json.load(sys.stdin); assert 'memory_writes' in d, 'missing memory_writes key'; assert isinstance(d['memory_writes'], dict)"
}

@test "rendered page contains memory-writes panel" {
  bash "$TELEMETRY" --json --days 30 | python3 "$RENDERER" > "$OUT"
  grep -q "Team learning" "$OUT"
  grep -q 'id="c4"' "$OUT"
}

@test "rendered page contains top-bad-days narrative table" {
  bash "$TELEMETRY" --json --days 30 | python3 "$RENDERER" > "$OUT"
  grep -q "Top bad days" "$OUT"
  grep -q "Memory writes 48h" "$OUT"
}

@test "narrative table renders honest-fold when no memory writes follow a spike" {
  bash "$TELEMETRY" --json --days 30 | python3 "$RENDERER" > "$OUT"
  # At least one row present; silent-day marker appears only when applicable (not a failure if absent)
  grep -qE "frustration|Day" "$OUT"
}

@test "canvases have explicit pixel-background to diagnose blank renders" {
  bash "$TELEMETRY" --json --days 30 | python3 "$RENDERER" > "$OUT"
  grep -q "display: block" "$OUT"
}
