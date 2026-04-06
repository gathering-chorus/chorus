#!/usr/bin/env bash
# Test: Chorus API must respond to health checks while handling slow routes
# AC: concurrent requests don't block — health endpoint responds within 2s
# even when a card-story request is in flight
set -euo pipefail

API="http://localhost:3340"
PASS=0
FAIL=0

echo "=== Chorus API freeze test ==="

# Test 1: Health endpoint responds within 2s under normal conditions
start=$(python3 -c "import time; print(int(time.time()*1000))")
if curl -sf --max-time 2 "$API/api/chorus/health" > /dev/null 2>&1; then
  end=$(python3 -c "import time; print(int(time.time()*1000))")
  elapsed=$((end - start))
  echo "PASS: health responds in ${elapsed}ms"
  PASS=$((PASS + 1))
else
  echo "FAIL: health didn't respond within 2s"
  FAIL=$((FAIL + 1))
fi

# Test 2: Fire a slow card-story request AND a health check concurrently
# card 99999 doesn't exist — the cards CLI will take time to fail
# Health should still respond within 2s even if card-story is blocking
curl -sf --max-time 15 "$API/api/chorus/card-story/99999" > /dev/null 2>&1 &
SLOW_PID=$!
sleep 0.5  # Let the slow request start

start=$(python3 -c "import time; print(int(time.time()*1000))")
if curl -sf --max-time 2 "$API/api/chorus/health" > /dev/null 2>&1; then
  end=$(python3 -c "import time; print(int(time.time()*1000))")
  elapsed=$((end - start))
  echo "PASS: health responds in ${elapsed}ms while card-story in flight"
  PASS=$((PASS + 1))
else
  echo "FAIL: health blocked by concurrent card-story request (execSync freeze)"
  FAIL=$((FAIL + 1))
fi

wait $SLOW_PID 2>/dev/null || true

# Test 3: Fire 3 card-story requests AND a health check
for i in 1 2 3; do
  curl -sf --max-time 15 "$API/api/chorus/card-story/9999$i" > /dev/null 2>&1 &
done
sleep 0.5

start=$(python3 -c "import time; print(int(time.time()*1000))")
if curl -sf --max-time 2 "$API/api/chorus/health" > /dev/null 2>&1; then
  end=$(python3 -c "import time; print(int(time.time()*1000))")
  elapsed=$((end - start))
  echo "PASS: health responds in ${elapsed}ms under 3 concurrent slow requests"
  PASS=$((PASS + 1))
else
  echo "FAIL: health blocked by 3 concurrent card-story requests"
  FAIL=$((FAIL + 1))
fi

wait 2>/dev/null || true

echo "---"
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
