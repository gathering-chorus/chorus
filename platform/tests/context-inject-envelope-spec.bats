#!/usr/bin/env bats
# context-inject-envelope-spec.bats
#
# SPECIFICATION lock (not a regression lock). Expected state: RED until the
# per-prompt context-injection envelope is wired with pulse + spine + athena
# sections.
#
# Background (2026-04-17): Jeff has repeatedly asked for pulse, spine, and
# Athena context to be injected into every prompt envelope. Today's audit of
# platform/services/chorus-hooks/src/hooks/context_inject.rs showed:
#   - The <context-synthesis> block includes Chorus hybrid search + memory
#     hits. That's it.
#   - Pulse is invoked as a SIDE EFFECT (spawn shim pulse) which writes
#     /tmp/pulse-latest.json — its contents are not placed into the envelope.
#   - Spine: not referenced.
#   - Athena/Domain: not referenced.
# The session-start envelope (context_cache.rs writing /tmp/session-start-<role>.md)
# does include pulse + Chorus search, but only on boot, not on every prompt.
# Jeff's "works then breaks" experience maps to "works at boot, gone for every
# prompt after."
#
# This file locks the TARGET shape, not today's shape. It is committed RED so
# that whoever wires the per-prompt injection (tomorrow's session or later) can
# TDD against a concrete specification. When the wiring is complete, all four
# tests below turn green. Until then, red is the correct and declared state.
#
# The grep patterns intentionally require the header string to appear inside a
# String-building call (context.push_str, writeln!, format!, etc.) so that a
# comment alone cannot satisfy the spec. Implementation has to emit the header
# into the synthesis block for real.

CHORUS_ROOT="/Users/jeffbridwell/CascadeProjects/chorus"
TARGET="${CHORUS_ROOT}/platform/services/chorus-hooks/src/hooks/context_inject.rs"

@test "spec: target file exists" {
  [ -f "$TARGET" ]
}

@test "spec: synthesis block emits a Pulse section header" {
  # Must appear inside a push_str / writeln / format call, not a comment
  grep -qE '(push_str|writeln!|format!).*"## Pulse' "$TARGET" || {
    echo "Missing Pulse section in context-synthesis block" >&2
    echo "Expected a line like: context.push_str(\"## Pulse\\n\")" >&2
    false
  }
}

@test "spec: synthesis block emits a Spine section header" {
  grep -qE '(push_str|writeln!|format!).*"## Spine' "$TARGET" || {
    echo "Missing Spine section in context-synthesis block" >&2
    echo "Expected a line like: context.push_str(\"## Spine\\n\")" >&2
    false
  }
}

@test "spec: synthesis block emits an Athena or Domain section header" {
  grep -qE '(push_str|writeln!|format!).*"## (Athena|Domain)' "$TARGET" || {
    echo "Missing Athena/Domain section in context-synthesis block" >&2
    echo "Expected a line like: context.push_str(\"## Athena\\n\") or ## Domain" >&2
    false
  }
}
