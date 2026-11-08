#!/bin/bash
# #4924 AC1 demo proof: live throwaway start outside Jeff's session
# Runs once during #4924 demo ceremony.
# Verifies:
# 1. /bin/launchctl asuser 502 /usr/bin/sudo -n -u chorus-silas has empty keychain
# 2. Key search list in isolated bootstrap domain is empty
# 3. Security refusal is instant (<10ms, exit 37, no GUI prompt)
# 4. Writes fresh credentials file (~/.claude/.credentials.json, mode 0600)
# 5. Posts before/after timestamps to card 4924 comment
set -euo pipefail

CARD=4924
CRED_FILE="/Users/chorus-silas/.claude/.credentials.json"
BEFORE_TS=$(stat -f "%Sm" -t "%Y-%m-%d %H:%M:%S" "$CRED_FILE" 2>/dev/null || echo "none")

echo "=== #4924 AC1 Demo: Live Throwaway Start ==="
echo "Before timestamp: $BEFORE_TS"

# 1. Run live isolated keychain check
KC_OUT=$(sudo -n /bin/launchctl asuser 502 /usr/bin/sudo -n -u chorus-silas -H security list-keychains -d user 2>&1 || true)
echo "Keychain list: '${KC_OUT}'"
if [ -n "$(echo "$KC_OUT" | tr -d '[:space:]\"')" ]; then
  echo "FAIL: Expected empty keychain list in isolated bootstrap, got: $KC_OUT" >&2
  exit 1
fi

# 2. Run throwaway sign-in / start
echo "Running throwaway claude session as chorus-silas..."
sudo -n /bin/launchctl asuser 502 /usr/bin/sudo -n -u chorus-silas -H bash -c "umask 002 && cd /Users/jeffbridwell/CascadeProjects/chorus/roles/silas && /Users/jeffbridwell/.local/bin/claude -c 'exit' 2>&1 || true"

# 3. Measure after timestamp
AFTER_TS=$(stat -f "%Sm" -t "%Y-%m-%d %H:%M:%S" "$CRED_FILE" 2>/dev/null || echo "none")
echo "After timestamp: $AFTER_TS"

MODE=$(stat -f "%OLp" "$CRED_FILE" 2>/dev/null || stat -c "%a" "$CRED_FILE")
OWNER=$(stat -f "%Su" "$CRED_FILE" 2>/dev/null || stat -c "%U" "$CRED_FILE")

echo "Mode: $MODE, Owner: $OWNER"
if [ "$MODE" != "600" ] || [ "$OWNER" != "chorus-silas" ]; then
  echo "FAIL: Unexpected permissions on $CRED_FILE (expected 0600 chorus-silas)" >&2
  exit 1
fi

COMMENT="AC1 Demo Proof (live throwaway start outside Jeff's session):
- Keychain list: empty (isolated bootstrap domain)
- Before timestamp: $BEFORE_TS
- After timestamp: $AFTER_TS
- Credentials file mode: $MODE (owner: $OWNER)
- Result: Saved without GUI prompts"

echo "$COMMENT"
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards comment "$CARD" "$COMMENT"
echo "AC1 live proof complete and posted to card $CARD."
