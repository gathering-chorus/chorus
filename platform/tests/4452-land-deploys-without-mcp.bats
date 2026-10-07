#!/usr/bin/env bats
# @test-type: unit — hermetic: extracts the real deploy-canonical step body from
# @domain: cicd — werk.yml and runs it against stubs on PATH. No services, no network.
#
# #4452 — Jeff, 2026-10-07: "the deploy 'dropping' and u fixing by hand is a pattern".
# The land's deploy-canonical step called werk-deploy THROUGH chorus-mcp. A card that
# deploys chorus-mcp restarted the service carrying the call: #4446 run 11 dropped at
# 10:25:04 (`curl: (18) transfer closed`), pulse never deployed, a role finished it by
# hand. The step must run werk-deploy itself, so a restart of chorus-mcp cannot cut it.
#
# The stubbed chorus-mcp-call.sh fails the way the drop did. A step that still routes
# the deploy through it goes red; the NEGATIVE PROOF runs the pre-#4452 step body and
# shows exactly that.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  WF="$REPO/.github/workflows/werk.yml"
  TMP="$(mktemp -d)"
  export TMP
  mkdir -p "$TMP/home/.chorus/bin" "$TMP/bin"
  # werk-deploy: records how it was called, succeeds
  cat > "$TMP/home/.chorus/bin/werk-deploy" <<'STUB'
#!/bin/bash
echo "args=$* role=${DEPLOY_ROLE:-} chorus_role=${CHORUS_ROLE:-} home=${CHORUS_HOME:-}" >> "$TMP/werk-deploy.calls"
echo "deploy.completed (stub)"
STUB
  # chorus-mcp-call.sh: the transport dropping mid-deploy, as on #4446 run 11
  cat > "$TMP/bin/chorus-mcp-call.sh" <<'STUB'
#!/bin/bash
echo "curl: (18) transfer closed with outstanding read data remaining"
echo "  <no response from MCP>"
exit 1
STUB
  chmod +x "$TMP/home/.chorus/bin/werk-deploy" "$TMP/bin/chorus-mcp-call.sh"
}

teardown() {
  [ -n "${TMP:-}" ] && rm -rf "$TMP"
}

# Pull the `run:` body of a named step out of a workflow file and dedent it.
# A vanished step fails loudly — a guard whose target is gone must go red (#3734).
extract_step_run() {
  local wf="$1" step="$2" out="$3"
  awk -v want="$step" '
    $0 ~ "^      - name: " { instep = ($0 == "      - name: " want); inrun = 0; next }
    instep && $0 ~ "^        run: \\|" { inrun = 1; next }
    inrun {
      if ($0 ~ /^        [a-z-]+:/) { inrun = 0; next }
      sub(/^          /, ""); print
    }
  ' "$wf" | sed 's/\${{ steps\.provenround\.outputs\.already_landed }}/false/g' > "$out"
  [ -s "$out" ] || { echo "extract failed: no run body for '$step' in $wf" >&2; return 1; }
}

run_step() {
  local body="$1"
  HOME="$TMP/home" PATH="$TMP/bin:/usr/bin:/bin" TMP="$TMP" \
    SYNC_OK=1 LANDED_COMMIT=abc123 CARD_ID=4452 ROLE=silas CHORUS_HOME="$TMP/chorus" \
    CHORUS_WERK_BASE="$TMP/werk" \
    bash -e "$body"
}

@test "the land's prod deploy finishes when the MCP transport drops" {
  extract_step_run "$WF" deploy-canonical "$TMP/step.sh"
  run run_step "$TMP/step.sh"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  # werk-deploy ran itself, as the card's role, against the landed commit
  grep -q 'args=4452 --landedCommit abc123 role=silas chorus_role=silas' "$TMP/werk-deploy.calls"
}

@test "NEGATIVE PROOF: the pre-#4452 step, which deploys through MCP, goes red on the same drop" {
  git -C "$REPO" show 31cae29c9:.github/workflows/werk.yml > "$TMP/werk-old.yml"
  extract_step_run "$TMP/werk-old.yml" deploy-canonical "$TMP/old.sh"
  run run_step "$TMP/old.sh"
  [ "$status" -ne 0 ]
  [ ! -s "$TMP/werk-deploy.calls" ]
}

@test "an unfinished prod deploy fails the land loudly and names the service" {
  cat > "$TMP/home/.chorus/bin/werk-deploy" <<'STUB'
#!/bin/bash
echo "canonical deploy of chorus-mcp failed: com.chorus.mcp did not come up after kickstart" >&2
exit 1
STUB
  extract_step_run "$WF" deploy-canonical "$TMP/step.sh"
  run run_step "$TMP/step.sh"
  [ "$status" -ne 0 ]
  [[ "$output" == *"com.chorus.mcp did not come up"* ]] || { echo "$output"; return 1; }
}

@test "NEGATIVE PROOF: a step that swallows werk-deploy's error would not name the service" {
  cat > "$TMP/home/.chorus/bin/werk-deploy" <<'STUB'
#!/bin/bash
echo "canonical deploy of chorus-mcp failed: com.chorus.mcp did not come up after kickstart" >&2
exit 1
STUB
  extract_step_run "$WF" deploy-canonical "$TMP/step.sh"
  # mutate: drop the 2>&1 capture, so stderr never reaches the printed $out
  sed -i '' 's/ 2>&1) \&\& rc=0/ 2>\/dev\/null) \&\& rc=0/' "$TMP/step.sh"
  grep -q '2>/dev/null) && rc=0' "$TMP/step.sh"
  run run_step "$TMP/step.sh"
  [ "$status" -ne 0 ]
  [[ "$output" != *"com.chorus.mcp did not come up"* ]] || return 1
}
