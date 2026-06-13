#!/usr/bin/env bats
# 2709-gitleaks-baseline.bats — secret-scanning gate (AC1)
# What Jeff sees: no new secret can be committed; the 35 known historical
# findings (logged auth + LAN fuseki cred, 2026-03-29 window) are baselined,
# pending triage, so the gate goes live without blasting every commit.

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"

@test "gitleaks config exists and extends the default ruleset" {
  [ -f "$REPO/.gitleaks.toml" ]
  grep -q "useDefault = true" "$REPO/.gitleaks.toml"
}

@test "baseline exists and is valid JSON with fingerprinted findings" {
  [ -f "$REPO/.gitleaks-baseline.json" ]
  run python3 -c "import json,sys; d=json.load(open('$REPO/.gitleaks-baseline.json')); sys.exit(0 if isinstance(d,list) and all('Fingerprint' in x for x in d) else 1)"
  [ "$status" -eq 0 ]
}

@test "pre-commit hook exists, is executable, and runs gitleaks against the baseline" {
  [ -x "$REPO/platform/hooks/pre-commit" ]
  grep -q "gitleaks protect --staged" "$REPO/platform/hooks/pre-commit"
  grep -q "baseline-path" "$REPO/platform/hooks/pre-commit"
}

@test "pre-commit fails OPEN when gitleaks is absent (dev-setup gap never blocks work; CI gates hard)" {
  grep -q "secret scan SKIPPED" "$REPO/platform/hooks/pre-commit"
}

@test "gitleaks is installed and catches a fresh curl-auth secret" {
  command -v gitleaks >/dev/null
  d=$(mktemp -d)
  printf 'curl -u admin:hunter2primarysecret http://x\n' > "$d/leak.txt"
  run gitleaks detect --source "$d" --no-git --redact --no-banner
  rm -rf "$d"
  [ "$status" -ne 0 ]   # nonzero = leak found
}

@test "baseline suppresses the known findings (clean tree scan is quiet)" {
  # The whole repo scanned WITH the baseline must report no NEW leaks.
  run bash -c "cd '$REPO' && gitleaks detect --source . --baseline-path .gitleaks-baseline.json --config .gitleaks.toml --redact --no-banner 2>&1"
  echo "$output" | grep -q "no leaks found"
}
