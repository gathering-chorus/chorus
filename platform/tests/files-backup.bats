#!/usr/bin/env bats
# @test-type: unit
# @domain: security — the sign-in servers' accounts and keys (#4518)
# Drives the REAL files-backup.sh end to end: stub ssh/scp run the "remote" in a
# temp dir. What Jeff gets: every file tree the store map marks for backup is on
# Bedroom each night, and nothing silently backs up nothing.

SCRIPT="${BATS_TEST_DIRNAME}/../scripts/files-backup.sh"

setup() {
  TMP="$BATS_TEST_TMPDIR"; REMOTE="$TMP/remote"; mkdir -p "$REMOTE" "$TMP/bin" "$TMP/scripts" "$TMP/local"
  mkdir -p "$TMP/trees/css" "$TMP/trees/wine-css" "$TMP/trees/other"
  head -c 4000 /dev/urandom > "$TMP/trees/css/account.json"
  head -c 4000 /dev/urandom > "$TMP/trees/wine-css/account.json"
  head -c 4000 /dev/urandom > "$TMP/trees/other/x"
  cp "$SCRIPT" "$TMP/scripts/files-backup.sh"; : > "$TMP/scripts/chorus-env-setup.sh"
  printf '%s\n' '#!/bin/bash' 'n=0; while [ $# -gt 0 ]; do case "$1" in -o) shift 2 ;; -n) n=1; shift ;; -*) shift ;; *) break ;; esac; done' \
    'shift' "[ \$n = 1 ] || python3 -c 'import os
os.set_blocking(0, False)
try: os.read(0, 1 << 20)
except Exception: pass' 2>/dev/null" 'bash -c "$*" < /dev/null' > "$TMP/bin/ssh"
  # ^ a real ssh reads stdin; the stub does too, so a loop fed on stdin that
  #   forgets `ssh -n` loses every store after the first here, as it did live
  #   on 2026-10-13 14:31 (solid-css shipped, wine-diary never read).
  printf '%s\n' '#!/bin/bash' 'args=(); while [ $# -gt 0 ]; do case "$1" in -o) shift 2 ;; -*) shift ;; *) args+=("$1"); shift ;; esac; done' \
    'cp "${args[0]}" "${args[1]#*:}"' > "$TMP/bin/scp"
  printf '#!/bin/bash\nexit 0\n' > "$TMP/bin/chorus-log"
  chmod +x "$TMP/bin/"*
}

# map FLOOR — two trees marked for backup, one system of record not marked
map() {
  printf '{"stores":[
 {"name":"solid-css","storeType":"files","role":"systemOfRecord","backup":"tar","backupMinBytes":%s,"match":{"kind":"path","value":"%s"}},
 {"name":"solid-css-wine-diary","storeType":"files","role":"systemOfRecord","backup":"tar","backupMinBytes":%s,"match":{"kind":"path","value":"%s"}},
 {"name":"other","storeType":"files","role":"systemOfRecord","match":{"kind":"path","value":"%s"}}]}' \
    "$1" "$TMP/trees/css" "$1" "$TMP/trees/wine-css" "$TMP/trees/other" > "$TMP/map.json"
}

run_backup() {
  run env -i PATH="$TMP/bin:/usr/bin:/bin" HOME="$TMP" FILES_BACKUP_REMOTE=stub FILES_BACKUP_DEST="$REMOTE" \
    FILES_BACKUP_LOCAL="$TMP/local" CHORUS_LOG="$TMP/bin/chorus-log" STORE_MAP="$TMP/map.json" \
    FILES_BACKUP_KEEP="${1:-7}" bash "$TMP/scripts/files-backup.sh"
}

@test "every tree the store map marks is shipped, and only those" {
  map 1000; run_backup
  [ "$status" -eq 0 ]
  [ "$(ls "$REMOTE/solid-css" | wc -l | tr -d ' ')" = 1 ]
  [ "$(ls "$REMOTE/solid-css-wine-diary" | wc -l | tr -d ' ')" = 1 ]
  [ ! -d "$REMOTE/other" ]
  tar -tzf "$REMOTE/solid-css-wine-diary/"*.tar.gz | grep -q "wine-css/account.json"
}

@test "NEGATIVE PROOF — a map that marks nothing refuses instead of backing up nothing" {
  printf '{"stores":[{"name":"other","storeType":"files","role":"systemOfRecord","match":{"kind":"path","value":"%s"}}]}' "$TMP/trees/other" > "$TMP/map.json"
  run_backup
  [ "$status" -ne 0 ]
  echo "$output" | grep -q "marks no file tree"
  [ -z "$(ls "$REMOTE")" ]
}

@test "NEGATIVE PROOF — an archive under its floor is refused and not shipped" {
  map 100000000; run_backup
  [ "$status" -ne 0 ]
  echo "$output" | grep -q "refusing to ship a stub"
  [ -z "$(ls "$REMOTE/solid-css" 2>/dev/null)" ]
}

@test "NEGATIVE PROOF — retention keeps the newest by name, whatever the mtimes say" {
  map 1000; mkdir -p "$REMOTE/solid-css"
  for d in 2026-01-01 2026-01-02 2026-01-03; do printf x > "$REMOTE/solid-css/solid-css_${d}_03-00-00.tar.gz"; done
  touch -t 209901010000 "$REMOTE/solid-css/solid-css_2026-01-01_03-00-00.tar.gz"
  run_backup 2
  [ "$status" -eq 0 ]
  [ "$(ls "$REMOTE/solid-css" | wc -l | tr -d ' ')" = 2 ]
  [ ! -f "$REMOTE/solid-css/solid-css_2026-01-01_03-00-00.tar.gz" ]
  [ ! -f "$REMOTE/solid-css/solid-css_2026-01-02_03-00-00.tar.gz" ]
}
