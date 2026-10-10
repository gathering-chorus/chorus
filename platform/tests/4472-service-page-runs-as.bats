#!/usr/bin/env bats
# @test-type: unit — evaluates the pages' shared predicate from athena-flow.js, no browser, no store
# @domain: services
# #4472 — the service page's "Runs as" fold lists the crawled units whose runsService names
# this service. The door serves the edge as service-<name>; the page is opened by <name>.

setup() {
  ATHENA="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)/platform/api/public/athena"
  PAGE="$ATHENA/athena-flow.js"
}

# matches <served runsService> <page name> → prints true|false, using the predicate in the page
matches() {
  node -e '
    const src = require("fs").readFileSync(process.argv[1], "utf8");
    const m = src.match(/function runsServiceNames\(v, s\) \{[^\n]*\}/);
    if (!m) { console.log("predicate-missing"); process.exit(0); }
    console.log(String(eval("(" + m[0] + ")")(process.argv[2], process.argv[3])));
  ' "$PAGE" "$1" "$2"
}

@test "a unit the door serves as service-werk shows on the werk page" {
  [ "$(matches service-werk werk)" = "true" ]
}

@test "the bare name and a prefixed IRI still match" {
  [ "$(matches werk werk)" = "true" ]
  [ "$(matches chorus:werk werk)" = "true" ]
}

@test "negative proof: another service does not show (service-werkx, service-athena)" {
  [ "$(matches service-werkx werk)" = "false" ]
  [ "$(matches service-athena werk)" = "false" ]
}

@test "negative proof: a unit with no runsService does not show" {
  [ "$(matches '' werk)" = "false" ]
}

@test "both pages that list units use the shared predicate, not a copy" {
  grep -qF "runsServiceNames(r.runsService, s)" "$ATHENA/service.html"
  grep -qF "runsServiceNames(u.runsService, name)" "$ATHENA/domain.html"
  run grep -c "function runsServiceNames" "$ATHENA/service.html" "$ATHENA/domain.html"
  printf '%s\n' "$output" | grep -qx "$ATHENA/service.html:0"
  printf '%s\n' "$output" | grep -qx "$ATHENA/domain.html:0"
}

@test "the service page asks for its hosting domain under the row's name, in any graph" {
  grep -qF 'VALUES ?svc { chorus:${s} chorus:service-${s} } GRAPH ?g' "$ATHENA/service.html"
}

# state <runState> <lastExitCode> → the text the pages show for a unit
state() {
  node -e '
    const src = require("fs").readFileSync(process.argv[1], "utf8");
    const m = src.match(/function unitState\(u\) \{[\s\S]*?\n\}/);
    if (!m) { console.log("helper-missing"); process.exit(0); }
    const f = eval("(" + m[0] + ")");
    console.log(f({ runState: process.argv[2], lastExitCode: process.argv[3] }));
  ' "$PAGE" "$1" "$2"
}

@test "a job shows how its last run ended, an instance its run state" {
  [ "$(state '' 0)" = "last run ok" ]
  [ "$(state '' 78)" = "last run exit 78" ]
  [ "$(state running '')" = "running" ]
}

@test "negative proof: a job with no recorded exit is not shown as ok" {
  [ "$(state '' '')" = "no exit recorded" ]
}

@test "both pages show unit state through the shared helper" {
  grep -qF '${esc(unitState(r))}' "$ATHENA/service.html"
  grep -qF '${esc(unitState(u))}' "$ATHENA/domain.html"
}
