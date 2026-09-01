#!/usr/bin/env bats
# @test-type: integration
#
# #4010 — the identity block of a design doc is a PROJECTION, and the check that
# says so must be able to fail.
#
# The failure this replaces: service-design-pulse.html carried 15,682 bytes of
# hand-typed identity and referenced the model zero times, so nothing could tell
# you whether its description of Pulse still matched what Pulse IS. A generator
# alone does not fix that — a generator plus a drift check does, and the drift
# check is only worth having if it has been seen to go red.

GEN="$(cd "${BATS_TEST_DIRNAME}/.." && pwd)/scripts/design-doc-identity.py"

setup() { TMP="$(mktemp -d)"; }
teardown() { rm -rf "$TMP"; }

@test "the block is generated from the model, not from the file" {
  run python3 "$GEN" --class Product --name pulse
  [ "$status" -eq 0 ]
  [[ "$output" == *"BEGIN generated-identity"* ]]
  [[ "$output" == *"Pulse"* ]]
  [[ "$output" == *"/products/pulse"* ]]
}

@test "a doc whose block matches the model PASSES" {
  { echo "<h1>Pulse</h1>"; python3 "$GEN" --class Product --name pulse; echo "<p>hand-written below</p>"; } > "$TMP/doc.html"
  run python3 "$GEN" --class Product --name pulse --check "$TMP/doc.html"
  [ "$status" -eq 0 ]
}

# NEGATIVE PROOF 1 — drifted identity must RED. The fixture is the real block
# with one field edited, which is exactly how a doc rots: someone updates the
# prose and the model moves on without it.
@test "a doc whose block DRIFTED from the model fails" {
  { echo "<h1>Pulse</h1>"; python3 "$GEN" --class Product --name pulse; } > "$TMP/doc.html"
  cp "$TMP/doc.html" "$TMP/doc.before"
  # the drift: a stale purpose line, the shape this card was filed about.
  # It used to sed on 'Agent vital signs', which the model no longer says — so
  # the edit was a no-op and this test failed because the FIXTURE never drifted,
  # not because the check was broken. Edit the label, then prove it changed.
  sed -i '' 's#<td>Pulse</td>#<td>Pulse and long-term recall</td>#' "$TMP/doc.html"
  ! cmp -s "$TMP/doc.html" "$TMP/doc.before"
  run python3 "$GEN" --class Product --name pulse --check "$TMP/doc.html"
  [ "$status" -eq 1 ]
  [[ "$output" == *"DRIFT"* ]]
}

# NEGATIVE PROOF 2 — a doc with NO block must RED too. Otherwise the check
# passes vacuously on every document that never adopted it, which would let the
# gate report green across a corpus it has never actually read.
@test "a doc carrying no generated block fails, rather than passing vacuously" {
  echo "<h1>Pulse</h1><p>all hand-written</p>" > "$TMP/doc.html"
  run python3 "$GEN" --class Product --name pulse --check "$TMP/doc.html"
  [ "$status" -eq 1 ]
  [[ "$output" == *"no generated-identity block"* ]]
}

# NEGATIVE PROOF 3 — an identity the model does not hold must REFUSE, not
# invent. A generator that emits an empty block for an unknown subject would
# happily document things that do not exist.
@test "an unknown subject refuses instead of emitting an empty block" {
  run python3 "$GEN" --class Product --name no-such-product-4010
  [ "$status" -ne 0 ]
  [[ "$output" == *"no identity"* ]] || [[ "$output" == *"did not answer"* ]]
}
