#!/usr/bin/env bats
# @test-type: unit — retirement guard
load test_helper
# 3710-no-committed-dist-symlink.bats — guard a recurring incident.
#
# #3279 (Wren) untracked platform/workflow-engine/dist, calling the committed
# self-loop symlink a "recurring board-outage root cause". #3710 (me)
# reintroduced it with a `git add -A` and caught it before the land.
#
# The mechanism is worth stating because it is why the existing rule did not
# help: .gitignore carries `dist/`, which matches a DIRECTORY. A SYMLINK named
# `dist` is a FILE, so the pattern misses it entirely and `git add -A` happily
# stages it. The symlink also pointed at an absolute path on Jeff's machine,
# so any other checkout — CI included — would resolve it to nothing.
#
# Twice is a pattern, so this is a test rather than another comment.

@test "no build-output symlink is tracked in git" {
  cd "$CHORUS_ROOT"
  # -l lists tracked SYMLINKS specifically (mode 120000), which is the form that
  # evades the directory ignore rules.
  run bash -c "git ls-files -s | awk '\$1 == \"120000\" {print \$4}' | grep -E '(^|/)(dist|node_modules|target)$' || true"
  [ -z "$output" ]
}

@test "platform/workflow-engine/dist is not tracked (#3279)" {
  cd "$CHORUS_ROOT"
  run bash -c "git ls-files platform/workflow-engine/dist"
  [ -z "$output" ]
}

@test "gitignore covers the dist SYMLINK form, not just the directory" {
  cd "$CHORUS_ROOT"
  # `dist/` alone is what let this through twice.
  grep -qE '^platform/workflow-engine/dist$' .gitignore
}
