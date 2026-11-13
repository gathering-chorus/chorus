#!/usr/bin/env bats
# @test-type: unit — reads git history of this werk (and a fixture repo); runs no test
# @domain: tests
# @card: #4965 · owner: kade
# AC2: no suite is deleted, skipped, or has an assertion or floor loosened to get
# green. Read from the card's diff against main:
#   - no test file deleted
#   - no changed test file has fewer cases than on main (bats @test, it/test(, #[test])
#   - no added skip (bats `skip`, .skip(, xit(, #[ignore])
#   - no floor in coverage-floors.yml lowered
# A test corrected because it was wrong keeps its case count and its negative proof.

WERK="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

is_test() { case "$1" in */fixtures/*) return 1;; *.bats|*.test.*|*.spec.*|*/tests/*.rs) return 0;; esac; return 1; }
cases() { grep -cE '^[[:space:]]*@test |^[[:space:]]*(it|test)(\.each\([^)]*\))?\(|#\[test\]|#\[tokio::test' || true; }

# loosened <repo> <base> — one line per violation; empty = AC2 met
loosened() {
  local repo="$1" base="$2" st f b h
  git -C "$repo" diff --name-status "$base"...HEAD | while IFS=$'\t' read -r st f _; do
    is_test "$f" || continue
    case "$st" in
      D) echo "deleted: $f";;
      M*|R*)
        b=$(git -C "$repo" show "$base:$f" 2>/dev/null | cases); h=$(cases < "$repo/$f")
        [ "${h:-0}" -ge "${b:-0}" ] || echo "fewer cases: $f ($b -> $h)"
        git -C "$repo" diff "$base"...HEAD -- "$f" | grep -E '^\+' | grep -vE '^\+\+\+' \
          | grep -qE '^\+[[:space:]]*skip( |$)|\.skip\(|(^|[^a-z])xit\(|#\[ignore' && echo "skip added: $f";;
    esac
  done
  if git -C "$repo" diff --quiet "$base"...HEAD -- coverage-floors.yml 2>/dev/null; then :; else
    diff <(git -C "$repo" show "$base:coverage-floors.yml" | grep -oE '^[[:space:]]*[A-Za-z0-9_./-]+:[[:space:]]*[0-9.]+' | tr -d ' ' | sort) \
         <(grep -oE '^[[:space:]]*[A-Za-z0-9_./-]+:[[:space:]]*[0-9.]+' "$repo/coverage-floors.yml" | tr -d ' ' | sort) \
      | awk -F'[:<> ]+' '/^</{old[$2]=$3} /^>/{new[$2]=$3} END{for(k in old) if(k in new && new[k]+0<old[k]+0) print "floor lowered: " k " " old[k] " -> " new[k]}'
  fi
}

@test "AC2: this card's diff deletes, skips and loosens nothing" {
  base="$(git -C "$WERK" merge-base origin/main HEAD)"
  out="$(loosened "$WERK" "$base")"
  [ -z "$out" ] || { echo "$out"; return 1; }
}

@test "AC2 NEGATIVE PROOF: a deleted test, a dropped case, an added skip and a lowered floor are each named" {
  r="$BATS_TEST_TMPDIR/r"; mkdir -p "$r/platform/tests"; cd "$r"; git init -q
  git config user.email t@x.invalid; git config user.name t
  printf '@test "a" { true; }\n@test "b" { true; }\n' > platform/tests/two.bats
  printf '@test "c" { true; }\n' > platform/tests/gone.bats
  printf '@test "d" { true; }\n' > platform/tests/skipme.bats
  printf 'floors:\n  platform/pulse: 95\n' > coverage-floors.yml
  git add -A; git commit -qm base; base=$(git rev-parse HEAD)
  printf '@test "a" { true; }\n' > platform/tests/two.bats
  git rm -q platform/tests/gone.bats
  printf '@test "d" {\n  skip "red"\n  true\n}\n' > platform/tests/skipme.bats
  printf 'floors:\n  platform/pulse: 80\n' > coverage-floors.yml
  git add -A; git commit -qm loosen
  out="$(loosened "$r" "$base")"
  [[ "$out" == *"deleted: platform/tests/gone.bats"* ]] || { echo "$out"; return 1; }
  [[ "$out" == *"fewer cases: platform/tests/two.bats (2 -> 1)"* ]] || { echo "$out"; return 1; }
  [[ "$out" == *"skip added: platform/tests/skipme.bats"* ]] || { echo "$out"; return 1; }
  [[ "$out" == *"floor lowered: platform/pulse 95 -> 80"* ]] || { echo "$out"; return 1; }
}

@test "AC2 control: a corrected test that keeps its cases, and a raised floor, pass" {
  r="$BATS_TEST_TMPDIR/r2"; mkdir -p "$r/platform/tests"; cd "$r"; git init -q
  git config user.email t@x.invalid; git config user.name t
  printf '@test "a" { false; }\n' > platform/tests/one.bats; printf 'floors:\n  p: 80\n' > coverage-floors.yml
  git add -A; git commit -qm base; base=$(git rev-parse HEAD)
  printf '@test "a" { true; }\n@test "a negative" { ! false; }\n' > platform/tests/one.bats; printf 'floors:\n  p: 85\n' > coverage-floors.yml
  git add -A; git commit -qm fix
  [ -z "$(loosened "$r" "$base")" ]
}
