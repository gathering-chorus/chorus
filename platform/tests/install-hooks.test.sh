#!/usr/bin/env bash
# Hermetic test for install-hooks.sh (#2465).
# Creates a fake git repo with platform/hooks/ + .git/hooks/ layout, exercises
# install cases: fresh, already-installed, stale-symlink, pre-existing-non-symlink.

set -uo pipefail

SCRIPT_SRC="$(cd "$(dirname "$0")/../scripts" && pwd)/install-hooks.sh"
PASS=0
FAIL=0

assert() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    echo "  PASS: $label"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: $label — expected='$expected' actual='$actual'"
    FAIL=$((FAIL + 1))
  fi
}

FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT

# Lay out a fake repo
mkdir -p "$FIXTURE/platform/hooks" "$FIXTURE/platform/scripts" "$FIXTURE/.git/hooks"
cp "$SCRIPT_SRC" "$FIXTURE/platform/scripts/install-hooks.sh"
chmod +x "$FIXTURE/platform/scripts/install-hooks.sh"

cat > "$FIXTURE/platform/hooks/pre-commit" <<'EOF'
#!/bin/bash
# fixture pre-commit v1
exit 0
EOF
chmod +x "$FIXTURE/platform/hooks/pre-commit"

# Case 1: fresh install (no existing hook)
bash "$FIXTURE/platform/scripts/install-hooks.sh" >/dev/null 2>&1
if [ -L "$FIXTURE/.git/hooks/pre-commit" ] && [ "$(readlink "$FIXTURE/.git/hooks/pre-commit")" = "$FIXTURE/platform/hooks/pre-commit" ]; then
  assert "fresh install creates symlink" "ok" "ok"
else
  assert "fresh install creates symlink" "ok" "missing-or-wrong"
fi

# Case 2: idempotent (re-run on already-installed state)
OUT=$(bash "$FIXTURE/platform/scripts/install-hooks.sh" 2>&1)
if echo "$OUT" | grep -q '0 installed, 1 already current'; then
  assert "idempotent re-run" "ok" "ok"
else
  assert "idempotent re-run" "ok" "wrong-output: $OUT"
fi

# Case 3: pre-existing non-symlink is backed up
rm "$FIXTURE/.git/hooks/pre-commit"
cat > "$FIXTURE/.git/hooks/pre-commit" <<'EOF'
#!/bin/bash
# old hand-written hook
EOF
chmod +x "$FIXTURE/.git/hooks/pre-commit"
bash "$FIXTURE/platform/scripts/install-hooks.sh" >/dev/null 2>&1
BACKUPS=$(ls "$FIXTURE/.git/hooks/" | grep 'pre-commit.preinstall-' | wc -l | tr -d ' ')
assert "non-symlink backed up" "1" "$BACKUPS"
if [ -L "$FIXTURE/.git/hooks/pre-commit" ]; then
  assert "new symlink installed after backup" "ok" "ok"
else
  assert "new symlink installed after backup" "ok" "missing"
fi

# Case 4: stale symlink pointing elsewhere is replaced
rm "$FIXTURE/.git/hooks/pre-commit"
ln -s "/tmp/does-not-exist" "$FIXTURE/.git/hooks/pre-commit"
bash "$FIXTURE/platform/scripts/install-hooks.sh" >/dev/null 2>&1
if [ "$(readlink "$FIXTURE/.git/hooks/pre-commit")" = "$FIXTURE/platform/hooks/pre-commit" ]; then
  assert "stale symlink replaced" "ok" "ok"
else
  assert "stale symlink replaced" "ok" "wrong-target"
fi

# Case 5: missing platform/hooks dir -> exit 0, no crash
rm -rf "$FIXTURE/platform/hooks"
bash "$FIXTURE/platform/scripts/install-hooks.sh" >/dev/null 2>&1
rc=$?
assert "missing hooks source is no-op" "0" "$rc"

# Case 6: missing .git/hooks dir -> exit 1
mkdir -p "$FIXTURE/platform/hooks"
cat > "$FIXTURE/platform/hooks/pre-commit" <<'EOF'
#!/bin/bash
EOF
rm -rf "$FIXTURE/.git"
bash "$FIXTURE/platform/scripts/install-hooks.sh" >/dev/null 2>&1
rc=$?
assert "missing .git/hooks errors" "1" "$rc"

echo ""
echo "Result: $PASS passed, $FAIL failed"
[ "$FAIL" = "0" ]
