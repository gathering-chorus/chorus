#!/usr/bin/env bash
# @test-type: unit — validates credential fixtures against the shipped CredentialShape with Jena shacl; no store, no network
#
# #4344 — credentials are rows, never secrets. A row names where a credential is
# held (the file), what kind it is and when it last changed. Each rule is shown
# refusing (#3734): a token in a field, a 64-hex key in a field, an unknown kind.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
pass=0; fail=0
cat "$ROOT/roles/silas/ontology/chorus.ttl" "$ROOT/roles/silas/ontology/session-model-4302.ttl" > "$TMP/shapes.ttl"
cat > "$TMP/c.ttl" <<'TTL'
@prefix c: <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
c:principal-silas a c:Principal .
c:cred-good a c:Credential ; rdfs:label "silas css-client" ; c:scope "urn:chorus:identity" ; c:source "~/.chorus/identity/silas/cred.json" ; c:ownedBy c:principal-silas ; c:credentialKind "css-client" ; c:rotatedAt "2026-07-23T21:47:00Z" .
c:cred-token a c:Credential ; rdfs:label "a token in a field" ; c:scope "urn:chorus:identity" ; c:source "eyJhbGciOiJFUzI1NiJ9.eyJzdWIiOiJ4In0.sig" ; c:ownedBy c:principal-silas .
c:cred-key a c:Credential ; rdfs:label "a key in a field" ; c:scope "urn:chorus:identity" ; c:source "a42ccebeb2be92f57434f9d25243fb98be918f07752993283eeb95c98857c62a" ; c:ownedBy c:principal-silas .
c:cred-kind a c:Credential ; rdfs:label "an unknown kind" ; c:scope "urn:chorus:identity" ; c:source "~/x" ; c:ownedBy c:principal-silas ; c:credentialKind "password" .
TTL
rep=$(shacl validate --shapes "$TMP/shapes.ttl" --data "$TMP/c.ttl" 2>/dev/null | tr '\n' ' ' | sed 's/\[ *a *sh:ValidationResult/\n/g')
refused() { printf '%s\n' "$rep" | grep -F "chorus:$1" | grep -qF "$2"; }
if printf '%s\n' "$rep" | grep -qF "chorus:cred-good"; then echo "FAIL a credential that names only its file was refused"; fail=$((fail+1)); else echo "PASS a credential that names its file, kind and rotation conforms"; pass=$((pass+1)); fi
refused cred-token "never the secret" && { echo "PASS a token in any field is refused"; pass=$((pass+1)); } || { echo "FAIL a token in a field was not refused"; fail=$((fail+1)); }
refused cred-key "never the secret" && { echo "PASS a 64-hex key in any field is refused"; pass=$((pass+1)); } || { echo "FAIL a 64-hex key was not refused"; fail=$((fail+1)); }
refused cred-kind "CredentialShape-credentialKind" && { echo "PASS an unknown credential kind is refused"; pass=$((pass+1)); } || { echo "FAIL an unknown kind was not refused"; fail=$((fail+1)); }
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
