#!/usr/bin/env bats
# @test-type: unit — tagged_match from platform/tests/lib/quartet-walk.sh over planted list pages
# @domain: tests
# @card: #4915 · owner: kade
# The production quartet sent an invented ADR.filePath ("zz-4267-…") against the
# shape's sh:pattern; the door refused it (422) and 4022/4279 read that as a
# disagreement. A patterned field now takes a served row's own value with the probe
# tag before its extension, and only if the result fullmatches the pattern.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
PAT='^roles/[a-z]+/adr/ADR-[0-9]{3}.*[.]md$'

setup() {
  eval "$(sed -n '/^tagged_match() {/,/^}/p' "$ROOT/platform/tests/lib/quartet-walk.sh")"
  declare -F tagged_match >/dev/null || { echo "tagged_match is gone from quartet-walk.sh"; return 1; }
}

@test "a served row's value, tagged before its extension, fullmatches the shape's pattern" {
  run tagged_match "$PAT" filePath zz-4267-r1 <<< '{"data":[{"filePath":"roles/silas/adr/ADR-041-one-path.md"}]}'
  [ "$status" -eq 0 ] || return 1
  [ "$output" = "roles/silas/adr/ADR-041-one-path-zz-4267-r1.md" ] || { echo "$output"; return 1; }
}

@test "negative proof: a pattern no served row matches is UNMET, never a value (UNMEASURED, not a red or a pass)" {
  run tagged_match "$PAT" filePath zz-4267-r1 <<< '{"data":[{"filePath":"docs/adr-41.txt"},{"label":"x"}]}'
  [ "$output" = "UNMET $PAT" ] || { echo "$output"; return 1; }
  run tagged_match "$PAT" filePath zz-4267-r1 <<< '{"data":[]}'
  [ "$output" = "UNMET $PAT" ] || { echo "$output"; return 1; }
  run tagged_match "$PAT" filePath zz-4267-r1 <<< 'not json'
  [ "$output" = "UNMET $PAT" ] || { echo "$output"; return 1; }
}

@test "negative proof: a tag that would break the pattern is not sent" {
  # a pattern pinning the stem exactly: the tagged value no longer fits, so UNMET
  run tagged_match '^roles/[a-z]+/adr/ADR-[0-9]{3}[.]md$' filePath zz-4267-r1 <<< '{"data":[{"filePath":"roles/silas/adr/ADR-041.md"}]}'
  [ "$output" = 'UNMET ^roles/[a-z]+/adr/ADR-[0-9]{3}[.]md$' ] || { echo "$output"; return 1; }
}

# #4925 (Wren 2', Silas) — an anchored pattern no tag can fit: DeployTarget.fileMode and
# KeyRegistryEntry.keyId read UNMEASURED every night and 4279 counted that red.
@test "#4925 keyId: the tag folded to the pattern's alphabet fits ^[A-Z0-9_]+\$" {
  run tagged_match '^[A-Z0-9_]+$' keyId zz-4267-r1 0 <<< '{"data":[{"keyId":"CHORUS_KEY"}]}'
  [ "$output" = "CHORUS_KEY_ZZ_4267_R1" ] || { echo "$output"; return 1; }
  python3 -c 'import re,sys; sys.exit(0 if re.search(sys.argv[1], sys.argv[2]) else 1)' '^[A-Z0-9_]+$' "$output"
}

@test "#4925 fileMode: a field that names nothing takes the served value as-is" {
  run tagged_match '^0[0-7]{3}$' fileMode zz-4267-r1 1 <<< '{"data":[{"fileMode":"0644"}]}'
  [ "$output" = "0644" ] || { echo "$output"; return 1; }
}

@test "#4925 NEGATIVE PROOF: a unique, name or anyURI field (asis 0) never gets a served value as-is" {
  run tagged_match '^0[0-7]{3}$' fileMode zz-4267-r1 0 <<< '{"data":[{"fileMode":"0644"}]}'
  [ "$output" = 'UNMET ^0[0-7]{3}$' ] || { echo "sent a live row's value: $output"; return 1; }
  run tagged_match '^0[0-7]{3}$' fileMode zz-4267-r1 <<< '{"data":[{"fileMode":"0644"}]}'
  [ "$output" = 'UNMET ^0[0-7]{3}$' ] || { echo "no asis flag defaulted to as-is: $output"; return 1; }
}

@test "#4925 NEGATIVE PROOF: a pattern no served row fits is still UNMET" {
  run tagged_match '^0[0-7]{3}$' fileMode zz-4267-r1 1 <<< '{"data":[{"fileMode":"rw-r--r--"}]}'
  [ "$output" = 'UNMET ^0[0-7]{3}$' ] || { echo "$output"; return 1; }
}
