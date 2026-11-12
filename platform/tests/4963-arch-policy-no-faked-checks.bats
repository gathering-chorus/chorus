#!/usr/bin/env bats
# @test-type: contract — reads the plan and the counter; no store, no network.
# @domain: policies
# @card: #4963
# @owner: silas
#
# #4963 AC3 — no faked checks. Every rule a machine could not decide as written is narrowed
# until it can, and its row says so: the comment states "Checked as written" or "Narrowed"
# and what was left out. No check is a constant; each command check names a real counter
# in arch-policy-count, so it cannot be an echo that always passes. (No rule was retired.)

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
CHECKS="$ROOT/designing/data/policy-checks-4963.json"
COUNT="$ROOT/platform/scripts/arch-policy-count"

@test "every row says whether it is checked as written or narrowed, and none is retired" {
  run python3 -c "
import json, sys
bad = [r['name'] for r in json.load(open('$CHECKS'))
       if not ('Checked as written' in r['comment'] or 'Narrowed' in r['comment']) or r['status'] == 'retired']
print(bad); sys.exit(1 if bad else 0)"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

# every command check's spec runs the counter for its own rule, and the counter has that rule
specs_ok() {
  for name in $(python3 -c "import json; print(' '.join(r['name'] for r in json.load(open('$1')) if r['checkKind']=='command'))"); do
    spec=$(python3 -c "import json; print([r['checkSpec'] for r in json.load(open('$1')) if r['name']=='$name'][0])")
    [[ "$spec" == *'platform/scripts/arch-policy-count'*"\"\$p\" $name" ]] || { echo "$name: $spec"; return 1; }
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

@test "the SPARQL check selects from the skills, not a constant" {
  run python3 -c "
import json; r = [x for x in json.load(open('$CHECKS')) if x['checkKind'] == 'sparql']
assert len(r) == 1 and 'chorus:Skill' in r[0]['checkSpec'] and 'chorus:Product' in r[0]['checkSpec'], r"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}
