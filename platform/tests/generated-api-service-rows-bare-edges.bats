#!/usr/bin/env bats
# @test-type: unit — platform/scripts/land-5042-rows against a fixture door (LAND_LIVE_JSON); no store, no runner
# @domain: services
# @card: #5042 AC6 · owner: kade
#
# Jeff said yes at 16:26 to reopen #5042: Wren's apply after the land was refused 4 of 4 with
# "double-prefix: 'document-athena-product-design' already starts with 'document-' — pass the bare
# name (ADR-040 Rule 0)". land-5042-rows writes every edge as its bare name (hasDesignDoc
# athena-product-design; hosts pulse-api, principles-api), and the door mints the prefix.
# NEGATIVE: a planned edge value holding its kind's prefix is named by the check.
# (Its own file: the demo refuses two AC sharing one check; AC2's is generated-api-service-rows.bats.)
#
# Covers: platform/scripts/land-5042-rows

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
LAND="$ROOT/platform/scripts/land-5042-rows"
MAP="$ROOT/platform/config/service-instance-map.json"
PRINCIPLES="$ROOT/shaping/domains/principles/api/surfaces/service.json"
PULSE="$ROOT/directing/products/pulse/api/surfaces/service.json"
STALE="Services: principles-api serves this domain (com.chorus.principles-api, #5017), but no Service row names it yet, so chorus:hosts has nothing to point at."

setup() {
  T="$(mktemp -d)"
  printf '#!/bin/bash\necho called >> "%s/runner.log"\n' "$T" > "$T/runner"
  chmod +x "$T/runner"
}
teardown() { rm -rf "$T"; }

# the fixture door: $1 = "none" | "done" Service rows, $2 = hosts on principles, $3 = "stale" | "kept" note
door() {
  python3 - "$PRINCIPLES" "$PULSE" "$1" "$2" "$3" "$STALE" > "$T/live.json" <<'PY'
import json, sys
principles, pulse, service, hosts, notes, stale = sys.argv[1:7]
facts = [json.load(open(principles)), json.load(open(pulse))]
kept = ["Decisions: 0 because no decision carries hasDomain yet; #4464 (decisions cleanup) owns that."]
domains = [
    {"name": "principles", "ownedBy": "principal-wren", "hosts": hosts, "foldNote": kept + ([stale] if notes == "stale" else [])},
    {"name": "awareness", "ownedBy": "principal-wren", "hosts": "service-pulse-api" if service == "done" else ""},
]
services = [dict(f["service"]) for f in facts] if service == "done" else []
print(json.dumps({"services": services, "domains": domains}))
PY
  LAND_LIVE_JSON="$T/live.json" LAND_SERVICE_FILES="$PRINCIPLES:$PULSE" CHORUS_RUNNER="$T/runner" run python3 "$LAND" --as "${AS:-wren}"
}

@test "#5042 NEGATIVE: every edge is written bare; a prefixed one (the door's double prefix) is red" {
  door none "" stale
  test "$status" -eq 0 || { echo "$output"; return 1; }
  # every edge value the plan writes, one per line: "<field> <value>" (the printed plan is cut at 160 characters)
  edges() {
    LAND_LIVE_JSON="$T/live.json" LAND_SERVICE_FILES="$PRINCIPLES:$PULSE" python3 - "$LAND" <<'PY'
import importlib.machinery, importlib.util, sys
loader = importlib.machinery.SourceFileLoader("land5042", sys.argv[1])
m = importlib.util.module_from_spec(importlib.util.spec_from_loader("land5042", loader)); loader.exec_module(m)
for _, _, w, _ in m.full_plan(m.load_live()):
    for f in ("hasDesignDoc", "hosts"):
        for v in (w.get(f) if isinstance(w.get(f), list) else [w.get(f)] if f in w else []):
            print(f, v)
PY
  }
  # the check itself: an edge value holding its kind's prefix is named
  prefixed() { grep -E '^(hasDesignDoc document-|hosts service-)'; }
  run edges
  test "$status" -eq 0 || { echo "$output"; return 1; }
  echo "$output" | grep -qx 'hasDesignDoc athena-product-design' || { echo "no bare hasDesignDoc: $output"; return 1; }
  echo "$output" | grep -qx 'hosts pulse-api' || { echo "no bare hosts edge: $output"; return 1; }
  ! echo "$output" | prefixed || { echo "a prefixed edge is planned: $output"; return 1; }
  echo 'hasDesignDoc document-athena-product-design' | prefixed >/dev/null || { echo "the check did not name a prefixed hasDesignDoc"; return 1; }
  echo 'hosts service-pulse-api' | prefixed >/dev/null || { echo "the check did not name a prefixed hosts edge"; return 1; }
}
