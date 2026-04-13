#!/usr/bin/env bats
# C4 diagram verification — #1991

HTML="$BATS_TEST_DIRNAME/chorus-c4.html"

@test "HTML file exists" {
  [ -f "$HTML" ]
}

@test "Contains L1 context diagram" {
  grep -q 'C4Context' "$HTML"
}

@test "Contains L2 container diagram" {
  grep -q 'C4Container' "$HTML"
}

@test "Contains L3 component diagram" {
  grep -q 'C4Component' "$HTML"
}

@test "Loads Mermaid JS library" {
  grep -q 'mermaid' "$HTML"
}

@test "Has navigation for all levels" {
  grep -q '#context' "$HTML"
  grep -q '#container' "$HTML"
  grep -q '#component' "$HTML"
}

@test "No Slack reference" {
  ! grep -qi 'slack' "$HTML"
}

@test "Grounded in source files" {
  grep -q 'main.rs' "$HTML"
  grep -q 'nudge.rs' "$HTML"
}
