#!/usr/bin/env bats
# @test-type: e2e — after the land: the live door, read-only (negative proofs run anywhere)
# @domain: domains
# @card: #5002
#
# #5002 AC3 — every door-listed row reads by name or sits in a named group (sampled ratchet). Split from no-ownerless-rows-live.bats so each AC names its own check.
# Skips in a card's werk: it measures what landed.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  DOOR="${ATHENA_MAKE_URL:-http://localhost:3360}"
}

in_werk() { case "${BATS_TEST_DIRNAME}" in */chorus-werk/*) return 0;; esac; return 1; }

unreadable_named() {
  python3 - "$1" "$2" <<'PY'
import sys
groups = [l.split("\t")[:3] for l in open(sys.argv[1]) if l.strip() and not l.startswith("#")]
def fits(rule, name):
    k, _, x = rule.partition(":")
    return {"prefix": name.startswith(x), "contains": x in name, "exact": name == x}.get(k, False) if x else \
        (rule == "any" or (rule == "uppercase-first" and name[:1].isupper()))
outside = []
for line in open(sys.argv[2]):
    cls, name, code = line.rstrip("\n").split("\t")
    if code == "200":
        continue
    if not any(c == "row-not-found-at-the-door" and k == cls and fits(r, name) for c, k, r in groups):
        outside.append(f"{cls}/{name} ({code})")
print("unreadable outside a named group: " + (", ".join(outside[:10]) or "none"))
sys.exit(1 if outside else 0)
PY
}

sample_reads() { # 5 listed rows per collection, read by name: Class<TAB>name<TAB>code
  python3 - "$DOOR" <<'PY'
import json, sys, urllib.request, urllib.parse
door = sys.argv[1]
for p in json.load(urllib.request.urlopen(door + "/", timeout=20))["primitives"]:
    try:
        names = [r["name"] for r in json.load(urllib.request.urlopen(door + p["collection"] + "?limit=5", timeout=40)).get("data", [])]
    except Exception:
        continue
    for n in names:
        try:
            urllib.request.urlopen(door + p["collection"] + "/" + urllib.parse.quote(n, safe=""), timeout=20); code = "200"
        except urllib.error.HTTPError as e:
            code = str(e.code)
        except Exception:
            code = "unanswered"
        print(f"{p['kind']}\t{n}\t{code}")
PY
}

@test "AC3 every door-listed row reads by name or sits in a named group (sampled)" {
  in_werk && skip "after-land check: a card's werk has not landed"
  sample_reads > "$BATS_TEST_TMPDIR/reads"
  [ -s "$BATS_TEST_TMPDIR/reads" ] || { echo "the door listed nothing"; return 1; }
  run unreadable_named "$ROOT/designing/domains/domains/validator-known-issues.tsv" "$BATS_TEST_TMPDIR/reads"
  echo "$(wc -l < "$BATS_TEST_TMPDIR/reads") reads; $output"
  [ "$status" -eq 0 ] || return 1
}

@test "AC3 NEGATIVE PROOF: an unreadable row outside every group is red; a grouped one and a readable one pass" {
  g="$ROOT/designing/domains/domains/validator-known-issues.tsv"
  printf 'Appointment\tappt-x\t404\nCard\tcard-1\t200\n' > "$BATS_TEST_TMPDIR/ok"
  run unreadable_named "$g" "$BATS_TEST_TMPDIR/ok"
  [ "$status" -eq 0 ] || return 1
  printf 'Appointment\tappointment-new\t404\n' > "$BATS_TEST_TMPDIR/bad"
  run unreadable_named "$g" "$BATS_TEST_TMPDIR/bad"
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"Appointment/appointment-new"* ]] || return 1
  printf 'Card\tcard-1\tunanswered\n' > "$BATS_TEST_TMPDIR/un"
  run unreadable_named "$g" "$BATS_TEST_TMPDIR/un"
  [ "$status" -eq 1 ] || { echo "an unanswered read is never counted as readable"; return 1; }
}
