#!/usr/bin/env bats
# @test-type: unit — rsync-backup.sh with the real local rsync (remote rewritten to a temp dir) and a fake ssh; nothing leaves the machine
# @domain: persistence — backups are persistence; there is no backups domain (#4670)
# #4873 — the nightly backup failed 2 of 5 targets (rsync exit 23) on files the
# backup account cannot read by design: other accounts' cargo build dirs
# (target/chorus-<account>/) and the role-private outboxes (.chorus/outbox/<role>/).
# Those are skipped; any other unreadable file must still fail the run.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  T="$(mktemp -d)"
  mkdir -p "$T/bin" "$T/dest" "$T/home/.chorus" "$T/home/.claude" "$T/home/.ssh" "$T/home/CascadeProjects/p/src" "$T/home/Library/LaunchAgents"
  echo ok > "$T/home/CascadeProjects/p/src/main.rs"
  printf '#!/bin/sh\nexit 0\n' > "$T/bin/ssh"; chmod +x "$T/bin/ssh"
  # the real rsync, with the -e transport dropped and host:path made local
  cat > "$T/bin/rsync" <<EOF
#!/bin/bash
args=(); skip=0
for a in "\$@"; do
  if [ \$skip = 1 ]; then skip=0; continue; fi
  case "\$a" in -e) skip=1 ;; *:/*) args+=("$T/dest/\${a#*:}") ;; *) args+=("\$a") ;; esac
done
mkdir -p "\${args[\${#args[@]}-1]}"
exec /usr/bin/rsync "\${args[@]}"
EOF
  chmod +x "$T/bin/rsync"
}

teardown() { chmod -R u+rwx "$T" 2>/dev/null; rm -rf "$T"; }

unreadable() { mkdir -p "$1"; echo x > "$1/f.lock"; chmod 000 "$1"; }

run_backup() {
  run env HOME="$T/home" PATH="$T/bin:$PATH" BACKUP_RSYNC="$T/bin/rsync" BACKUP_SSH="$T/bin/ssh" BACKUP_DEST=/b CHORUS_ROOT="$T/none" \
    bash -c 'sleep() { :; }; export -f sleep; bash "$0"' "$ROOT/platform/scripts/rsync-backup.sh"
}

@test "other accounts' cargo build dirs and the role-private outboxes are skipped, and the run is green" {
  unreadable "$T/home/CascadeProjects/p/target/chorus-kade/debug/incremental"
  unreadable "$T/home/CascadeProjects/p/target/chorus-wren"
  unreadable "$T/home/.chorus/outbox/kade"
  run_backup
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"5 synced, 0 failed"* ]] || return 1
  [ -f "$T/dest/b/CascadeProjects/CascadeProjects/p/src/main.rs" ] || return 1
}

@test "negative proof: any other unreadable dir still fails the copy, never OK" {
  unreadable "$T/home/CascadeProjects/p/secret"
  run_backup
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  [[ "$output" == *"FAIL: CascadeProjects (rsync exit 23 after 2 attempts)"* ]] || return 1
  [[ "$output" != *"OK: CascadeProjects"* ]] || return 1
}

@test "negative proof: an unreadable file in the outbox root (not a role's dir) still fails" {
  mkdir -p "$T/home/.chorus/outbox"; echo x > "$T/home/.chorus/outbox/queue.json"
  chmod 000 "$T/home/.chorus/outbox/queue.json"
  run_backup
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  [[ "$output" == *"FAIL: dot-chorus"* ]] || return 1
}
