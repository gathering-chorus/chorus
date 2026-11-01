#!/usr/bin/env bats
# @test-type: unit — reads the repo's own platform/config/canonical-surfaces.json; no live service
# @domain: spine
# @card: 4850 · owner: wren
# What Jeff sees: the #3424 bounded-surface list (which werk verbs and role
# skills may exist) can't be broken by an edit that touches only the list.
# Kade 2026-11-01 09:36: a json-only edit ran no test, because the Rust test that
# parses it is not a suite the pipeline selects by path. This suite names the
# file, so an edit to it alone runs this.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  LIST="$ROOT/platform/config/canonical-surfaces.json"
}

check_list() { # FILE — the guard's rule: both axes present, non-empty, lowercase-hyphen names
  python3 -I - "$1" <<'PY'
import json, re, sys
d = json.load(open(sys.argv[1]))
for key in ("verbs", "skills"):
    v = d.get(key)
    if not isinstance(v, list) or not v:
        sys.exit(f"'{key}' is missing or empty")
    bad = [x for x in v if not isinstance(x, str) or not re.fullmatch(r"[a-z0-9]+(-[a-z0-9]+)*", x)]
    if bad:
        sys.exit(f"'{key}' has names off the rule: {bad}")
    if len(set(v)) != len(v):
        sys.exit(f"'{key}' repeats a name")
print("ok", len(d["verbs"]), len(d["skills"]))
PY
}

@test "AC6: platform/config/canonical-surfaces.json holds both axes, mob among the skills" {
  run check_list "$LIST"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  python3 -c 'import json,sys; sys.exit(0 if "mob" in json.load(open(sys.argv[1]))["skills"] else 1)' "$LIST" || { echo "mob is not a canonical skill"; return 1; }
}

@test "AC6 NEGATIVE PROOF: an emptied axis, a broken file, a repeat or an off-rule name is refused" {
  B="$BATS_TEST_TMPDIR"
  python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); d["skills"]=[]; json.dump(d,open(sys.argv[2],"w"))' "$LIST" "$B/empty.json"
  run check_list "$B/empty.json"; [ "$status" -ne 0 ] || { echo "empty passed"; return 1; }
  head -c 40 "$LIST" > "$B/broken.json"
  run check_list "$B/broken.json"; [ "$status" -ne 0 ] || { echo "broken passed"; return 1; }
  python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); d["skills"].append(d["skills"][0]); json.dump(d,open(sys.argv[2],"w"))' "$LIST" "$B/dup.json"
  run check_list "$B/dup.json"; [ "$status" -ne 0 ] || { echo "repeat passed"; return 1; }
  python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); d["skills"].append("Mob_Flow"); json.dump(d,open(sys.argv[2],"w"))' "$LIST" "$B/case.json"
  run check_list "$B/case.json"; [ "$status" -ne 0 ] || { echo "off-rule name passed"; return 1; }
}
