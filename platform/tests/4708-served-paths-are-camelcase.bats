#!/usr/bin/env bats
# @test-type: unit
# @domain: ontology
# @card: #4708
# @owner: wren
# #4708 — on 10-23 #4702 gave DomainShape a path <urn:borg:ontology/usesEnvironment>.
# The door names a field by the text after the IRI's last '#'
# (platform/services/athena-make/src/lib.rs, rsplit('#')), which for a slash
# IRI is the whole IRI: not camelCase, so adr040_check refused the class and
# prod lost /v1/domains/domains. This reads roles/silas/ontology/chorus.ttl the
# same way and fails on any served shape whose path has no camelCase name.

ROOT="$BATS_TEST_DIRNAME/../.."
SHAPES="$ROOT/roles/silas/ontology/chorus.ttl"

# Every (shape, path) on a shape with a target class: the routes the door serves.
# A query that fails (unparseable TTL, no sparql) fails here, never reads as "no bad paths".
bad_paths() {
  local rows
  rows=$(sparql --data "$1" --results=TSV --query <(cat <<'Q'
PREFIX sh: <http://www.w3.org/ns/shacl#>
SELECT ?shape ?path WHERE { ?shape sh:targetClass ?c ; sh:property ?b . ?b sh:path ?path FILTER(isIRI(?path)) }
Q
)) || { echo "SPARQL FAILED on $1"; return 2; }
  [ "$(printf '%s\n' "$rows" | wc -l)" -gt 100 ] || { echo "TOO FEW PATHS READ from $1"; return 2; }
  printf '%s\n' "$rows" | tail -n +2 | tr -d '<>' | while IFS=$'\t' read -r shape path; do
    name="${path##*#}"
    [[ "$name" =~ ^[a-z][A-Za-z0-9]*$ ]] || echo "${shape##*#} $path"
  done
}

@test "every path on a served shape has a camelCase field name, so no class loses its route" {
  command -v sparql >/dev/null || { echo "sparql (Jena) not installed"; return 1; }
  run bad_paths "$SHAPES"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ -z "$output" ] || { echo "$output"; return 1; }
}

@test "usesEnvironment is ignored by DomainShape, not a path, so its 9 rows still pass the closed shape" {
  sparql --data "$SHAPES" --results=TSV --query <(printf '%s\n' 'PREFIX sh: <http://www.w3.org/ns/shacl#>' 'PREFIX c: <https://jeffbridwell.com/chorus#>' 'ASK { c:DomainShape sh:ignoredProperties/<http://www.w3.org/1999/02/22-rdf-syntax-ns#rest>*/<http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:borg:ontology/usesEnvironment> }') | grep -q true || return 1
}

@test "negative proof: an unreadable shapes file fails rather than reading clean" {
  printf 'this is not turtle <<<\n' > "$BATS_TEST_TMPDIR/broken.ttl"
  run bad_paths "$BATS_TEST_TMPDIR/broken.ttl"
  [ "$status" -eq 2 ] || { echo "status=$status $output"; return 1; }
}

@test "negative proof: the 10-23 path, planted back on DomainShape, is named" {
  { cat "$SHAPES"; printf '\nchorus:DomainShape sh:property [ sh:path <urn:borg:ontology/usesEnvironment> ] .\n'; } > "$BATS_TEST_TMPDIR/planted.ttl"
  run bad_paths "$BATS_TEST_TMPDIR/planted.ttl"
  [[ "$output" == *"DomainShape urn:borg:ontology/usesEnvironment"* ]] || { echo "$output"; return 1; }
}
