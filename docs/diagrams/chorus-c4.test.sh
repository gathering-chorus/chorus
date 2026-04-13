#!/bin/bash
# C4 diagram HTML verification — #1991
# Tests that Jeff can open the page and see all three C4 levels

HTML="$(dirname "$0")/chorus-c4.html"
PASS=0; FAIL=0

check() {
  if eval "$2" >/dev/null 2>&1; then
    echo "PASS: $1"; ((PASS++))
  else
    echo "FAIL: $1"; ((FAIL++))
  fi
}

check "HTML file exists" "[ -f '$HTML' ]"
check "Contains L1 context diagram" "grep -q 'C4Context' '$HTML'"
check "Contains L2 container diagram" "grep -q 'C4Container' '$HTML'"
check "Contains L3 component diagram" "grep -q 'C4Component' '$HTML'"
check "Loads Mermaid JS library" "grep -q 'mermaid' '$HTML'"
check "Has navigation links for all levels" "grep -q '#context' '$HTML' && grep -q '#container' '$HTML' && grep -q '#component' '$HTML'"
check "No Slack reference" "! grep -qi 'slack' '$HTML'"
check "Shows actual source files (grounded in code)" "grep -q 'main.rs' '$HTML' && grep -q 'nudge.rs' '$HTML'"

echo ""
echo "Results: $PASS pass, $FAIL fail"
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
