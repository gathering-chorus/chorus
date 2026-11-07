#!/usr/bin/env bats
# @test-type: unit
# @domain: knowledge
# @card: #4709
# @owner: wren
# #4709 (child of #4702) — the 9 facts #4702 retired must be in no model source,
# or a deploy puts them back. Read as parsed triples from every ontology and data
# file: roles/silas/ontology/chorus.ttl, roles/*/ontology/*.ttl, designing/data/*.ttl.

ROOT="$BATS_TEST_DIRNAME/../.."
C="https://jeffbridwell.com/chorus#"

retired() {
  printf '%s\n' \
    "<${C}borg> <${C}consumes> <${C}alerts>" \
    "<${C}borg> <${C}consumes> <${C}infrastructure>" \
    "<${C}borg> <${C}consumes> <${C}monitors>" \
    "<${C}borg> <${C}consumes> <${C}deploys-domain>" \
    "<${C}borg> <${C}consumes> <${C}logs-domain>" \
    "<${C}chorus> <${C}hasDomain> <${C}tests>" \
    "<${C}pulse> <${C}consumes> <${C}streams>" \
    "<${C}deploys> <${C}supports> <${C}cicd>" \
    "<${C}observability> <${C}hasDomain> <${C}time>"
}

# Every retired triple found in the given files, as "file triple". A file riot cannot read fails the check.
found_in() {
  local f nt
  for f in "$@"; do
    nt=$(riot --output=nt "$f" 2>/dev/null) || { echo "UNREADABLE $f"; return 2; }
    while IFS= read -r t; do grep -qF "$t" <<<"$nt" && echo "$f $t"; done < <(retired)
  done
  return 0
}

sources() { ls "$ROOT"/roles/*/ontology/*.ttl "$ROOT"/designing/data/*.ttl; }

@test "no model source carries any of the 9 retired facts" {
  run found_in $(sources)
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ -z "$output" ] || { echo "$output"; return 1; }
  [ "$(sources | wc -l)" -gt 20 ] || { echo "too few source files read"; return 1; }
}

@test "negative proof: a retired fact planted back in a source file is found" {
  { cat "$ROOT/roles/silas/ontology/chorus.ttl"; printf '\nchorus:borg chorus:consumes chorus:alerts .\n'; } > "$BATS_TEST_TMPDIR/planted.ttl"
  run found_in "$BATS_TEST_TMPDIR/planted.ttl"
  [[ "$output" == *"<${C}borg> <${C}consumes> <${C}alerts>"* ]] || { echo "$output"; return 1; }
}

@test "negative proof: an unreadable source file fails rather than reading clean" {
  printf 'not turtle <<<\n' > "$BATS_TEST_TMPDIR/broken.ttl"
  run found_in "$BATS_TEST_TMPDIR/broken.ttl"
  [ "$status" -eq 2 ] || { echo "status=$status $output"; return 1; }
}
