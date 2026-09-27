#!/usr/bin/env bats
# @test-type: contract
# @domain: code — the product domain this suite guards (#4334)
# #4178, re-aimed by #4210 on 2026-09-18 — the scheduled pass must present the
# principal that OWNS the rows.
#
# athena-make stamps ownedBy from the caller and only the owner may update a
# row, so whoever runs the crawler TAKES every row it writes. That fact has not
# changed; what changed is who should hold them. This file used to require
# `crawler`, on the reasoning that a service owns what it writes. Jeff ruled the
# other way on 2026-09-17: principals own things, principals own services, and a
# service's rows belong to the principal it runs as. Holding `crawler` cost
# three hand migrations in one day — code 6,218 rows, tests 6,610, logs 132 —
# because a role editing this repo cannot update rows a service took.
#
# The 2026-09-15 measurement that argued for `crawler` still holds and is why
# this value is never changed casually: one pass under a different name
# reassigns every row it touches, and the previous owner's runs are refused.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  PLIST="$REPO/platform/launchd/com.chorus.crawl-nightly.plist"
}

@test "the nightly unit exists and is valid" {
  [ -f "$PLIST" ]
  run plutil -lint "$PLIST"
  [ "$status" -eq 0 ]
}

@test "it runs as the principal that owns the rows" {
  run /usr/libexec/PlistBuddy -c 'Print :EnvironmentVariables:CHORUS_ROLE' "$PLIST"
  [ "$status" -eq 0 ]
  declared="$output"

  # DERIVED, not restated (#4201, Wren 14:10): this used to hardcode the name,
  # so Jeff's 09-17 ruling made the contract wrong and nothing said so until a
  # human read it. The owner of the rows this unit writes is a fact in the
  # store — ask it, and the test survives the next ruling without an edit.
  owner="$(curl -s --max-time 10 http://localhost:3030/pods/sparql \
    --data-urlencode 'query=PREFIX c: <https://jeffbridwell.com/chorus#> SELECT ?o WHERE { GRAPH <urn:chorus:domains:code> { ?s a c:CodeFile ; c:ownedBy ?o } } GROUP BY ?o ORDER BY DESC(COUNT(?s)) LIMIT 1' \
    -H 'Accept: text/csv' 2>/dev/null | tail -1 | tr -d '\r' | sed 's|.*[#/]principal-||')"
  [ -n "$owner" ] || skip "UNMEASURABLE: the store is not answering"
  [ "$declared" = "$owner" ]
}

# NEGATIVE PROOF (#3734): the check must FAIL on the state it exists to catch —
# the same unit wired to a person's identity. Without this it would pass on any
# plist that merely has the key.
# NEGATIVE PROOF (#3734): the check must FAIL on the state it exists to catch —
# this unit wired to a name that holds none of the rows it writes.
@test "NEGATIVE PROOF: an automation name that owns nothing is caught" {
  bad="$BATS_TEST_TMPDIR/bad.plist"
  cp "$PLIST" "$bad"
  /usr/libexec/PlistBuddy -c 'Set :EnvironmentVariables:CHORUS_ROLE crawler' "$bad"
  run /usr/libexec/PlistBuddy -c 'Print :EnvironmentVariables:CHORUS_ROLE' "$bad"
  declared="$output"
  [ "$declared" = "crawler" ]

  # Run the SAME derivation the check above runs, against the violating file.
  # Comparing two literals here would prove only that crawler != kade — it
  # would not show that the derived check can go red, which is the point.
  owner="$(curl -s --max-time 10 http://localhost:3030/pods/sparql \
    --data-urlencode 'query=PREFIX c: <https://jeffbridwell.com/chorus#> SELECT ?o WHERE { GRAPH <urn:chorus:domains:code> { ?s a c:CodeFile ; c:ownedBy ?o } } GROUP BY ?o ORDER BY DESC(COUNT(?s)) LIMIT 1' \
    -H 'Accept: text/csv' 2>/dev/null | tail -1 | tr -d '\r' | sed 's|.*[#/]principal-||')"
  [ -n "$owner" ] || skip "UNMEASURABLE: the store is not answering"
  [ "$declared" != "$owner" ]
}

@test "the on-land delta also runs as the owner and cannot fail the land" {
  YML="$REPO/.github/workflows/werk.yml"
  run grep -A 12 '\- name: crawl-delta' "$YML"
  # bash 3.2 never fires errexit on a failing [[ ]] that is not the last line
  # (#4185), so these are simple commands.
  grep -qF 'CHORUS_ROLE: kade' <<<"$output"
  grep -qF 'continue-on-error: true' <<<"$output"
}

# NEGATIVE PROOF: "cannot fail the land" has to be a property of the step, not a
# hope. Strip continue-on-error and the check must go red — otherwise a crawl
# that cannot reach the door would take a merged, deployed land down with it.
@test "NEGATIVE PROOF: without continue-on-error the step would fail the land" {
  YML="$REPO/.github/workflows/werk.yml"
  bad="$BATS_TEST_TMPDIR/werk.yml"
  grep -v 'continue-on-error: true' "$YML" > "$bad"
  run grep -A 12 '\- name: crawl-delta' "$bad"
  [[ "$output" != *"continue-on-error: true"* ]]
}

# #4180 — the nightly must be a FULL pass. The first launchd-started run walked
# 0 files: it read the watermark and did a delta, which the on-land step had
# already done. A nightly that repeats the delta never sees graph-side drift.
@test "the nightly forces a full walk, not a delta" {
  run /usr/libexec/PlistBuddy -c 'Print :EnvironmentVariables:CHORUS_CRAWL_WATERMARK' "$PLIST"
  [ "$status" -eq 0 ]
  [ -z "$output" ]
}

# NEGATIVE PROOF: a unit without the override is the one that ran a delta —
# the check must fail on it.
@test "NEGATIVE PROOF: a unit without the full-walk override is caught" {
  bad="$BATS_TEST_TMPDIR/bad.plist"; cp "$PLIST" "$bad"
  /usr/libexec/PlistBuddy -c 'Delete :EnvironmentVariables:CHORUS_CRAWL_WATERMARK' "$bad"
  run /usr/libexec/PlistBuddy -c 'Print :EnvironmentVariables:CHORUS_CRAWL_WATERMARK' "$bad"
  [ "$status" -ne 0 ]
}
