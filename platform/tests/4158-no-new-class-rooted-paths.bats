#!/usr/bin/env bats
# @test-type: fitness — a repo-wide check over source text; no service, no store, no network.
# #4158 AC4 — no CALLER may hardcode a class-rooted athena-make path.
#
# This replaces a mention-counter that could not do the job. That guard grepped
# for the bare strings and carried a ceiling that ROSE across this card
# (76 -> 79), because every negative proof has to NAME /v1/testresults in order
# to assert it is not handed back. A check that cannot tell an assertion from a
# call cannot certify that callers moved — it was measuring the wrong thing, so
# it is replaced rather than retuned.
#
# What a CALL looks like: a class-rooted path on a URL or URL-bearing variable
# ("http://host:3360/testresults", "$URL/codefiles", "${BASE}/logsources") that
# is HANDED TO AN HTTP CLIENT on the same line — curl, fetch(, requests., .get(,
# .post(, http(. Both halves are needed. URL-shape alone was tried first and
# flagged seven lines that are not callers: `assert_eq!(pre,
# "http://h:1/v1/testresults/batch")` in this card's own tests, and units.rs
# fixtures feeding a pure pagination function. Those strings are never
# requested. A fixture that names a URL is still a mention.
#
# ROOT: this check measures the tree it TRAVELS WITH, from its own location —
# deliberately NOT $CHORUS_ROOT. Run 83 (2026-09-13) failed here: prove-live set
# CHORUS_ROOT to canonical, so the guard graded canonical's pre-land copy of the
# callers and reported 9. A source-fitness check pointed at a different checkout
# than the diff it guards can never pass before the land, and says nothing true
# about the change under test.
#
# The rule callers follow instead: ASK the server which collection it serves —
# its discovery document advertises it — the way the crawler (chorus-crawl) always has.
# A pre-#4158 server answers /v1/testresults and a post-#4158 one answers
# /v1/tests/results, so a caller that asks is right on both and needs no
# land-ordering. Hardcoding either literal is what lost 650 of 650 test results
# in run 77 (2026-09-13).

CLASS_ROOTED='(https?://[^"'"'"' ]*|\$\{?[A-Z_]+\}?)/(codefiles|codekinds|testresults|testsuiteruns|logsources)\b'

# The one caller still permitted a literal, with its reason:
#   werk-test/src/main.rs — the FALLBACK used only when discovery is unreadable.
#   It degrades to the alias rather than inventing a route.
ALLOWED='platform/services/werk-test/src/main.rs'

# The second half of a call: the URL reaches an HTTP client on this line.
REQUESTED='curl|fetch\(|requests\.|\.get\(|\.post\(|http\('

count_calls() {
  cd "$BATS_TEST_DIRNAME/../.." || return 1
  grep -rnE "$CLASS_ROOTED" \
    --include='*.ts' --include='*.js' --include='*.sh' --include='*.py' \
    --include='*.rs' --include='*.bats' --include='*.yml' . 2>/dev/null \
    | grep -vE 'node_modules|/dist/|target/|chorus-werk|4158-no-new-class-rooted' \
    | grep -E "$REQUESTED" \
    | grep -vE "$ALLOWED" \
    | wc -l | tr -d ' '
}

list_calls() {
  cd "$BATS_TEST_DIRNAME/../.." || return 1
  grep -rnE "$CLASS_ROOTED" \
    --include='*.ts' --include='*.js' --include='*.sh' --include='*.py' \
    --include='*.rs' --include='*.bats' --include='*.yml' . 2>/dev/null \
    | grep -vE 'node_modules|/dist/|target/|chorus-werk|4158-no-new-class-rooted' \
    | grep -E "$REQUESTED" \
    | grep -vE "$ALLOWED"
}

@test "no caller hardcodes a class-rooted athena-make path" {
  n=$(count_calls)
  [ "$n" -eq 0 ] || {
    echo "hardcoded class-rooted CALLS: $n (expected 0)"
    list_calls
    echo "Ask the server instead: read the collection its discovery document"
    echo "advertises for the class, the way chorus-crawl does."
    false
  }
}

@test "NEGATIVE PROOF: the guard fails on a real call and ignores a mere mention" {
  # The two states this check exists to SEPARATE. The old one could not: it went
  # red for both, so it could never reach zero and never certified anything.
  cd "$BATS_TEST_DIRNAME/../.."
  before=$(count_calls)
  [ "$before" -eq 0 ]

  # (a) a real call — must go RED
  probe="platform/tests/.4158-probe-call-$$.sh"
  printf 'curl -s "http://localhost:3360/testresults/x"\n' > "$probe"
  after_call=$(count_calls)
  rm -f "$probe"
  [ "$after_call" -gt "$before" ] || { echo "guard is blind to a real call: $before -> $after_call"; false; }

  # (b) a mention in a comment or an assertion — must stay GREEN. This is the
  # half the mention-counter got wrong, and why its ceiling kept rising.
  probe2="platform/tests/.4158-probe-mention-$$.sh"
  {
    printf '# the deprecated collection is /v1/testresults - do not use it\n'
    printf 'assert_not_contains "$body" "/v1/testresults"\n'
  } > "$probe2"
  after_mention=$(count_calls)
  rm -f "$probe2"
  [ "$after_mention" -eq "$before" ] || {
    echo "guard counts mentions, not calls: $before -> $after_mention"
    false
  }

  # tree clean again
  [ "$(count_calls)" -eq "$before" ]
}
