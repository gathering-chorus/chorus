#!/usr/bin/env bats
# @test-type: unit — no other agent can write a role's bin dir
# @domain: security
# @card: #4710 · owner: wren
# What Jeff sees: a role's tools can't be swapped by another role. Borg Queen's bypass 3
# (10-23): role *-bin dirs were group-writable and on the land's PATH. Fixed for abby-normal
# 10-24 18:16 by hand (then owned by chorus-abby-normal); this is its check. Measured
# 11-11 17:20: every *-bin is jeffbridwell 755, so no chorus-* agent account can write any of
# them. Why abby-normal-bin's owner moved back to jeffbridwell is unknown (dir mtime 17:07 is
# an entry added or removed; its newest file is 16:33). Writers running as jeffbridwell stay
# open under AC1.
load test_helper
BASE="${CHORUS_WERK_BASE:-$(dirname "$CHORUS_ROOT")/chorus-werk}"

# prints why dir $1 is writable by an agent account other than its own role, if it is
problems() {
  local d="$1" role; role="$(basename "$d")"; role="${role%-bin}"
  [ -d "$d" ] || { echo "$d: missing"; return 0; }
  local mode owner; mode="$(stat -f '%Lp' "$d")"; owner="$(stat -f '%Su' "$d")"
  (( (8#$mode & 8#022) == 0 )) || echo "$d: group or other may write (mode $mode)"
  case "$owner" in chorus-*) [ "$owner" = "chorus-$role" ] || echo "$d: owned by another agent, $owner" ;; esac
  return 0
}

@test "AC6: no role bin dir is writable by group, other, or another agent" {
  out=""
  for d in "$BASE"/abby-normal-bin "$BASE"/borg-queen-bin "$BASE"/kade-bin "$BASE"/silas-bin "$BASE"/wren-bin; do
    out+="$(problems "$d")"
  done
  [ -z "$out" ] || { echo "$out"; return 1; }
}

@test "AC6 negative proof: a group-writable bin (the 10-23 state) is caught" {
  d="$BATS_TEST_TMPDIR/abby-normal-bin"; mkdir -p "$d"; chmod 775 "$d"
  out="$(problems "$d")"
  [[ "$out" == *"group or other may write (mode 775)"* ]] || { echo "$out"; return 1; }
}
