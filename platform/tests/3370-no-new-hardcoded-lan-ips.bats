#!/usr/bin/env bats
# @test-type: unit — static repo grep, no live service
# 3370-no-new-hardcoded-lan-ips.bats — regression guard (#3370)
# What Jeff sees: a machine can change address (DHCP drift, hub unplugged —
# the 2026-06-12 incident) and nothing goes silently dark, because no NEW
# code pins a 192.168.86.x address. Legacy pins are baselined and may only
# SHRINK (the test-hardcoded-bin-paths.sh ratchet pattern).

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
# #3904 — scan governs CODE. Captured data (.nt graph dumps, .backup dbs) and
# prose/rendered surfaces (.md/.html/.ejs) may legitimately CONTAIN addresses;
# pinning behavior lives in scripts/services, which stay fully governed.
BASELINE="$REPO/platform/tests/3370-lan-ip-baseline.txt"

@test "no NEW hardcoded 192.168.86.x references (shrink-only ratchet)" {
  [ -f "$BASELINE" ]
  fails=""
  while IFS=: read -r file allowed; do
    [ -z "$file" ] && continue
    actual=$(grep -c "192\.168\.86\." "$REPO/$file" 2>/dev/null) || actual=0
    if [ "$actual" -gt "$allowed" ]; then
      fails="$fails $file($actual>$allowed)"
    fi
  done < "$BASELINE"
  # any file NOT in the baseline must have zero references
  while IFS= read -r hit; do
    grep -q "^${hit}:" "$BASELINE" || fails="$fails NEW:$hit"
  done < <(grep -rl "192\.168\.86\." "$REPO/platform" "$REPO/proving" "$REPO/designing" "$REPO/.github" 2>/dev/null \
    | grep -vE "\.git|node_modules|/dist|/coverage|/logs/|/target/|board-snapshot|baseline" \
    | grep -vE "/backups/|\.nt$|\.backup$|\.md$|\.html$|\.ejs$" \
    | sed "s|^$REPO/||")
  [ -z "$fails" ] || { echo "ratchet violations:$fails"; false; }
}
