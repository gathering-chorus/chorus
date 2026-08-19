#!/usr/bin/env bats
# @test-type: unit — hermetic: extracts the real step body from werk.yml and runs it
# against a stubbed verb on PATH. No services, no network, no $HOME.
#
# #3932 — a failing merge step must PRINT why.
#
# The defect this pins: werk.yml's merge step is the only step in the pipeline
# that captures a chorus-mcp-call.sh invocation into a variable. The helper
# prints the verb's error text to STDOUT and exits 1; steps run under `bash -e`,
# so the shell dies AT the assignment and the printf that would have shown the
# reason never runs. Result: "Failure - Main merge [6.6s]" with no reason —
# three times on #3911, and again on #3928's land, where the PR had in fact
# already merged.
#
# The test runs the REAL step body extracted from werk.yml (never a copy, which
# would drift) against a stub verb that fails with a known reason.

setup() {
  # Resolve the workflow from THIS test file's own tree — never $CHORUS_ROOT,
  # which points at canonical. A test that reads canonical while running in a
  # werk measures the code it is not testing: this test passed green against
  # the unfixed step for exactly that reason before the pin was removed.
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  WF="$REPO/.github/workflows/werk.yml"
  TMP="$(mktemp -d)"
  export TMP
}

teardown() {
  [ -n "${TMP:-}" ] && rm -rf "$TMP"
}

# Pull the `run:` body of a named step out of werk.yml and dedent it.
# Fails loudly if the step or its run block is gone — a guard whose target
# vanished must go red, never pass vacuously (#3734).
extract_step_run() {
  local step="$1" out="$2"
  awk -v want="$step" '
    $0 ~ "^      - name: " {
      instep = ($0 == "      - name: " want); inrun = 0; next
    }
    instep && $0 ~ "^        run: \\|" { inrun = 1; next }
    inrun {
      # A line at step-key indentation ends the run block.
      if ($0 ~ /^        [a-z-]+:/) { inrun = 0; next }
      sub(/^          /, "")
      print
    }
  ' "$WF" > "$out"
  [ -s "$out" ] || {
    echo "extract failed: no run body for step '$step' in $WF" >&2
    return 1
  }
}

# A stub standing in for the real MCP helper: prints the verb's reason to
# STDOUT (as chorus-mcp-call.sh does) and exits non-zero.
stub_failing_helper() {
  mkdir -p "$TMP/bin"
  cat > "$TMP/bin/chorus-mcp-call.sh" <<'STUB'
#!/usr/bin/env bash
echo "  merge-fail: pr #1013 state is 'MERGED', not OPEN — content did not land"
exit 1
STUB
  chmod +x "$TMP/bin/chorus-mcp-call.sh"
}

stub_succeeding_helper() {
  mkdir -p "$TMP/bin"
  cat > "$TMP/bin/chorus-mcp-call.sh" <<'STUB'
#!/usr/bin/env bash
echo "  {\"ok\":true,\"landedCommit\":\"658f317b2aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}"
exit 0
STUB
  chmod +x "$TMP/bin/chorus-mcp-call.sh"
}

run_merge_step() {
  extract_step_run merge "$TMP/step.sh"
  PATH="$TMP/bin:$PATH" \
  ROLE=kade CARD_ID=3932 GITHUB_ENV="$TMP/gh_env" \
    bash -e "$TMP/step.sh" 2>&1
}

@test "a failing merge prints the verb's reason" {
  stub_failing_helper
  run run_merge_step
  [ "$status" -ne 0 ]
  [[ "$output" == *"merge-fail"* ]]
  [[ "$output" == *"pr #1013"* ]]
}

@test "a succeeding merge still threads the landed commit forward" {
  stub_succeeding_helper
  run run_merge_step
  [ "$status" -eq 0 ]
  grep -q "LANDED_COMMIT=658f317b2" "$TMP/gh_env"
}

@test "the extractor fails loud when the step is gone" {
  # NEGATIVE PROOF for the harness itself: if werk.yml is renamed or the step
  # removed, this test must go red rather than silently testing nothing.
  run extract_step_run no-such-step "$TMP/gone.sh"
  [ "$status" -ne 0 ]
  [[ "$output" == *"extract failed"* ]]
}
