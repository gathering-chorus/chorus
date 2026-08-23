#!/usr/bin/env bats
# @test-type: unit — signal:security is fixture-data; validates a SHACL shape
# against synthetic credentials; no service, store, or real key involved.
#
# #3691 — the Nostr-credential shape, each rule proven to REFUSE a violation
# (#3734: a rule never shown to fail is decoration). The shape file existed for
# 18 days while nothing enforced it; entering MODEL_SET is the other half, and
# a deploy-set assertion below pins that this file cannot silently leave it.

setup() {
  SHACL="${SHACL_BIN:-shacl}"
  command -v "$SHACL" >/dev/null 2>&1 || skip "SHACL tool absent — cannot validate shapes here"
  ROOT="${CHORUS_ROOT:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}"
  SHAPES="$ROOT/roles/silas/ontology/nostr-credential-shape-3691.ttl"
  WORK="$(mktemp -d)"
  PRE='@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
chorus:silas a chorus:Principal .
'
}

teardown() { rm -rf "$WORK"; }

@test "a well-formed nostr credential validates green" {
  cat > "$WORK/ok.ttl" <<TTL
${PRE}chorus:key-silas-nostr a chorus:KeyRegistryEntry ;
  chorus:forPrincipal chorus:silas ;
  chorus:keyType "nostr-secp256k1" ;
  chorus:keyId "SILAS_NOSTR_KEY" ;
  chorus:nostrPubkey "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" .
TTL
  run "$SHACL" validate --shapes "$SHAPES" --data "$WORK/ok.ttl"
  [[ "$output" == *"sh:conforms  true"* ]]
}

@test "NEGATIVE: an unowned credential (no forPrincipal) REFUSES" {
  cat > "$WORK/unowned.ttl" <<TTL
${PRE}chorus:key-orphan a chorus:KeyRegistryEntry ;
  chorus:keyType "nostr-secp256k1" ;
  chorus:nostrPubkey "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" .
TTL
  run "$SHACL" validate --shapes "$SHAPES" --data "$WORK/unowned.ttl"
  [[ "$output" == *"sh:conforms  false"* ]]
  [[ "$output" == *"parallel identity"* ]]
}

@test "NEGATIVE: a keyId carrying a VALUE-looking string (not an env-var NAME) REFUSES" {
  cat > "$WORK/leak.ttl" <<TTL
${PRE}chorus:key-leak a chorus:KeyRegistryEntry ;
  chorus:forPrincipal chorus:silas ;
  chorus:keyType "nostr-secp256k1" ;
  chorus:keyId "nsec1qqqsyqcyq5rqwzqfpg9scrgwpugpzysn" ;
  chorus:nostrPubkey "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" .
TTL
  run "$SHACL" validate --shapes "$SHAPES" --data "$WORK/leak.ttl"
  [[ "$output" == *"sh:conforms  false"* ]]
  [[ "$output" == *"ENV-VAR NAME"* ]]
}

@test "NEGATIVE: a malformed pubkey (wrong length / case) REFUSES" {
  cat > "$WORK/badkey.ttl" <<TTL
${PRE}chorus:key-bad a chorus:KeyRegistryEntry ;
  chorus:forPrincipal chorus:silas ;
  chorus:keyType "nostr-secp256k1" ;
  chorus:nostrPubkey "DEADBEEF" .
TTL
  run "$SHACL" validate --shapes "$SHAPES" --data "$WORK/badkey.ttl"
  [[ "$output" == *"sh:conforms  false"* ]]
}

@test "the shape is IN the deploy set — declared is not deployed (#3691's whole lesson)" {
  grep -q "nostr-credential-shape-3691.ttl" "$ROOT/platform/scripts/athena-deploy-model.sh"
}
