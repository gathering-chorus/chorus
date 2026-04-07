#!/bin/bash
# validate-config-paths.sh — Detect stale paths in LaunchAgent plists and Claude settings
# Checks that all referenced scripts/binaries exist on disk.
# Run as part of cruft-scan or after repo restructures.

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects}"

CANONICAL="${CHORUS_ROOT}/proving/config/launchagents"
CLAUDE_SETTINGS="$HOME/.claude/settings.json"
errors=0
warnings=0

echo "=== Config Path Validation ==="
echo ""

# 1. Check canonical plists match deployed
echo "--- Plist drift check ---"
for src in "$CANONICAL"/*.plist; do
  name="$(basename "$src")"
  deployed="$HOME/Library/LaunchAgents/$name"

  if [ ! -f "$deployed" ]; then
    echo "  WARN: $name exists in repo but not deployed"
    warnings=$((warnings + 1))
  elif ! diff -q "$src" "$deployed" >/dev/null 2>&1; then
    echo "  DRIFT: $name differs from canonical"
    warnings=$((warnings + 1))
  fi
done

# 2. Check all script/binary paths referenced in plists resolve
echo ""
echo "--- Path resolution check (plists) ---"
for plist in "$CANONICAL"/*.plist; do
  name="$(basename "$plist")"
  # Extract paths that look like ${CHORUS_ROOT}/...
  paths=$(grep -oE '/Users/jeffbridwell/[^ "<]+\.(sh|py|js)' "$plist" 2>/dev/null || true)
  paths="$paths $(grep -oE '/Users/jeffbridwell/[^ "<]+/chorus-hook-shim' "$plist" 2>/dev/null || true)"
  paths="$paths $(grep -oE '/Users/jeffbridwell/bin/[^ "<]+' "$plist" 2>/dev/null || true)"

  for p in $paths; do
    # Skip glob patterns
    [[ "$p" == *"*"* ]] && continue
    if [ ! -e "$p" ]; then
      echo "  BROKEN: $name -> $p"
      errors=$((errors + 1))
    fi
  done
done

# 3. Check Claude settings.json hook paths
echo ""
echo "--- Path resolution check (Claude hooks) ---"
if [ -f "$CLAUDE_SETTINGS" ]; then
  hook_paths=$(grep -oE '/Users/jeffbridwell/[^ "]+' "$CLAUDE_SETTINGS" 2>/dev/null || true)
  for p in $hook_paths; do
    if [ ! -e "$p" ]; then
      echo "  BROKEN: settings.json -> $p"
      errors=$((errors + 1))
    fi
  done
fi

# 4. Check role settings.local.json hook paths
echo ""
echo "--- Path resolution check (role settings) ---"
for role_dir in architect engineer product-manager; do
  settings="${CHORUS_ROOT}/platform/roles/$role_dir/.claude/settings.local.json"
  if [ -f "$settings" ]; then
    role_paths=$(grep -oE '/Users/jeffbridwell/[^ "]+' "$settings" 2>/dev/null || true)
    for p in $role_paths; do
      if [ ! -e "$p" ]; then
        echo "  BROKEN: $role_dir/settings.local.json -> $p"
        errors=$((errors + 1))
      fi
    done
  else
    echo "  MISSING: $settings"
    errors=$((errors + 1))
  fi
done

# 5. Check for stale messages/ references
echo ""
echo "--- Stale messages/ path check ---"
stale=$(grep -rl "CascadeProjects/messages/" "$CANONICAL"/*.plist "$CLAUDE_SETTINGS" 2>/dev/null || true)
if [ -n "$stale" ]; then
  for f in $stale; do
    echo "  STALE: $(basename "$f") still references CascadeProjects/messages/"
    errors=$((errors + 1))
  done
fi

echo ""
echo "=== Result: $errors errors, $warnings warnings ==="
[ "$errors" -gt 0 ] && exit 1
exit 0
