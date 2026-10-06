#!/usr/bin/env bats
# @test-type: unit — runs the generator against a fixture API response and a temp registry; no live services
# @domain: events — the product domain this suite guards
# #4438 — spine-events.json's events are generated from the EventType rows the
# store serves (GET /owl/v1/events/types). These tests feed the generator a
# fixture shaped like that response.
#
# Negative proof (#3734): a registry hand-edited after generation makes
# --check exit 1 and name the event; a response that holds fewer rows than its
# count is refused rather than written as a partial registry.

GEN="$BATS_TEST_DIRNAME/../scripts/event-types-generate.py"

setup() {
  T="$(mktemp -d)"
  cat > "$T/rows.json" <<'JSON'
{ "count": 2, "data": [
  { "eventName": "card.pulled", "producedBy": "cards", "eventAbout": "https://jeffbridwell.com/chorus#Card",
    "eventCategory": "fact", "eventVersion": "1", "atVertebra": "building", "emitterSource": "chorus-events",
    "comment": "Card pulled to WIP", "joinKeys": "card_id", "payloadField": "card_id: Card identifier" },
  { "eventName": "hook.decision", "producedBy": "https://jeffbridwell.com/chorus#gates",
    "eventCategory": "diagnostic", "eventVersion": "1",
    "payloadField": ["hook: Dispatch point", "decision: allow or deny"] }
] }
JSON
  printf '{"@domain":"spine","product_map":{"chorus-events":"Chorus"},"events":{}}\n' > "$T/registry.json"
  export EVENT_TYPES_URL="file://$T/rows.json" EVENT_TYPES_REGISTRY="$T/registry.json"
}

teardown() { rm -rf "$T"; }

@test "writes each served row as a registry event, keeping the header" {
  run python3 "$GEN"
  [ "$status" -eq 0 ] || return 1
  run python3 -c "
import json; d = json.load(open('$T/registry.json'))
assert d['product_map'] == {'chorus-events': 'Chorus'}, d
e = d['events']['card.pulled']
assert e == {'producer': 'cards', 'about': 'Card', 'category': 'fact', 'version': '1', 'vertebra': 'building',
             'source': 'chorus-events', 'description': 'Card pulled to WIP', 'keys': ['card_id'],
             'fields': {'card_id': 'Card identifier'}}, e
h = d['events']['hook.decision']
assert h['producer'] == 'gates' and h['fields'] == {'decision': 'allow or deny', 'hook': 'Dispatch point'}, h
"
  [ "$status" -eq 0 ] || return 1
}

@test "--check passes on a freshly generated registry" {
  python3 "$GEN"
  run python3 "$GEN" --check
  [ "$status" -eq 0 ] || return 1
}

@test "NEGATIVE: a hand edit to the registry makes --check fail and name the event" {
  python3 "$GEN"
  python3 -c "
import json; p = '$T/registry.json'; d = json.load(open(p))
d['events']['made.up.event'] = {'fields': {}}; json.dump(d, open(p, 'w'))"
  run python3 "$GEN" --check
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"made.up.event"* ]] || return 1
}

@test "NEGATIVE: a response holding fewer rows than its count is refused, nothing written" {
  sed -i '' 's/"count": 2/"count": 5/' "$T/rows.json"
  run python3 "$GEN"
  [ "$status" -ne 0 ] || return 1
  [[ "$output" == *"never write a partial registry"* ]] || return 1
  run python3 -c "import json; assert json.load(open('$T/registry.json'))['events'] == {}"
  [ "$status" -eq 0 ] || return 1
}
