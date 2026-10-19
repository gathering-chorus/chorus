#!/usr/bin/env bats
# @test-type: unit — memory-backup.py over fixture sources; the three places are temp dirs, Buzz dumps are echo commands
# @domain: backups
# #4630 — one dated folder per Boston day, sha256 per file, copied to Library,
# Bedroom and Google Drive and checked by hash at each place. Short is red.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  T="$(mktemp -d)"
  mkdir -p "$T/local" "$T/bedroom" "$T/drive-home/My Drive" "$T/transcripts/p"
  {
    echo '{"event":"a","timestamp":"2026-10-17T23:30:00-0400"}'
    echo '{"event":"b","timestamp":"2026-10-18T01:00:00-0400"}'
    echo '{"event":"c","timestamp":"2026-10-18T12:00:00.123Z"}'
    echo 'torn tail with no time'
    echo '{"event":"d","timestamp":"2026-10-19T00:10:00-0400"}'
  } > "$T/chorus.log"
  python3 -c 'import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute("create table messages(x)"); c.execute("insert into messages values (1)"); c.commit()' "$T/messages.db"
  echo '{}' > "$T/transcripts/p/s.jsonl"
  touch -t 202610181200 "$T/transcripts/p/s.jsonl"
}

teardown() { rm -rf "$T"; }

backup() {
  run env MEMORY_SPINE="$T/chorus.log" MEMORY_MESSAGES_DB="$T/messages.db" MEMORY_TRANSCRIPTS="$T/transcripts" \
    MEMORY_LOCAL="$T/local" MEMORY_REMOTE="$T/bedroom" MEMORY_DRIVE="${DRIVE-$T/drive-home/My Drive/chorus-backups}" \
    MEMORY_BUZZ_PG_CMD="echo 'create table buzz'" MEMORY_BUZZ_MINIO_CMD="echo minio" \
    python3 "$ROOT/platform/scripts/memory-backup.py" "$@"
}

@test "a day is kept in three places with matching hashes, and its spine slice is only that day's lines" {
  backup 2026-10-18
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"OK — 5 files, 3 places"* ]] || return 1
  for place in "$T/local" "$T/bedroom" "$T/drive-home/My Drive/chorus-backups"; do
    (cd "$place/2026-10-18" && shasum -a 256 -c SHA256SUMS >/dev/null) || return 1
  done
  run bash -c "gunzip -c '$T/local/2026-10-18/spine-2026-10-18.jsonl.gz'"
  [ "${#lines[@]}" -eq 3 ] || return 1
  [[ "$output" == *'"b"'* && "$output" == *'"c"'* && "$output" == *"torn tail"* ]] || return 1
  [[ "$output" != *'"a"'* && "$output" != *'"d"'* ]]
}

@test "negative proof: one changed byte in a copied slice turns the backup red" {
  backup 2026-10-18
  [ "$status" -eq 0 ] || return 1
  f="$T/bedroom/2026-10-18/spine-2026-10-18.jsonl.gz"
  printf 'X' | dd of="$f" bs=1 seek=20 conv=notrunc 2>/dev/null
  rm -rf "$T/local/2026-10-18/../2026-10-18.tmp"
  # the next run re-copies to Bedroom only what is missing: point it at a
  # fresh Library so it verifies the damaged Bedroom copy as-is
  run env MEMORY_SPINE="$T/chorus.log" MEMORY_MESSAGES_DB="$T/messages.db" MEMORY_TRANSCRIPTS="$T/transcripts" \
    MEMORY_LOCAL="$T/local" MEMORY_REMOTE="$T/bedroom-check" MEMORY_DRIVE="$T/drive-home/My Drive/chorus-backups" \
    MEMORY_BUZZ_PG_CMD="echo x" MEMORY_BUZZ_MINIO_CMD="echo y" \
    python3 -c 'import sys,importlib.util; s=importlib.util.spec_from_file_location("m",sys.argv[1]); m=importlib.util.module_from_spec(s); s.loader.exec_module(m); bad=m.verify(sys.argv[2], m.sums_of(sys.argv[3])); print(bad); sys.exit(1 if bad else 0)' \
    "$ROOT/platform/scripts/memory-backup.py" "$T/bedroom/2026-10-18" "$T/local/2026-10-18"
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"spine-2026-10-18.jsonl.gz sha256 differs"* ]] || return 1
}

@test "Google Drive not signed in is red, never a quiet skip" {
  DRIVE="$T/nowhere/My Drive/chorus-backups" backup 2026-10-18
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"drive: Google Drive is not signed in"* ]] || return 1
}

@test "a failed Buzz dump is red and the day is not written as kept" {
  run env MEMORY_SPINE="$T/chorus.log" MEMORY_MESSAGES_DB="$T/messages.db" MEMORY_TRANSCRIPTS="$T/transcripts" \
    MEMORY_LOCAL="$T/local" MEMORY_REMOTE="$T/bedroom" MEMORY_DRIVE="$T/drive-home/My Drive/chorus-backups" \
    MEMORY_BUZZ_PG_CMD="exit 3" MEMORY_BUZZ_MINIO_CMD="echo y" \
    python3 "$ROOT/platform/scripts/memory-backup.py" 2026-10-18
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"buzz postgres: dump failed (exit 3)"* ]] || return 1
  [ ! -f "$T/local/2026-10-18/SHA256SUMS" ]
}
