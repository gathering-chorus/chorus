#!/usr/bin/env bats
# @test-type: unit — evaluates the page's own predicate from the file, no browser, no store
# @domain: services
# #4472 — the service page's "Runs as" fold lists the crawled units whose runsService names
# this service. The door serves the edge as service-<name>; the page is opened by <name>.

setup() {
  PAGE="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)/platform/api/public/athena/service.html"
}

# matches <served runsService> <page name> → prints true|false, using the predicate in the page
matches() {
  node -e '
    const src = require("fs").readFileSync(process.argv[1], "utf8");
    const m = src.match(/const runsServiceNames = (\(v, s\) => \{[^\n]*\});/);
    if (!m) { console.log("predicate-missing"); process.exit(0); }
    console.log(String(eval(m[1])(process.argv[2], process.argv[3])));
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
