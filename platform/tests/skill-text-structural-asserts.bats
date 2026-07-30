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

# #3710 — these three demanded a literal `chorus-log <event>` line in the skill
# markdown. That was the right assertion when the SKILL told a human which
# command to run; #3443/#3116 moved emission INTO the verbs (werk-demo presents
# and emits; werk-pull emits card.pulled), so the skills now DECLARE the event
# instead of instructing it. The contract worth guarding is that the skill still
# names the spine event it is responsible for — a reader must be able to learn
# what lands on the spine — not which binary types it.

@test "/demo skill markdown declares the card.demo.started spine event" {
  DEMO_MD="$SKILLS_DIR/demo/SKILL.md"
  [ -f "$DEMO_MD" ] || skip "/demo/SKILL.md not present at $DEMO_MD"

  if ! grep -q "card\.demo\.started" "$DEMO_MD"; then
    echo "/demo skill no longer names the card.demo.started spine event."
    false
  fi
}

@test "/demo skill markdown declares the feedback gather as part of the ceremony" {
  DEMO_MD="$SKILLS_DIR/demo/SKILL.md"
  [ -f "$DEMO_MD" ] || skip "/demo/SKILL.md not present"

  # Jeff made the feedback gather mandatory (2026-04-30). The old assertion
  # required the literal "[feedback]" nudge-prefix from when the demoer sent it
  # by hand; werk-demo now fires the verbatim gather itself, so the skill
  # describes it in prose. Guard that the ceremony still includes it at all.
  if ! grep -qi "feedback" "$DEMO_MD"; then
    echo "/demo skill markdown no longer mentions the feedback gather."
    echo "  The feedback step is mandatory per Jeff (2026-04-30)."
    false
  fi
}

@test "/pull skill markdown declares the card.pulled spine event" {
  PULL_MD="$SKILLS_DIR/pull/SKILL.md"
  [ -f "$PULL_MD" ] || skip "/pull/SKILL.md not present"

  if ! grep -q "card\.pulled" "$PULL_MD"; then
    echo "/pull skill no longer names the card.pulled spine event."
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
