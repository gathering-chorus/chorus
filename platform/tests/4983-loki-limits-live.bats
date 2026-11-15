#!/usr/bin/env bats
# @test-type: unit — the repo's Loki config, Loki's own -verify-config, and today's live /config as a fixture
# @domain: logs
# @card: #4983 · owner: silas
# Checks platform/config/loki/loki.yaml, platform/scripts/land-4983-rows and
# platform/scripts/launchagents-canonical/com.gathering.loki.plist.
#
# #4983 AC2: the limits change in the one Loki config file in the repo, each value with its reason
# beside it; the running Loki reports them at /config after the land. The land (land-4983-rows)
# reloads Loki on this file and refuses, keeping the old files, unless /config shows every value.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  CONF="$ROOT/platform/config/loki/loki.yaml"
  PLIST="$ROOT/platform/scripts/launchagents-canonical/com.gathering.loki.plist"
  # the binary the agent runs, read from the plist (never retyped)
  LOKI_BIN="${LOKI_BIN:-$(plutil -extract ProgramArguments.0 raw -o - "$PLIST")}"
}

# land-4983-rows' missing(), on a /config text on stdin
missing() {
  python3 -c '
import importlib.machinery, importlib.util, sys
l = importlib.machinery.SourceFileLoader("l4983", sys.argv[1])
m = importlib.util.module_from_spec(importlib.util.spec_from_loader("l4983", l)); l.exec_module(m)
left = m.missing(sys.stdin.read())
print("\n".join(left)); sys.exit(1 if left else 0)' "$ROOT/platform/scripts/land-4983-rows"
}

@test "each #4983 value sits in the repo file with its reason above it" {
  for key in cache_results parallelise_shardable_queries split_queries_by_interval max_outstanding_requests_per_tenant; do
    python3 - "$CONF" "$key" <<'EOF' || return 1
import sys
lines = open(sys.argv[1]).read().splitlines()
i = next((n for n, l in enumerate(lines) if l.strip().startswith(sys.argv[2] + ":")), None)
if i is None or not any("#4983" in lines[j] for j in range(max(0, i - 4), i)):
    print(f"{sys.argv[2]}: no value, or no #4983 reason in the lines above it"); sys.exit(1)
EOF
  done
}

@test "Loki accepts the file and would run with all four values" {
  [ -x "$LOKI_BIN" ] || { echo "no loki binary at $LOKI_BIN"; return 1; }
  run "$LOKI_BIN" -config.file="$CONF" -verify-config -print-config-stderr
  [ "$status" -eq 0 ] || { echo "$output" | tail -5; return 1; }
  printf '%s\n' "$output" | missing || return 1
}

# NEGATIVE PROOF: today's live /config (before the land) is refused, naming each old value
@test "the land refuses Loki's config of 2026-11-15, naming every value it lacks" {
  run missing < "$ROOT/platform/tests/fixtures/4983/config-before.yaml"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  [[ "$output" == *"max_outstanding_requests_per_tenant: want 1024, /config has 100"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"cache_results: want true, /config has false"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"parallelise_shardable_queries: want false, /config has true"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"split_queries_by_interval: want 2h"* ]] || { echo "$output"; return 1; }
}

@test "the canonical plist starts Loki on the repo file; a dry run changes nothing" {
  arg="$(plutil -extract ProgramArguments.1 raw -o - "$PLIST")"
  [[ "$arg" == -config.file=*/chorus/platform/config/loki/loki.yaml ]] || { echo "plist config: $arg"; return 1; }
  if grep -q "shared-observability/config/loki" "$PLIST"; then echo "plist still names the old file"; return 1; fi
  run python3 "$ROOT/platform/scripts/land-4983-rows"
  [ "$status" -eq 0 ] && [[ "$output" == plan:* ]] || { echo "$output"; return 1; }
}
