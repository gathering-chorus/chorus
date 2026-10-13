#!/usr/bin/env bats
# @test-type: unit
# #4509 — a product's parent is on-model. Seven product rows carried partOf from the
# seed while ProductShape never declared it, so the door refused moving Convergence
# under Athena as off-model (422, Kade 2026-10-12 20:34). The model the generators
# read (athena-gen) must show partOf as one link to a Product.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  GEN="${ATHENA_GEN_BIN:-$ROOT/designing/services/athena-gen/target/release/athena-gen}"
  [ -x "$GEN" ] || GEN="${CHORUS_HOME:-$HOME/CascadeProjects/chorus}/designing/services/athena-gen/target/release/athena-gen"
  [ -x "$GEN" ] || GEN="$(command -v athena-gen || true)"
  [ -n "$GEN" ] || skip "athena-gen not built — UNMEASURED"
}

partof_on_product() {
  "$GEN" model --root "$1" --product athena 2>/dev/null \
    | python3 -c 'import sys,re; t=sys.stdin.read(); m=re.search(r"name: \"Product\",(.*?)\n                \},\n", t, re.S); print("yes" if m and re.search(r"name: \"partOf\",\s*kind: Link \{\s*class: Some\(\s*\"Product\"", m.group(1)) else "no")'
}

@test "ProductShape declares partOf as a link to a Product" {
  run partof_on_product "$ROOT"
  [ "$output" = "yes" ]
}

@test "NEGATIVE PROOF: without the #4509 declaration, partOf is not on Product" {
  tmp="$(mktemp -d)"
  mkdir -p "$tmp/roles" "$tmp/designing"
  cp -R "$ROOT/roles/." "$tmp/roles/"
  cp -R "$ROOT/designing/data" "$tmp/designing/"
  python3 - "$tmp/roles/wren/ontology/product-design-4018.ttl" <<'PY'
import sys,re
p=sys.argv[1]; s=open(p).read()
s=re.sub(r' ;\n    # #4509.*?sh:description "The parent product, if this product is a child of another." \] \.', ' .', s, flags=re.S)
open(p,'w').write(s)
PY
  run partof_on_product "$tmp"
  rm -rf "$tmp"
  [ "$output" = "no" ]
}
