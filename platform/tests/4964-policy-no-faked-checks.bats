#!/usr/bin/env bats
# @test-type: contract — reads the plan and the counter; no store, no network.
# @domain: policies
# @card: #4964
# @owner: silas
#
# #4964 AC3 — no faked checks. Every rule a machine could not decide as written is narrowed
# until it can, and its row says so: the comment states "Narrowed" and what was left out. No
# rule moved out of the card (Jeff 20:09: all 15 stay) and none is retired. No check is a
# constant: each command check runs the one counter (policy-count) for its own rule, and each
# SPARQL check selects from the rows its rule governs.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
CHECKS="$ROOT/designing/domains/policies/policy-checks-4964.json"
COUNT="$ROOT/platform/scripts/policy-count"

@test "all 15 are in, none retired, and every row names its narrowing" {
  run python3 -c "
import json, sys
r = json.load(open('$CHECKS'))
bad = [x['name'] for x in r if 'Narrowed' not in x['comment'] or x['status'] == 'retired' or 'Narrowed: he ' in x['comment']]
print(len(r), bad); sys.exit(0 if len(r) == 15 and not bad else 1)"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

specs_ok() {
  for name in $(python3 -c "import json; print(' '.join(r['name'] for r in json.load(open('$1')) if r['checkKind']=='command'))"); do
    spec=$(python3 -c "import json; print([r['checkSpec'] for r in json.load(open('$1')) if r['name']=='$name'][0])")
    [[ "$spec" == *'platform/scripts/policy-count'*"\"\$p\" $name" ]] || { echo "$name: $spec"; return 1; }
    grep -q "\"$name\": " "$COUNT" || { echo "no counter for $name"; return 1; }
  done
}

@test "every command check runs its own counter, and the counter knows that rule" {
  run specs_ok "$CHECKS"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "negative proof: a constant check (true), or one pointing at another rule's counter, is refused" {
  python3 -c "
import json; r = json.load(open('$CHECKS')); c = [x for x in r if x['checkKind'] == 'command']
c[0]['checkSpec'] = 'true'
json.dump(r, open('$BATS_TEST_TMPDIR/faked.json', 'w'))
c[0]['checkSpec'] = c[1]['checkSpec']
json.dump(r, open('$BATS_TEST_TMPDIR/swapped.json', 'w'))"
  run specs_ok "$BATS_TEST_TMPDIR/faked.json"
  [ "$status" -eq 1 ] || return 1
  run specs_ok "$BATS_TEST_TMPDIR/swapped.json"
  [ "$status" -eq 1 ] || return 1
}

@test "each SPARQL check selects from the rows its rule governs, not a constant" {
  run python3 -c "
import json
want = {'data-domain-is-punned-owl-class': ['chorus:Domain', 'owl:Class'],
        'data-property-key-registered': ['chorus:Property', 'chorus:PropertyKey'],
        'qual-domain-fitness-functions': ['chorus:Domain', 'chorus:FitnessFunction'],
        'prod-private-by-default-graduation': ['jb:hasVisibility', 'urn:gathering:']}
r = {x['name']: x['checkSpec'] for x in json.load(open('$CHECKS')) if x['checkKind'] == 'sparql'}
assert sorted(r) == sorted(want), sorted(r)
for n, terms in want.items():
    assert all(t in r[n] for t in terms) and 'SELECT' in r[n], n"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}
