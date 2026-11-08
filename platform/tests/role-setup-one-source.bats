#!/usr/bin/env bats
# @test-type: unit — reads the repo's role settings; no live services
# @domain: identity
#
# #4931 AC1 — the three Claude roles' settings are one setup that differs only by the
# role's own name and paths (Jeff 2026-11-08: "i dont need 3 snow flake claude code
# instances"). Measured 08:17: the live files were identical apart from the role name,
# but ~60 lines were never committed, so git and the running roles disagreed.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  T="$BATS_TEST_TMPDIR"
}
# a role's settings with its own name taken out (werk names of the other roles stay)
norm() { sed -E "s/[[:<:]]$1[[:>:]]/ROLE/g; s/chorus-$1/chorus-ROLE/g" "$2" | grep -vE '"(KADE|SILAS|WREN)_WERK"'; }

@test "#4931 silas, wren and kade have one setup, differing only by name" {
  for r in silas kade; do
    diff <(norm wren "$ROOT/roles/wren/.claude/settings.json") <(norm "$r" "$ROOT/roles/$r/.claude/settings.json") \
      || { echo "$r settings differ from wren's beyond the role name"; return 1; }
  done
}

@test "#4931 NEGATIVE PROOF: a role file changed by one line is reported, naming role and line" {
  cp "$ROOT/roles/kade/.claude/settings.json" "$T/kade.json"
  sed -i '' '2s/$/ /; 2s/^/  "drift": true,\n/' "$T/kade.json"
  run diff <(norm wren "$ROOT/roles/wren/.claude/settings.json") <(norm kade "$T/kade.json")
  [ "$status" -ne 0 ]
  printf '%s\n' "$output" | grep -qF '"drift": true'
}

@test "#4931 the Gemini roles' settings are in git" {
  for r in abby-normal borg-queen; do
    git -C "$ROOT" ls-files --error-unmatch "roles/$r/.gemini/settings.json" >/dev/null || { echo "$r .gemini/settings.json not in git"; return 1; }
  done
}
