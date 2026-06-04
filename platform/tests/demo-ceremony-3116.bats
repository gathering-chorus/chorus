#!/usr/bin/env bats
# #3116 retirement gate — werk-demo is the proving ceremony, not an orchestrator.
# Structural memory (ADR-026 retirement-gate pattern): the ACT and the
# go-run-your-gate nudge relay were stripped out of the binary; a future
# contributor must not type them back in. The gate-CHECKS survive, relocated to
# the /demo skill layer as subagents — so we assert their RELAY is gone, not the
# gates themselves. Pairs with designing/docs/demo-service-design.html.

ROOT="$BATS_TEST_DIRNAME/../.."
DEMO="$ROOT/platform/services/werk-demo/src/lib.rs"
ACCEPT="$ROOT/platform/services/werk-accept/src/lib.rs"

@test "werk-demo does NOT run the go-run-your-gate nudge relay (#3116)" {
  run grep -n "send_gate_request_nudge" "$DEMO"
  [ "$status" -ne 0 ]
}

@test "werk-demo does NOT block on an in-binary gate-chain wait (#3116)" {
  run grep -n "CHORUS_DEMO_GATE_WAIT_SECS" "$DEMO"
  [ "$status" -ne 0 ]
}

@test "werk-demo does NOT build or deploy — the act is out (#3116)" {
  run grep -nE 'run\("werk-build"|run\("werk-deploy"' "$DEMO"
  [ "$status" -ne 0 ]
}

@test "werk-demo delegates the gate chain to the skill layer (#3116)" {
  run grep -n "demo.gate.delegated" "$DEMO"
  [ "$status" -eq 0 ]
}

@test "werk-demo records ONE demo.verdict (the proving output) (#3116)" {
  run grep -n "demo.verdict" "$DEMO"
  [ "$status" -eq 0 ]
}

@test "werk-accept gates finalize on demo.verdict, not preflight-pass/show.completed (#3116)" {
  run grep -n "demo_verdict_pass" "$ACCEPT"
  [ "$status" -eq 0 ]
}
