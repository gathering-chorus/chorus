#!/usr/bin/env bats
# @test-type: contract
# 4171 — a night's backup writes its delta, not the whole store.
#
# Measured 2026-09-14 08:47: the 00:00:12 run was still copying at 08:46 —
# 268 GB of a 377 GB store, with 09-13 (407G) and 09-12 (376G) already held and
# Bedroom at 86%. rsync's delta kept the WIRE small; with no --link-dest every
# unchanged file was still WRITTEN into a fresh dated directory, so the target
# grew by a store per night and the job ran into the working day.
#
# Jeff, three times: 2026-08-09 "too long and too big", 2026-09-10 "they start
# at midnight and are still running", 2026-09-14 "the size increases".

REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
SCRIPT="$REPO_ROOT/platform/scripts/fuseki-backup.sh"

setup() {
  TMP="$(mktemp -d)"
  SRC="$TMP/src"; mkdir -p "$SRC"
  # A store: one big unchanged file, one small file that changes nightly.
  head -c 2000000 /dev/urandom > "$SRC/Data-0001"
  echo "n1" > "$SRC/journal"
}
teardown() { rm -rf "$TMP"; }

# The copy as the script performs it, so the test exercises the real flags.
copy_night() {
  local src="$1" dest="$2" link="$3"
  if [ -n "$link" ]; then
    rsync -a --partial --link-dest="$link" "$src/" "$dest/"
  else
    rsync -a --partial "$src/" "$dest/"
  fi
}

@test "the script hard-links against the newest prior snapshot" {
  grep -q -- '--link-dest=' "$SCRIPT"
  # by NAME, the same ordering prune uses (#3837: never mtime)
  grep -q "sort -r | head -1" "$SCRIPT"
}

@test "a second night costs its delta, not another store" {
  copy_night "$SRC" "$TMP/n1" ""
  echo "night two is longer" > "$SRC/journal"
  copy_night "$SRC" "$TMP/n2" "$TMP/n1"

  # Apparent size of each night is a full store — both restore independently.
  local a1 a2
  a1=$(du -sk "$TMP/n1" | cut -f1)
  a2=$(du -sk "$TMP/n2" | cut -f1)
  [ "$a2" -ge $(( a1 * 90 / 100 )) ]

  # But the two together occupy barely more than one, because the big file is
  # ONE inode shared by both. Before this card it was two.
  local both
  both=$(du -skc "$TMP/n1" "$TMP/n2" | tail -1 | cut -f1)
  [ "$both" -lt $(( a1 * 130 / 100 )) ]
}

@test "the unchanged file is literally shared, the changed one is not" {
  copy_night "$SRC" "$TMP/n1" ""
  echo "night two is longer" > "$SRC/journal"
  copy_night "$SRC" "$TMP/n2" "$TMP/n1"

  [ "$(stat -f %i "$TMP/n1/Data-0001")" = "$(stat -f %i "$TMP/n2/Data-0001")" ]
  [ "$(stat -f %i "$TMP/n1/journal")" != "$(stat -f %i "$TMP/n2/journal")" ]
}

@test "NEGATIVE PROOF — without --link-dest the same two nights cost two stores" {
  # The state this card leaves behind. If this does not blow the budget, the
  # test above proves nothing.
  copy_night "$SRC" "$TMP/n1" ""
  echo "night two is longer" > "$SRC/journal"
  copy_night "$SRC" "$TMP/n2" ""

  local a1 both
  a1=$(du -sk "$TMP/n1" | cut -f1)
  both=$(du -skc "$TMP/n1" "$TMP/n2" | tail -1 | cut -f1)
  [ "$both" -gt $(( a1 * 180 / 100 )) ]
  [ "$(stat -f %i "$TMP/n1/Data-0001")" != "$(stat -f %i "$TMP/n2/Data-0001")" ]
}

@test "NEGATIVE PROOF — deleting an old snapshot does not hollow out a newer one" {
  # Hard links are not a chain. Prune must stay safe.
  copy_night "$SRC" "$TMP/n1" ""
  echo "night two is longer" > "$SRC/journal"
  copy_night "$SRC" "$TMP/n2" "$TMP/n1"
  local before
  before=$(md5 -q "$TMP/n2/Data-0001" 2>/dev/null || cksum < "$TMP/n2/Data-0001")
  rm -rf "$TMP/n1"
  local after
  after=$(md5 -q "$TMP/n2/Data-0001" 2>/dev/null || cksum < "$TMP/n2/Data-0001")
  [ "$before" = "$after" ]
  [ -s "$TMP/n2/Data-0001" ]
}

@test "a first run with no prior snapshot still copies in full" {
  grep -q "no prior snapshot" "$SCRIPT"
  copy_night "$SRC" "$TMP/n1" ""
  [ -s "$TMP/n1/Data-0001" ]
}
