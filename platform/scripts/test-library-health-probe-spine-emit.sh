#!/usr/bin/env bash
# test-library-health-probe-spine-emit.sh (#2835)
# AC: library-health-probe.sh emits library.health.{failed,passed} via chorus-log
# alongside the existing nudge path, so probe results are queryable on the spine.
#
# Stubs ssh to capture invocations; runs the probe with synthetic curl results.
# Asserts a chorus-log invocation appears in the captured ssh commands.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROBE="$SCRIPT_DIR/library-health-probe.sh"
TMPDIR=$(mktemp -d)
# #2856 — emit canonical results line on EXIT (composed with TMPDIR cleanup)
# so nightly-suites.sh consumer hits tier-1 summary parser, not rc-synthesis.
trap '_rc=$?; rm -rf "$TMPDIR"; if [ $_rc -eq 0 ]; then echo "=== Results: 1 passed, 0 failed ==="; else echo "=== Results: 0 passed, 1 failed ==="; fi' EXIT

# Stub ssh: record args, succeed.
cat > "$TMPDIR/ssh" <<'EOF'
#!/usr/bin/env bash
echo "$@" >> "$SSH_LOG"
exit 0
EOF
chmod +x "$TMPDIR/ssh"

# Stub curl: simulate all-services-down so FAILURES > 0 path runs.
cat > "$TMPDIR/curl" <<'EOF'
#!/usr/bin/env bash
echo "000"
exit 0
EOF
chmod +x "$TMPDIR/curl"

export SSH_LOG="$TMPDIR/ssh.log"
: > "$SSH_LOG"

PATH="$TMPDIR:$PATH" bash "$PROBE" >/dev/null 2>&1 || true

if grep -q "chorus-log library.health.failed" "$SSH_LOG"; then
  echo "PASS: library.health.failed emitted on FAILURES>0 path"
else
  echo "FAIL: no chorus-log library.health.failed in ssh invocations"
  echo "--- ssh.log ---"
  cat "$SSH_LOG"
  exit 1
fi
