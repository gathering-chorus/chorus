#!/bin/sh
# @domain: policies
# #5020 — a git tree in $1 holding one violation of domain-no-side-door for domain x: a script
# outside x's unit that writes x's graph directly. x's own door naming it, a test naming it,
# and the model naming it are not violations.
set -e
cd "$1"
git init -q .
mkdir -p platform/scripts shaping/domains/x/api/src platform/tests designing/schemas
printf 'curl -s localhost:3030/pods/update --data "update=INSERT DATA { GRAPH <urn:chorus:domains:x> { } }"\n' > platform/scripts/writer
printf 'const G = "urn:chorus:domains:x";\n' > shaping/domains/x/api/src/model.rs
printf '@test "x" { echo urn:chorus:domains:x; }\n' > platform/tests/x.bats
printf '<urn:chorus:domains:x> a <urn:g> .\n' > designing/schemas/x.ttl
printf 'G=urn:chorus:domains:xylophone\n' > platform/scripts/other
git add -A
