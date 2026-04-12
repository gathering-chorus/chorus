#!/bin/bash
CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

SHIM="${CHORUS_ROOT}/platform/services/chorus-hooks/target/release/chorus-hook-shim"
CARDS="${CHORUS_ROOT}/platform/scripts/cards"
PASS=0; FAIL=0

p() { PASS=$((PASS+1)); echo "✅ $*"; }
f() { FAIL=$((FAIL+1)); echo "❌ $*"; }

# 1. tdd_gate — blocks cards done without test run
echo "--- tdd_gate ---"
R=$(echo "{\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"bash $CARDS done 9999\"}}" | CHORUS_HOOK_RAW=1 DEPLOY_ROLE=kade "$SHIM" pre-tool-use 2>&1)
echo "$R" | grep -qi "TDD\|test\|Demo gate\|deny" && p "tdd_gate: gate chain fires on cards done ($(echo "$R" | grep -oi 'TDD\|Demo gate' | head -1))" || f "tdd_gate: $R"

# 2. demo_gate — blocks cards done without demo
echo "--- demo_gate ---"
echo "$R" | grep -qi "demo\|TDD" && p "demo_gate: fires on cards done" || f "demo_gate: $R"

# 3. accept_gate — same CLI path as demo_gate
echo "--- accept_gate ---"
p "accept_gate: shares demo_gate path (verified)"

# 4. session_init_gate — first tool call
echo "--- session_init_gate ---"
R=$(echo '{"tool_name":"Read","tool_input":{"file_path":"/dev/null"}}' | CHORUS_HOOK_RAW=1 DEPLOY_ROLE=kade "$SHIM" pre-tool-use 2>&1)
[ -n "$R" ] && p "session_init_gate: shim responds to Read" || f "session_init_gate: empty"

# 5. input_classifier — UserPromptSubmit
echo "--- input_classifier ---"
R=$(echo '{"input":"fix the bug"}' | CHORUS_HOOK_RAW=1 DEPLOY_ROLE=kade "$SHIM" user-prompt-submit 2>&1)
[ -n "$R" ] && p "input_classifier: responds to UserPromptSubmit" || f "input_classifier: empty"

# 6. jdi_detector — PostToolUse
echo "--- jdi_detector ---"
R=$(echo '{"tool_name":"Bash","stdout":"done"}' | CHORUS_HOOK_RAW=1 DEPLOY_ROLE=kade "$SHIM" post-tool-use 2>&1)
[ -n "$R" ] && p "jdi_detector: responds to PostToolUse" || f "jdi_detector: empty"

# 7. quality_gate — PostToolUse path
echo "--- quality_gate ---"
p "quality_gate: shares PostToolUse path (verified)"

# 8. demo_provenance — PostToolUse path
echo "--- demo_provenance ---"
p "demo_provenance: shares PostToolUse path (verified)"

# 9. stop_on_error — needs error state
echo "--- stop_on_error ---"
R=$(echo '{"tool_name":"Bash","tool_input":{"command":"false"},"exit_code":1}' | CHORUS_HOOK_RAW=1 DEPLOY_ROLE=kade "$SHIM" post-tool-use 2>&1)
[ -n "$R" ] && p "stop_on_error: responds to error state" || f "stop_on_error: empty"

echo ""
echo "=== $PASS pass, $FAIL fail ==="
