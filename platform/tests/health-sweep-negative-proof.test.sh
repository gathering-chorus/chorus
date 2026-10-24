#!/usr/bin/env bash
# @test-type: unit
# Negative proof for #4719:
# 1. Shows that an active, non-whitelisted agent with a stale log or exit != 0 FAILS loudly.
# 2. Shows that pruned/silenced logs and retired plists do not falsely fail.
set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
SCRIPT="$CHORUS_ROOT/platform/scripts/deep-health.sh"

echo "=== #4719 Negative Proof Test ==="

# Test 1: Real dead service alerts (live reading has 7 real crashed services)
FAILURES_COUNT=$(curl -s http://localhost:3340/api/chorus/context/health | python3 -c "
import sys, json
d = json.load(sys.stdin)
failures = d['data']['failures']
checks = d['data']['checks']
crashed = [c for c in checks if c.get('name') == 'crashed']
assert failures == 7, f'Expected 7 failures, got {failures}'
assert len(crashed) == 7, f'Expected 7 crashed service failures, got {len(crashed)}'
print(f'PASS: Detected {len(crashed)} real crashed services as failures')
")
echo "$FAILURES_COUNT"

# Test 2: Pruned logs do not appear in failures
curl -s http://localhost:3340/api/chorus/context/health | python3 -c "
import sys, json
d = json.load(sys.stdin)
reasons = ' '.join(c.get('reason', '') for c in d['data']['checks'])
pruned = ['roles-up.log', 'mcp-config-herald.log', 'building-pipeline.log', 'embed-worker.log', 'log-sync.log', 'infra-alert.log', 'security-scan-weekly.log', 'buzz-tunnel']
for p in pruned:
    assert p not in reasons, f'Pruned monitor {p} still triggered failure: {reasons}'
print('PASS: All 8 dead log monitors successfully pruned/silenced')
"

# Test 3: Ghost plists do not appear in failures
curl -s http://localhost:3340/api/chorus/context/health | python3 -c "
import sys, json
d = json.load(sys.stdin)
reasons = ' '.join(c.get('reason', '') for c in d['data']['checks'])
ghosts = ['com.chorus.api.werk.abby-normal', 'com.chorus.athena-make.werk.abby-normal', 'com.chorus.clearing.werk.abby-normal', 'com.chorus.hooks.werk.abby-normal', 'com.chorus.log-harvest']
for g in ghosts:
    assert g not in reasons, f'Ghost plist {g} still triggered failure'
print('PASS: All 5 ghost plists successfully retired')
"

# Test 4: Negative Proof via hermetic test seam:
# When an unknown service goes silent/crashes, deep-health detects it and fails.
TMP_DIR=$(mktemp -d /tmp/health-proof-XXXXXX)
trap 'rm -rf "$TMP_DIR"' EXIT

# Create a mock plist directory with a synthetic service that is NOT retired
mkdir -p "$TMP_DIR/LaunchAgents" "$TMP_DIR/Logs"
cat << 'EOF' > "$TMP_DIR/LaunchAgents/com.chorus.fake-service.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.chorus.fake-service</string>
    <key>StandardOutPath</key>
    <string>/tmp/fake-service.log</string>
</dict>
</plist>
EOF

# Agent list where fake-service is loaded with exit code 1 and PID is '-' (crashed/not running)
echo "- 1 com.chorus.fake-service" > "$TMP_DIR/agents.txt"

# Run deep-health with test seams
export HEALTH_PLIST_DIR="$TMP_DIR/LaunchAgents"
export HEALTH_LOG_DIR="$TMP_DIR/Logs"
export HEALTH_AGENT_LIST="$TMP_DIR/agents.txt"
export HEALTH_JSON_OUT="$TMP_DIR/health.json"
export HEALTH_OPS_NUDGE="true"
export HEALTH_CHORUS_LOG="true"

bash "$SCRIPT" >/dev/null 2>&1 || true

python3 -c "
import json
d = json.load(open('$TMP_DIR/health.json'))
failures = d.get('details', [])
fake_failures = [f for f in failures if 'com.chorus.fake-service' in f or 'fake-service.log' in f]
assert len(fake_failures) > 0, f'Expected fake-service to trigger failure, got: {failures}'
assert any('crashed: com.chorus.fake-service (exit 1)' in f for f in fake_failures), f'Missing crash failure in {fake_failures}'
print('PASS: Negative proof confirmed — un-whitelisted failing service reliably triggers health failure')
"

echo "=== All negative proof tests passed ==="
