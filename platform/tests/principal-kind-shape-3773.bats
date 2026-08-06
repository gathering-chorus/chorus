#!/usr/bin/env bats
# @test-type: unit — signal:security is fixture-data; these validate a SHACL
# shape against synthetic principals, with no service, store or credential
# involved.
#
# #3773 — the Principal shape's new rules, each proven to REFUSE a violation.
#
# Why this file exists in this form: three rules were added to PrincipalShape
# (sign-in needs a webId, a worker cannot sign in, a worker holds no role). A
# rule that has never been shown to fail is decoration — it can be green because
# nothing violates it OR because it cannot detect a violation, and those two
# states are indistinguishable from the outside (#3734).
#
# So every test here plants the violation and asserts the check catches it. The
# happy-path assertions exist only to prove the checks are not simply refusing
# everything, which would pass every negative test while being just as useless.

setup() {
  SHACL="${SHACL_BIN:-shacl}"
  command -v "$SHACL" >/dev/null 2>&1 || skip "SHACL tool absent — cannot validate shapes here"
  ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
  SHAPES="$ROOT/roles/silas/ontology/security-model-3618.ttl"
  WORK="$(mktemp -d)"
  PRE='@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
'
}

teardown() { rm -rf "$WORK"; }

# Runs SHACL and returns 0 when the data CONFORMS, 1 when it is refused.
conforms() {
  local data="$1"
  "$SHACL" validate --shapes "$SHAPES" --data "$data" 2>/dev/null | grep -q "sh:conforms *true"
}

@test "a person who can sign in and has a webId conforms" {
  cat > "$WORK/ok.ttl" <<EOF
$PRE
chorus:principal-fixture-person a chorus:Principal ;
  rdfs:label "fixture person" ;
  chorus:principalKind "person" ;
  chorus:canSignIn "true" ;
  chorus:webId "https://example.test/person#me" .
EOF
  conforms "$WORK/ok.ttl"
}

@test "NEGATIVE PROOF: canSignIn=true without a webId is REFUSED" {
  # The half of the old blanket minCount that was right. Someone who can sign in
  # must have an identity to sign in AS; without it the record claims a login
  # that cannot exist.
  cat > "$WORK/bad.ttl" <<EOF
$PRE
chorus:principal-fixture-nowebid a chorus:Principal ;
  rdfs:label "fixture" ;
  chorus:principalKind "person" ;
  chorus:canSignIn "true" .
EOF
  ! conforms "$WORK/bad.ttl"
}

@test "NEGATIVE PROOF: a worker that can sign in is REFUSED" {
  # The rule the old shape could not express. A worker authenticates with a
  # service credential and never at a login screen; a worker marked canSignIn
  # would silently become eligible for the guard's allow-set.
  cat > "$WORK/bad.ttl" <<EOF
$PRE
chorus:principal-fixture-worker a chorus:Principal ;
  rdfs:label "fixture worker" ;
  chorus:principalKind "worker" ;
  chorus:canSignIn "true" ;
  chorus:webId "https://example.test/worker#me" .
EOF
  ! conforms "$WORK/bad.ttl"
}

@test "NEGATIVE PROOF: a worker holding a role is REFUSED" {
  # role != user (Jeff, 2026-08-06). A worker is a party that acts; it holds no
  # role, so it can never be an accepter or an owner.
  cat > "$WORK/bad.ttl" <<EOF
$PRE
chorus:principal-fixture-roleworker a chorus:Principal ;
  rdfs:label "fixture worker" ;
  chorus:principalKind "worker" ;
  chorus:canSignIn "false" ;
  chorus:holdsRole chorus:role-architect .
EOF
  ! conforms "$WORK/bad.ttl"
}

@test "NEGATIVE PROOF: an unrecognised principalKind is REFUSED" {
  # The enum has to be closed, or "wroker" is a silent fourth kind that no
  # conditional rule matches — and every worker rule then passes vacuously.
  cat > "$WORK/bad.ttl" <<EOF
$PRE
chorus:principal-fixture-typo a chorus:Principal ;
  rdfs:label "fixture" ;
  chorus:principalKind "wroker" ;
  chorus:canSignIn "false" .
EOF
  ! conforms "$WORK/bad.ttl"
}

@test "NEGATIVE PROOF: a missing principalKind is REFUSED" {
  # Absent is not "unknown". A principal whose kind is unmodelled cannot be
  # reasoned about by any rule that branches on kind.
  cat > "$WORK/bad.ttl" <<EOF
$PRE
chorus:principal-fixture-nokind a chorus:Principal ;
  rdfs:label "fixture" ;
  chorus:canSignIn "false" .
EOF
  ! conforms "$WORK/bad.ttl"
}

@test "a worker with no login and no role conforms — the shape is not refusing everything" {
  # Without this, every negative test above would also pass against a shape that
  # rejected all input. This is the control.
  cat > "$WORK/ok.ttl" <<EOF
$PRE
chorus:principal-fixture-goodworker a chorus:Principal ;
  rdfs:label "fixture worker" ;
  chorus:principalKind "worker" ;
  chorus:canSignIn "false" .
EOF
  conforms "$WORK/ok.ttl"
}

@test "webId is no longer mandatory for principals that cannot sign in" {
  # The defect this card fixes: the old shape demanded a webId on all ten
  # principals, eight of which have no account anywhere. It required credentials
  # for things that must never hold any.
  cat > "$WORK/ok.ttl" <<EOF
$PRE
chorus:principal-fixture-agent a chorus:Principal ;
  rdfs:label "fixture agent" ;
  chorus:principalKind "agent" ;
  chorus:canSignIn "false" .
EOF
  conforms "$WORK/ok.ttl"
}
