#!/usr/bin/env bash
# test-security-single-def-3735.sh — #3735: chorus:security must be defined in
# exactly ONE MODEL_SET file, and its resolved definesVocabulary must be the
# current 6 classes with NO stale V1 TrustMetric. RED while domains-wren-silas.ttl
# carries a competing stale block; GREEN once security-model-3618.ttl is the sole def.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WREN="$ROOT/roles/wren/ontology/domains-wren-silas.ttl"
SEC="$ROOT/roles/silas/ontology/security-model-3618.ttl"
FAIL=0; ok(){ echo "  PASS: $1"; }; no(){ echo "  FAIL: $1"; FAIL=1; }

# 1 — chorus:security defined in exactly ONE MODEL_SET file
defs=$(grep -lE '^chorus:security a ' "$WREN" "$SEC" 2>/dev/null | grep -c .)
[ "$defs" -eq 1 ] && ok "chorus:security defined in exactly 1 MODEL_SET file" || no "chorus:security defined in $defs files (competing definition)"
# 2 — the surviving def is security-model-3618 (mine), and it carries APISurface+Gate
grep -qE '^chorus:security a ' "$SEC" && ok "authoritative def is security-model-3618" || no "security-model-3618 lost its def"
# 3 — no file in the security def carries the stale V1 TrustMetric on security's vocab
if grep -A2 '^chorus:security a ' "$WREN" 2>/dev/null | grep -q TrustMetric; then
  no "stale V1 TrustMetric still on a chorus:security definesVocabulary"
else
  ok "no stale TrustMetric on any chorus:security definition"
fi
echo; [ $FAIL -eq 0 ] && { echo PASS; exit 0; } || { echo RED; exit 1; }
