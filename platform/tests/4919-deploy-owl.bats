#!/usr/bin/env bats
# @test-type: contract — runs SHACL over fixtures written to BATS_TEST_TMPDIR against
# the repo's DeployTarget shape (roles/silas/ontology/deploy-targets-4919.ttl); no store, no network.
# @domain: werk
# #4919 AC1 — a deploy target declares what should exist in prod: path, owner
# account, file mode, source, health check and kind. 2026-11-07 showed the cost
# of not declaring it: principals drifted to hostAccount=jeffbridwell (01:18) and
# nostr.json keys drifted to mode 660 (07:10), and nothing knew what "right" was.
#
# Each NEGATIVE proof (#3734) puts a row in the state the shape exists to refuse
# and shows SHACL non-conformance; the control shows a well-formed row conforms.

setup() {
  REPO="${CHORUS_ROOT:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}"
  TTL="$REPO/roles/silas/ontology/deploy-targets-4919.ttl"
  SHAPES="$BATS_TEST_TMPDIR/shapes.nt"
  riot --output=nt "$TTL" 2>/dev/null \
    | grep -E '^<https://jeffbridwell\.com/chorus#DeployTargetShape>|^_:' \
    | grep -vE 'shacl#(sparql|node)>' > "$SHAPES" || true
  PREFIX='@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
'
  KEY='chorus:path "/var/chorus/identity/silas/nostr.json" ; chorus:ownerAccount "jeffbridwell" ; chorus:source "minted, never in git" ; chorus:targetKind chorus:target-kind-secret'
}

verdict() {
  local out
  out=$(shacl validate --shapes "$SHAPES" --data "$1" 2>&1)
  if echo "$out" | grep -q "sh:conforms *true"; then echo "CONFORMS"
  elif echo "$out" | grep -q "sh:conforms *false"; then echo "VIOLATES"
  else echo "ERROR"; fi
}

fixture() { printf '%s%s\n' "$PREFIX" "$1" > "$BATS_TEST_TMPDIR/$2.ttl"; echo "$BATS_TEST_TMPDIR/$2.ttl"; }

@test "the DeployTarget shape is in the model, with required fields" {
  grep -q '#DeployTargetShape>' "$SHAPES" || { echo "no DeployTargetShape in $TTL"; return 1; }
  grep -q 'minCount' "$SHAPES" || return 1
}

@test "control: a secret target with path, owner, mode 0600, source, health check and kind conforms" {
  f=$(fixture "chorus:target-silas-nostr a chorus:DeployTarget ; $KEY ; chorus:fileMode \"0600\" ; chorus:healthCheck \"stat -f %Lp = 600\" ." ok)
  [ "$(verdict "$f")" = CONFORMS ] || return 1
}

@test "negative proof: a target with no file mode is refused" {
  f=$(fixture "chorus:target-silas-nostr a chorus:DeployTarget ; $KEY ; chorus:healthCheck \"stat -f %Lp = 600\" ." nomode)
  [ "$(verdict "$f")" = VIOLATES ] || return 1
}

@test "negative proof: a target with no health check is refused" {
  f=$(fixture "chorus:target-silas-nostr a chorus:DeployTarget ; $KEY ; chorus:fileMode \"0600\" ." nohealth)
  [ "$(verdict "$f")" = VIOLATES ] || return 1
}

@test "negative proof: a file mode that is not four octal digits is refused (660 is not 0660)" {
  f=$(fixture "chorus:target-silas-nostr a chorus:DeployTarget ; $KEY ; chorus:fileMode \"660\" ; chorus:healthCheck \"stat -f %Lp = 600\" ." badmode)
  [ "$(verdict "$f")" = VIOLATES ] || return 1
}

@test "negative proof: a target with no owner account is refused" {
  f=$(fixture "chorus:t a chorus:DeployTarget ; chorus:path \"/x\" ; chorus:source \"git\" ; chorus:targetKind chorus:target-kind-config ; chorus:fileMode \"0644\" ; chorus:healthCheck \"test -f /x\" ." noowner)
  [ "$(verdict "$f")" = VIOLATES ] || return 1
}
