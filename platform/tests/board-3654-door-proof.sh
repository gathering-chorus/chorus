#!/usr/bin/env bash
# board-3654-door-proof.sh — the LIVE, prove-by-running door proof for #3654.
#
# Proves the coordination primitive actually HOLDS: uniqueness-within-scope refusals
# fire at the DAL write door (not decorative SHACL). Three refusals, one idiom:
#   • rank          unique within inChunk   (card order within a chunk)
#   • roleSequence  unique within ownedBy    (chunk order per role)
#   • loomSequence  unique class-wide        (cross-role loom axis)
#
# RED until BOTH land:
#   1. board-3654.ttl deployed into urn:chorus:ontology (chorus-model-deploy.sh) so
#      read_shape sees the uniqueWithin/uniqueGlobal annotations, AND
#   2. chorus-model rebuilt with Silas's enforcement primitive (#3681 — read_shape
#      +2 maps, write() +2 sibling-ASK loops).
# Until then dup writes SUCCEED and the refusal assertions FAIL — that is the correct
# TDD red (DEC-1674). Green on all three = the load-bearing proof; projection follows.
#
# ownedBy targets the PREFIXED Role instance (role-wren), not bare chorus:wren — the
# DAL edge-type ASK is direct-type and only role-* is typed chorus:Role (verified
# 2026-07-24). role-wren / role-kade must exist (they do: ADR-054's 4 Role instances).
set -u

CM="${CHORUS_MODEL_BIN:-$(command -v chorus-model 2>/dev/null || echo ./target/release/chorus-model)}"
# Isolated throwaway graph — the uniqueness ASK scopes to req.graph (lib.rs:462/506),
# so both the writes and the sibling-checks land here, never in live urn:chorus:instances.
# Drop it after: curl -X POST .../update --data-urlencode "update=DROP GRAPH <$PROOF_GRAPH>"
PROOF_GRAPH="${PROOF_GRAPH:-urn:chorus:proof-board-$$}"
GA=(--graph "$PROOF_GRAPH")
PASS=0; FAIL=0
S="A#$$"   # unique suffix so the proof is re-runnable without collisions

ok(){ echo "  ✓ $1"; PASS=$((PASS+1)); }
no(){ echo "  ✗ $1"; FAIL=$((FAIL+1)); }

# expect_ok  <label> <chorus-model add args...>   → command must exit 0
expect_ok(){ local l="$1"; shift; if "$CM" add "$@" "${GA[@]}" >/dev/null 2>&1; then ok "$l"; else no "$l (write refused, expected success)"; fi; }

# expect_refusal <label> <needle> -- <add args...> → must exit!=0 AND stderr ~ needle
expect_refusal(){
  local l="$1" needle="$2"; shift 2; [ "$1" = "--" ] && shift
  local err rc
  err="$("$CM" add "$@" "${GA[@]}" 2>&1)"; rc=$?
  if [ $rc -eq 0 ]; then no "$l (write SUCCEEDED — uniqueness not enforced)"
  elif echo "$err" | grep -qi "$needle"; then ok "$l"
  else no "$l (refused, but not on '$needle': $err)"; fi
}

echo "board #3654 door proof — binary: $CM"
echo "proof graph (isolated): $PROOF_GRAPH"

# ── prerequisites (must succeed) ─────────────────────────────────────────────
expect_ok "card stub card-$S-1"      --kind card  --name "$S-1" --field label="Door Proof Card 1"
expect_ok "card stub card-$S-2"      --kind card  --name "$S-2" --field label="Door Proof Card 2"
expect_ok "chunk alpha (roleSeq 1, loomSeq 1)" \
  --kind chunk --name "alpha-$S" --field label="Alpha" --field slug="alpha-$S" \
  --field roleSequence=1 --field loomSequence=1 --edge ownedBy=role:wren
expect_ok "membership rank 1 in alpha" \
  --kind chunkmembership --name "alpha-$S-1" --field rank=1 \
  --edge inChunk=chunk:alpha-$S --edge hasCard=card:$S-1

# ── REFUSAL 1 — dup rank within the same chunk ───────────────────────────────
expect_refusal "dup rank within inChunk refused" "rank" -- \
  --kind chunkmembership --name "alpha-$S-2" --field rank=1 \
  --edge inChunk=chunk:alpha-$S --edge hasCard=card:$S-2

# ── REFUSAL 2 — dup roleSequence within the same role ────────────────────────
expect_refusal "dup roleSequence within ownedBy refused" "roleSequence" -- \
  --kind chunk --name "beta-$S" --field label="Beta" --field slug="beta-$S" \
  --field roleSequence=1 --edge ownedBy=role:wren

# ── REFUSAL 3 — dup loomSequence class-wide (different role, same ordinal) ────
expect_refusal "dup loomSequence global refused" "loomSequence" -- \
  --kind chunk --name "gamma-$S" --field label="Gamma" --field slug="gamma-$S" \
  --field roleSequence=1 --field loomSequence=1 --edge ownedBy=role:kade

echo "── $PASS passed, $FAIL failed ──"
[ $FAIL -eq 0 ]
