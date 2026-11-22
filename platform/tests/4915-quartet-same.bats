#!/usr/bin/env bats
# @test-type: unit — _same from platform/tests/lib/quartet-walk.sh over planted sent/read-back pairs
# @domain: tests
# @card: #4915 · owner: kade
# The quartet's read-back compare: an xsd:decimal comes back in the store's canonical
# spelling (sent "922119", read "922119.0") and is the same value. Review 11-22: the first
# cut compared every field by numeric value, so a string field sent "007" and read "7"
# passed. Only the canonical decimal spelling counts as the same.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

same() { # same <have> <want> → True / False
  python3 -c '
import sys
src = open(sys.argv[1]).read()
start = src.find("def _same(have, want):")
end = src.find("\nbad=[]", start)
if start < 0 or end < 0: sys.exit("_same is gone from quartet-walk.sh")
exec(src[start:end])
print(_same(sys.argv[2], sys.argv[3]))' "$ROOT/platform/tests/lib/quartet-walk.sh" "$1" "$2"
}

@test "an xsd:decimal read back in the store's canonical spelling is the same value" {
  [ "$(same 922119.0 922119)" = True ] || { echo "922119.0 vs 922119: $(same 922119.0 922119)"; false; }
  [ "$(same 1.5 1.50)" = True ]
  [ "$(same chorus:principal-abby-normal abby-normal)" = True ]
}

@test "negative proof: a different number, or a numeric-looking string respelt, is a lost field" {
  [ "$(same 922120.0 922119)" = False ] || { echo "922120.0 matched 922119"; false; }
  [ "$(same 7 007)" = False ] || { echo "a string '007' read back as '7' passed"; false; }
  [ "$(same 1000 1e3)" = False ] || { echo "'1e3' read back as '1000' passed"; false; }
}
