#!/usr/bin/env bash
# Test: chorus-reference-model.html v2 architecture asserts (5-layer + subproducts)
# Per Jeff direction 2026-05-01: Engine layer decomposes into Werk/Borg/Athena/Convergence/Loom/Clearing as subproducts; layer count grows from 4 to 5 (Human/Role/Protocol/Subproduct/Engine).

set -euo pipefail
DOC="$(dirname "$0")/chorus-reference-model.html"

fail() { echo "FAIL: $1" >&2; exit 1; }
pass=0

# AC1: file exists
[ -f "$DOC" ] || fail "doc not found at $DOC"
pass=$((pass+1))

# AC2: section II opens with "Five layers" not "Four layers"
grep -q "Five layers" "$DOC" || fail "section II should say 'Five layers' (currently 'Four layers')"
pass=$((pass+1))

# AC3: Subproduct layer block present
grep -q 'class="layer subproduct"' "$DOC" || fail "missing <div class=\"layer subproduct\"> block"
pass=$((pass+1))

# AC4: all six subproducts named in the subproduct layer block
for name in Athena Werk Convergence Borg Loom Clearing; do
  grep -q "$name" "$DOC" || fail "subproduct '$name' not named in doc"
done
pass=$((pass+1))

# AC5: CSS rule for .layer.subproduct present
grep -q '\.layer\.subproduct \.layer-label' "$DOC" || fail "missing CSS rule for .layer.subproduct"
pass=$((pass+1))

# AC6: Engine layer text repositions engines as underneath subproducts (mentions 'subproduct' nearby)
grep -q -i "swappable" "$DOC" || fail "Engine layer should describe engines as swappable beneath subproducts"
pass=$((pass+1))

echo "PASS: $pass/6 assertions"
