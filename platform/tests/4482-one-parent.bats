#!/usr/bin/env bats
# @test-type: unit — reads the pages and the model files; no store
# @domain: domains
#
# #4482 — one parent per row. A Domain's step is its parent; a Product or Service stores
# no step and the pages read it up the chain. primaryStep and atStream are retired, and
# removing a word from the model removes it from the code that named it.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
A="$ROOT/platform/api/public/athena"

steps() { # js expression over the helpers in athena-flow.js
  node -e "
    const src = require('fs').readFileSync('$A/athena-flow.js', 'utf8');
    const pick = n => src.match(new RegExp('function ' + n + '\\\\([\\\\s\\\\S]*?\\\\n}'))[0];
    eval(['asArray', 'stepLocal', 'bareLocal', 'chainSteps', 'productSteps', 'serviceSteps'].map(pick).join('\n'));
    const domains = [
      { name: 'cicd', atStep: 'https://jeffbridwell.com/chorus#value-stream-step-building' },
      { name: 'tests', atStep: 'value-stream-step-proving', hosts: ['service-tests'] },
      { name: 'cards', atStep: 'value-stream-step-directing', hosts: 'service-clearing' } ];
    console.log(JSON.stringify($1));"
}

@test "a product's steps are its domains' steps, never a step it stores" {
  run steps "productSteps({ hasDomain: ['cicd', 'chorus:tests'], atStep: 'value-stream-step-shaping' }, domains)"
  [ "$status" -eq 0 ]
  [ "$output" = '["building","proving"]' ] || { echo "got: $output"; false; }
}

@test "a service's step is the step of the domain that hosts it, by either name" {
  run steps "[serviceSteps('tests', domains), serviceSteps('service-clearing', domains), serviceSteps('nobody', domains)]"
  [ "$output" = '[["proving"],["directing"],[]]' ] || { echo "got: $output"; false; }
}

@test "the product, products, value-stream, service and services pages read the step up the chain" {
  for p in product products value-stream; do grep -q "productSteps(" "$A/$p.html" || { echo "$p.html"; false; }; done
  for p in service services; do grep -q "serviceSteps(" "$A/$p.html" || { echo "$p.html"; false; }; done
  # negative proof: none of them reads a step stored on the product or service row
  run grep -nE "stepBadge\((p|e|links|s\._links)\.atStep\)|stepLocal\((p|e)\.atStep\)|_links\.atStep" \
    "$A/product.html" "$A/products.html" "$A/value-stream.html" "$A/service.html" "$A/services.html"
  [ "$status" -ne 0 ] || { echo "$output"; false; }
}

retired_in() { grep -lE '\b(primaryStep|atStream)\b' "$@"; }

@test "primaryStep and atStream are gone from the code, pages and model files that named them" {
  run retired_in \
    "$A/atlas-mock.html" "$ROOT/platform/api/public/chorus-data-model.html" "$ROOT/platform/api/public/chorus-instance-explorer.js" \
    "$ROOT/platform/api/src/server.ts" "$ROOT/platform/api/tests/fixtures/athena-minimal.ttl" \
    "$ROOT/platform/scripts/4353-domain-rename.sh" "$ROOT/platform/scripts/products-3603-migrate.sh" \
    "$ROOT/roles/wren/artifacts/roles-domain-design.html" "$ROOT/roles/wren/ontology/awareness-4018.ttl" \
    "$ROOT/roles/wren/ontology/board-3654.ttl" "$ROOT/roles/wren/ontology/domains-wren-silas.ttl" \
    "$ROOT/roles/kade/ontology/domains-kade-3581.ttl" "$ROOT/designing/data/product-instances.ttl"
  [ "$status" -ne 0 ] || { echo "still named in: $output"; false; }
}

@test "the generated Domain door serves atStep as its one step link and no retired link" {
  # full paths, literal: the pipeline maps a changed file to the tests that name it
  grep -q '"atStep|edge:ValueStreamStep"' "$ROOT/platform/services/athena-make/generated/routes-domain.json"
  run retired_in "$ROOT/platform/services/athena-make/generated/routes-domain.json" \
    "$ROOT/platform/services/athena-make/generated/openapi-domain.json" \
    "$ROOT/platform/services/athena-make/generated/dashboard-domain.json"
  [ "$status" -ne 0 ] || { echo "still generated in: $output"; false; }
}

@test "negative proof: the word check finds a retired link when one is there" {
  f="$BATS_TEST_TMPDIR/x.ttl"
  printf 'chorus:d a chorus:Domain ;\n    chorus:atStream chorus:value-stream-chorus .\n' > "$f"
  run retired_in "$f"
  [ "$status" -eq 0 ]
}
