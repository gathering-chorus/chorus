#!/usr/bin/env bats
# @test-type: unit — hermetic: runs the deploy script's OWN audit query with arq
# over a fixture TTL. No Fuseki, no network, no live graph.
#
# #4011 — the practices deploy block was copy-pasted from PRINCIPLES_SET and the
# audit count kept asking for `c:Principle` against a graph that holds
# `c:Practice` individuals. A clean 18-practice deploy logged `0 principles
# live`, which is byte-identical to what a deploy that loaded nothing logs.
#
# That is a check that cannot distinguish the two states it exists to separate
# (#3734), so the proof below is not "does the file contain the right string" —
# a substring assert is the same weak shape. It extracts the query the script
# actually runs and executes it against two fixtures whose answers must differ.

SCRIPT="$BATS_TEST_DIRNAME/../scripts/athena-deploy-model.sh"

setup() {
  TMP="$BATS_TEST_TMPDIR"

  # Three practices, ZERO principles — the shape of the real practices graph.
  cat > "$TMP/practices.ttl" <<'EOF'
@prefix c: <https://jeffbridwell.com/chorus#> .
c:practice-pair-programming  a c:Practice ; c:expresses c:principle-obtain-a-yield .
c:practice-test-first        a c:Practice ; c:expresses c:principle-apply-self-regulation .
c:practice-small-releases    a c:Practice ; c:expresses c:principle-use-small-slow-solutions .
EOF

  # The same graph after a deploy that loaded nothing.
  printf '@prefix c: <https://jeffbridwell.com/chorus#> .\n' > "$TMP/empty.ttl"
}

# Pull the audit query out of the PRACTICES_SET block as the script will run it.
# Reading it from source is the point: if someone edits the script, this test
# follows the edit instead of asserting against a copy that can drift.
#
# FRAGILITY, named because both reviewers found it (#4011 gate): this is text
# surgery over one source line. It assumes the query stays on a single line
# inside the curl --data-urlencode argument. Split that line across two, or
# rename PRACTICES_GRAPH, and the extractor returns empty — the first test
# below fails FIRST and says so, so the breakage reads as "the extractor lost
# the query," not as a logic regression in the deploy. If you are here because
# test 1 went red, fix this function, not the script.
practices_audit_query() {
  awk '/^# PRACTICES_SET \(#3754\)/,0' "$SCRIPT" \
    | grep -o 'PREFIX c: <[^"]*COUNT(DISTINCT ?p)[^"]*}' \
    | head -1 \
    | sed 's|<\$PRACTICES_GRAPH>|?g|; s|GRAPH ?g ||; s|{ { |{ |; s|} }$|}|'
}

ask() {  # $1 = fixture ttl, $2 = query
  arq --data="$1" --query=/dev/stdin --results=csv <<<"$2" 2>/dev/null | tail -1 | tr -dc '0-9'
}

@test "the audit query is extractable from the practices block" {
  q="$(practices_audit_query)"
  [ -n "$q" ]
}

@test "negative proof: the audit counts the practices that are actually there" {
  # Before the fix this asks for c:Principle and answers 0 against a graph
  # holding three Practice individuals — a populated deploy reported as empty.
  q="$(practices_audit_query)"
  n="$(ask "$TMP/practices.ttl" "$q")"
  [ "$n" = "3" ]
}

@test "control: the same query answers 0 when nothing loaded — the states separate" {
  # Without this the fix could be a constant and still pass the proof above.
  q="$(practices_audit_query)"
  n="$(ask "$TMP/empty.ttl" "$q")"
  [ "$n" = "0" ]
}

@test "the practices block names practices, not principles, in every message" {
  block="$(awk '/^# PRACTICES_SET \(#3754\)/,0' "$SCRIPT")"
  ! grep -q 'merge staging->principles failed' <<<"$block"
  ! grep -q 'reason="principles-verify-missing"' <<<"$block"
  ! grep -q 'principles file(s)' <<<"$block"
  ! grep -q '\$_pn principles live' <<<"$block"
}

@test "the deployed-event field names practices so the spine is queryable by it" {
  block="$(awk '/^# PRACTICES_SET \(#3754\)/,0' "$SCRIPT")"
  grep -q 'model.deployed .* practices=' <<<"$block"
}

@test "the section separator above VALUES_SET is intact, not merged into a comment" {
  # Cosmetic, but it is what makes the blocks findable by eye in a 900-line file.
  ! grep -qE '^# ={10,}# ' "$SCRIPT"
}
