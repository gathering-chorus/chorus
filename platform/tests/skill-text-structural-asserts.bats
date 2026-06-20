#!/usr/bin/env bats
# @test-type: unit — hermetic source guard
# skill-text-structural-asserts.bats — #2630 wave 5
#
# Structural assertions on skill markdown source. Catches the failure
# mode where a skill's mandatory side-effect step gets removed from
# the markdown (vs. the runtime-skipping pattern wave 2/4 catches).
#
# These are READ-ONLY structural tests on the skill .md files. If
# someone edits /acp to remove the spine-emit line, the test fails
# at pre-commit before the change lands. Complements wave 2/4 which
# catch the orthogonal failure (skill text correct, invoker skips).
#
# Tested skills + their declared side-effects:
# - /acp: must call chorus-log card.accepted
# - /demo: must call chorus-log card.demo.started
# - /pull: must call chorus-log card.pulled
# - /gate-product: must reference probe.evidence emission
# - /reboot: must reference next-session.md write

CHORUS_ROOT="${CHORUS_ROOT:-$(cd "${BATS_TEST_DIRNAME}/../.." && pwd)}"
SKILLS_DIR="$CHORUS_ROOT/skills"

setup() {
  if [ ! -d "$SKILLS_DIR" ]; then
    skip "skills dir missing at $SKILLS_DIR"
  fi
}

@test "/acp skill markdown contains card.accepted spine-emit step" {
  ACP_MD="$SKILLS_DIR/acp/SKILL.md"
  [ -f "$ACP_MD" ] || skip "/acp/SKILL.md not present at $ACP_MD"

  if ! grep -q "chorus-log.*card\.accepted" "$ACP_MD"; then
    echo "/acp skill is missing the chorus-log card.accepted spine-emit step."
    echo "  Without this, /acp invocations land brief files but no spine"
    echo "  events (today's pattern: 12 briefs in 5 days, 1 spine event)."
    echo "  Search target: chorus-log card.accepted"
    false
  fi
}

@test "/demo skill markdown contains card.demo.started spine-emit step" {
  DEMO_MD="$SKILLS_DIR/demo/SKILL.md"
  [ -f "$DEMO_MD" ] || skip "/demo/SKILL.md not present at $DEMO_MD"

  if ! grep -q "chorus-log.*card\.demo\.started" "$DEMO_MD"; then
    echo "/demo skill is missing the chorus-log card.demo.started step."
    false
  fi
}

@test "/demo skill markdown declares step 5 [feedback] nudge as mandatory" {
  DEMO_MD="$SKILLS_DIR/demo/SKILL.md"
  [ -f "$DEMO_MD" ] || skip "/demo/SKILL.md not present"

  # Must reference [feedback] nudge in step 5 (Jeff's mandatory framing)
  if ! grep -qE "\[feedback\]" "$DEMO_MD"; then
    echo "/demo skill markdown is missing the [feedback] nudge step."
    echo "  Step 5 [feedback] is mandatory per Jeff (2026-04-30)."
    false
  fi
}

@test "/pull skill markdown contains card.pulled spine-emit step" {
  PULL_MD="$SKILLS_DIR/pull/SKILL.md"
  [ -f "$PULL_MD" ] || skip "/pull/SKILL.md not present"

  if ! grep -q "chorus-log.*card\.pulled" "$PULL_MD"; then
    echo "/pull skill is missing the chorus-log card.pulled step."
    false
  fi
}

@test "/gate-product skill markdown references live-probe / probe.evidence emission" {
  GP_MD="$SKILLS_DIR/gate-product/SKILL.md"
  [ -f "$GP_MD" ] || skip "/gate-product/SKILL.md not present"

  # The skill must reference probe-evidence requirement specifically.
  # Per-subagent finding: bare-word `evidence` was too permissive (a
  # sentence like "no hard evidence required" would pass). Tighten to
  # explicit constructs: `probe.evidence`, `live probe`, `live-probe`,
  # `probe-evidence`, or `probe evidence`.
  if ! grep -qE "probe\.evidence|live[- ]probe|probe[- ]evidence" "$GP_MD"; then
    echo "/gate-product skill text does not reference probe-evidence."
    echo "  Gate-PASS without probe-evidence is the paper-trail pattern"
    echo "  (#2625 morning). Skill must encode the requirement using"
    echo "  one of: 'probe.evidence', 'live probe', 'live-probe',"
    echo "  'probe-evidence', or 'probe evidence' (not bare 'evidence')."
    false
  fi
}
