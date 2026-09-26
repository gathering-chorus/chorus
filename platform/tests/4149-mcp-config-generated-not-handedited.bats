#!/usr/bin/env bats
# @test-type: unit
# @domain: roles — the product domain this suite guards (#4334)
# 4149 — the role MCP config is GENERATED, and the generator is the only writer.
#
# What went wrong (2026-09-12/13). #4149 hand-edited a grafana block into all
# three roles/*/.mcp.json. gen-role-mcp.sh's template never learned it. At
# 03:26 the next morning the nightly ran 3125-cclsp-abs-path.bats, whose last
# case invokes the generator against the LIVE repo — it regenerated all three
# configs from the old template and the grafana entry was gone. Every role
# booted without Loki/Grafana tools twelve hours after the card was accepted.
#
# Two defects, proven separately here:
#   1. the template does not carry grafana  (hand-edit into a generated file)
#   2. a test writes to the product surface (the generator had no way to be
#      pointed somewhere else, so testing it meant clobbering the live configs)
#
# Plus the gate the card explicitly waived ("no test added for config text"):
#   3. drift — on-disk role configs must equal what the generator emits today.

REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
GEN="$REPO_ROOT/platform/scripts/gen-role-mcp.sh"
DRIFT="$REPO_ROOT/platform/scripts/check-mcp-config-drift.sh"
ROLES=(wren silas kade)

srv() { python3 -c "import json,sys; print(' '.join(json.load(open(sys.argv[1]))['mcpServers'].keys()))" "$1"; }

setup() { TMP="$(mktemp -d)"; }
teardown() { rm -rf "$TMP"; }

@test "generator emits the grafana server for every role" {
  run env GEN_MCP_WRITE_ROOT="$TMP" bash "$GEN"
  [ "$status" -eq 0 ]
  for r in "${ROLES[@]}"; do
    run srv "$TMP/roles/$r/.mcp.json"
    [ "$status" -eq 0 ]
    [[ "$output" == *grafana* ]]
  done
}

@test "grafana runs read-only — admin and write tools disabled" {
  env GEN_MCP_WRITE_ROOT="$TMP" bash "$GEN" >/dev/null
  run python3 -c "import json;a=json.load(open('$TMP/roles/silas/.mcp.json'))['mcpServers']['grafana']['args'];print(' '.join(a))"
  [[ "$output" == *-disable-admin* ]]
  [[ "$output" == *-disable-write* ]]
}

@test "generator writes where told and leaves the live repo untouched" {
  # cksum, not md5: md5 is macOS-only and the pipeline's act container has
  # neither it nor md5sum. The test must run wherever the pipeline runs.
  before="$(cksum < "$REPO_ROOT/roles/silas/.mcp.json")"
  env GEN_MCP_WRITE_ROOT="$TMP" bash "$GEN" >/dev/null
  after="$(cksum < "$REPO_ROOT/roles/silas/.mcp.json")"
  [ "$before" = "$after" ]
  [ -f "$TMP/roles/silas/.mcp.json" ]
}

@test "drift check passes when the on-disk configs are what the generator emits" {
  env GEN_MCP_WRITE_ROOT="$TMP" bash "$GEN" >/dev/null
  run bash "$DRIFT" "$TMP"
  [ "$status" -eq 0 ]
}

@test "NEGATIVE PROOF — drift check goes red when a role loses its grafana server" {
  env GEN_MCP_WRITE_ROOT="$TMP" bash "$GEN" >/dev/null
  python3 - "$TMP/roles/kade/.mcp.json" <<'PY'
import json,sys
p=sys.argv[1]; d=json.load(open(p)); del d["mcpServers"]["grafana"]
json.dump(d,open(p,"w"),indent=2)
PY
  run bash "$DRIFT" "$TMP"
  [ "$status" -ne 0 ]
  [[ "$output" == *kade* ]]
}

@test "NEGATIVE PROOF — drift check goes red when a role config is missing entirely" {
  env GEN_MCP_WRITE_ROOT="$TMP" bash "$GEN" >/dev/null
  rm "$TMP/roles/wren/.mcp.json"
  run bash "$DRIFT" "$TMP"
  [ "$status" -ne 0 ]
  [[ "$output" == *wren* ]]
}
