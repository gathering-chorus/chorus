#!/usr/bin/env bats
# #4185 — repointed again: the parsers moved from the Python library (#4159)
# into the crawler crate (chorus-crawl, src/cases.rs). Same behaviour, same
# asserts, third home — the seams are the binary's own (--names-of, --covers-of,
# --check-shares, --classify), no store, no network.
# @test-type: unit — hermetic: imports the tagger's pure functions, no store
# @domain: tests — the product domain this suite guards (#4334)
#
# #3924 — the AUTHORED @test-type header beats the path/content heuristic.
# The header was enforced at commit (#3442) and then thrown away at ingest;
# classify() re-guessed every layer. These pin the new contract:
#   authored layer[:concern] wins on BOTH axes it declares (Wren trap 1),
#   prose that merely MENTIONS the header does not declare one (Wren trap 2),
#   and absence still falls to the heuristic, flagged inferred.

# bash 3.2 (this Mac) never fires errexit on a failing `[[ ]]`, so a `[[` assert
# that is not the LAST line of a test can fail and the test still passes (#4185,
# measured 2026-09-16: `[[ "a" == *"b"* ]]; true` → ok). Every assert here is a
# simple command, which bash 3.2 does honour.
has()   { grep -qF -- "$1" <<<"${2-$output}"; }
lacks() { if grep -qF -- "$1" <<<"${2-$output}"; then echo "unexpected: $1" >&2; return 1; fi; }
eq()    { [ "$1" = "$2" ] || { echo "expected [$2] got [$1]" >&2; return 1; }; }

setup() {
  BIN="${CHORUS_CRAWL_BIN:-$BATS_TEST_DIRNAME/../services/chorus-crawl/target/release/chorus-crawl}"; [ -x "$BIN" ] || BIN="$BATS_TEST_DIRNAME/../services/chorus-crawl/target/debug/chorus-crawl"; [ -x "$BIN" ] || skip "chorus-crawl not built at $BIN"
}

# --classify prints: layer hermeticity concern declared|inferred
cls() { printf '%s' "$2" > "$BATS_TEST_TMPDIR/$1"; "$BIN" --classify "$BATS_TEST_TMPDIR/$1"; }

@test "declared() contract — authored wins, prose does not declare, junk refused" {
  # authored beats heuristic on layer even when content SCREAMS integration
  [ "$(cls a.bats $'// @test-type: unit — hermetic despite the curl below\ncurl http://localhost:3030/x')" = "unit needs-stack - declared" ]
  # authored concern beats classify()'s own concern regex (trap 1)
  [ "$(cls b.bats $'// @test-type: unit:api\n#  gitleaks mention would heuristically say security')" = "unit hermetic api declared" ]
  # prose MENTION is not a declaration (trap 2 — the June gate-test-type case)
  [ "$(cls c.bats $'// the @test-type: header is required by the gate\n// @test-type: headers matter')" = "unit hermetic - inferred" ]
  # junk layer -> undeclared -> heuristic+inferred, never a fabricated row
  [ "$(cls d.bats '// @test-type: banana')" = "unit hermetic - inferred" ]
  # justification form parses
  [ "$(cls e.bats '# @test-type: e2e:ui — playwright flow')" = "e2e hermetic ui declared" ]
  # no header at all -> heuristic path
  [ "$(cls f.bats 'plain file, no header')" = "unit hermetic - inferred" ]
}

@test "discovery reaches proving/ and .spec.cjs — the zero-browser-tests hole (#3872)" {
  # The crawler walks `git ls-files`, so discovery IS the tree; what matters is
  # that a .spec.cjs under proving/ is classified kind=test, which the crate's
  # is_test_path/classify unit tests pin. Here: the exact spec that went green
  # without running is tracked, and a dry run over the tree counts it.
  cd "$BATS_TEST_DIRNAME/../.."
  git ls-files --error-unmatch proving/flows/clearing-base-path-3872.spec.cjs >/dev/null
  # #4273 — chorus-crawl refuses to run with CHORUS_ROLE unset (#4178: it will
  # not write as an invented principal). The nightly plist runs it as kade;
  # this test only passed in shells that already exported a role.
  run env CHORUS_ROOT="$PWD" CHORUS_ROLE=kade "$BIN" --dry-run
  [ "$status" -eq 0 ]
  has "cases posted="
  # files that yield no runnable case (#4106) are REPORTED on their own line, not silent
  has "no-case files:"
}
