#!/bin/bash
# @test-type: perf — session-start wall clock, shown in seconds; over budget is SLOW, never red (#4136)
#
# #4138 — this was "session-start completes in under 10s typical" inside
# session-start-orchestration-e2e.bats, a coin flip on a loaded box that read
# as a nightly red. As a perf row the nightly prints the seconds beside the red
# count (#4136), the same as the werk phase budgets.
set -u
CHORUS_ROOT="${CHORUS_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
BUDGET_S="${SESSION_START_BUDGET_S:-10}"
ROLE="${SESSION_START_ROLE:-silas}"
SHIM="$(command -v chorus-hook-shim || true)"
[ -x "$SHIM" ] || SHIM="$HOME/.chorus/bin/chorus-hook-shim"
[ -x "$SHIM" ] || SHIM="${CHORUS_ROOT}/platform/services/chorus-hooks/target/release/chorus-hook-shim"
[ -x "$SHIM" ] || { echo "SKIP: no chorus-hook-shim installed"; exit 0; }

start=$(python3 -c 'import time; print(int(time.time()*1000))')
"$SHIM" session-start "$ROLE" >/dev/null 2>&1 || true
end=$(python3 -c 'import time; print(int(time.time()*1000))')
ms=$((end - start)); secs=$(python3 -c "print(f'{$ms/1000:.1f}')")
load=$(uptime | sed 's/.*load averages*: *//' | awk '{print int($1)}')

if [ "$ms" -lt $((BUDGET_S * 1000)) ]; then
  echo "PASS: session-start ${secs}s (budget ${BUDGET_S}s, load ${load:-?})"; exit 0
fi
echo "SLOW: session-start ${secs}s over the ${BUDGET_S}s budget (load ${load:-?}) — speed, not breakage"; exit 1
