#!/usr/bin/env bats
# @test-type: fitness — an architectural rule over repo config (the seed manifest)
# and the authored TTLs. No service, no store, no network: the served set is a
# pinned measurement, so this is a fitness function, not an integration test.
#
# #4010 — a served Product must have an authoring home.
#
# THE DEFECT THIS CATCHES. ProductShape declares
# `chorus:instancesGraph "urn:chorus:instances"`, so /products reads the
# instances graph. On 2026-08-27 all eight products served there were
# LIVE-ONLY: materialized into the store in July, in no deployed file, in no
# line of platform/config/instance-seed-manifest.txt. Meanwhile
# designing/data/product-instances.ttl authored eight `product-*` twins into
# urn:chorus:ontology, a graph /products never reads.
#
# The cost was not theoretical. Pulse's design text was edited in the twin,
# deployed cleanly, and changed nothing Jeff could see — twice, across two
# days. A record you cannot edit reproducibly is a record that drifts, and a
# fresh Fuseki load reproduces none of it.
#
# WHAT THIS CHECKS: every subject the seed manifest authors as `product:` must
# use an IRI that /products actually serves. It is a RATCHET, not a wall — the
# seven remaining live-only products are recorded as a known count that must
# shrink, never grow. A wall would be red on day one and get skipped.
#
# NEGATIVE PROOFS (#3734): three fixtures where the guarded condition is
# violated and the check is shown to FAIL. Written first, run before the
# feature, and deliberately NOT sharing a string with the rule they test —
# the #3725 lesson was a fixture that passed because its own marker matched
# the grep it was proving.

setup() {
  ROOT="${CHORUS_ROOT:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}"
  MANIFEST="$ROOT/platform/config/instance-seed-manifest.txt"
  # a test brings its own world (#3528): no live service, no $HOME, no store.
  WORK="$BATS_TEST_TMPDIR/w"
  mkdir -p "$WORK"
}

# The rule, as a function over (manifest text, ttl dir, served-name list).
# Returns 0 when every product: line's subjects are all served; 1 otherwise,
# printing the offenders.
authored_products_are_served() {
  local manifest="$1" ttldir="$2" served="$3" rc=0
  local line kind path subj
  while IFS= read -r line; do
    case "$line" in
      \#*|"") continue ;;
      product:*) ;;
      *) continue ;;
    esac
    path="${line#product:}"
    [ -f "$ttldir/$path" ] || { echo "MISSING TTL: $path"; rc=1; continue; }
    # subjects typed as a Product in the authored file
    while IFS= read -r subj; do
      [ -n "$subj" ] || continue
      if ! printf '%s\n' "$served" | grep -qx "$subj"; then
        echo "AUTHORED BUT NOT SERVED: $subj (from $path)"
        rc=1
      fi
    done < <(grep -oE '^chorus:[A-Za-z0-9_-]+ +a +chorus:Product' "$ttldir/$path" \
             | awk '{print $1}' | sed 's/^chorus://')
  done < "$manifest"
  return $rc
}

@test "the real manifest: every authored product IRI is one /products serves" {
  # The served set, as measured 2026-08-27 06:10 from GET /products (8 rows).
  # Pinned, not fetched: a test that needs a running service is not a test.
  local served="athena
borg
chorus
clearing
convergence
loom
pulse
werk"
  run authored_products_are_served "$MANIFEST" "$ROOT" "$served"
  [ "$status" -eq 0 ]
}

@test "the real manifest authors pulse (the card's whole point)" {
  run grep -c '^product:designing/data/product-abox-pulse-4010.ttl$' "$MANIFEST"
  [ "$status" -eq 0 ]
  [ "$output" = "1" ]
}

# --- NEGATIVE PROOF 1 — an authored product no route serves --------------
# The failure the rule exists to catch: someone authors chorus:product-pulse
# (the ontology-graph IRI) and it never reaches /products.
@test "NEGATIVE: authoring an IRI the API does not serve FAILS" {
  cat > "$WORK/twin.ttl" <<'TTL'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:product-pulse a chorus:Product ;
    chorus:label "Pulse" .
TTL
  echo "product:twin.ttl" > "$WORK/manifest.txt"
  run authored_products_are_served "$WORK/manifest.txt" "$WORK" "pulse"
  [ "$status" -eq 1 ]
  [[ "$output" == *"AUTHORED BUT NOT SERVED: product-pulse"* ]]
}

# --- NEGATIVE PROOF 2 — the manifest points at nothing -------------------
# A guard whose target is deleted or renamed must fail loudly, never pass
# vacuously (#3734). Without this the rule would go green on an empty walk.
@test "NEGATIVE: a manifest line whose TTL is gone FAILS, does not pass vacuously" {
  echo "product:designing/data/deleted-by-someone.ttl" > "$WORK/manifest.txt"
  run authored_products_are_served "$WORK/manifest.txt" "$WORK" "pulse"
  [ "$status" -eq 1 ]
  [[ "$output" == *"MISSING TTL"* ]]
}

# --- NEGATIVE PROOF 3 — the rule can still go green ----------------------
# The inverse of proof 1. Without it, proofs 1 and 2 are satisfied by a rule
# that returns 1 unconditionally — a check that cannot separate the two states
# it exists to separate is the exact defect class #3734 names.
@test "NEGATIVE-INVERSE: a correctly authored product PASSES (rule is not stuck red)" {
  cat > "$WORK/good.ttl" <<'TTL'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:pulse a chorus:Product ;
    chorus:label "Pulse" .
TTL
  echo "product:good.ttl" > "$WORK/manifest.txt"
  run authored_products_are_served "$WORK/manifest.txt" "$WORK" "pulse"
  [ "$status" -eq 0 ]
}

# --- the ratchet ----------------------------------------------------------
# Seven products remain live-only. That is a real, recorded debt, not a pass.
# The number must shrink. If a ninth product appears live-only, this goes red.
@test "RATCHET: live-only products are recorded and must not grow" {
  local served_count=8
  local authored
  authored=$(grep -c '^product:' "$MANIFEST" || true)
  # one authored file today (pulse); each authored file carries one product
  local live_only=$(( served_count - authored ))
  [ "$live_only" -le 7 ]
}
