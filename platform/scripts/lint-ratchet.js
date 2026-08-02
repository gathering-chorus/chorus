#!/usr/bin/env node
/* eslint-env node */
// #2462: per-rule ESLint ratchet.
// Runs eslint on the chorus workspace, compares per-rule counts against .eslint-baseline.json.
// Fails if any rule count climbs above baseline. --regenerate overwrites baseline.
//
// Exit codes:
//   0 — pass (every rule count at or below baseline)
//   1 — ratchet violation (a rule count climbed)
//   2 — rule firing that isn't in baseline (new rule added without regenerate)
//   3 — eslint invocation failed

const { execFileSync } = require('child_process');
const fs = require('fs');
const path = require('path');

const REPO_ROOT = process.env.LINT_RATCHET_ROOT
  ? path.resolve(process.env.LINT_RATCHET_ROOT)
  : path.resolve(__dirname, '..', '..');
const BASELINE_PATH = process.env.LINT_RATCHET_BASELINE
  ? path.resolve(process.env.LINT_RATCHET_BASELINE)
  : path.join(REPO_ROOT, '.eslint-baseline.json');
const LINT_GLOBS = process.env.LINT_RATCHET_GLOBS
  ? process.env.LINT_RATCHET_GLOBS.split(':')
  : [
    'platform/**/src/**/*.ts',
    'directing/**/src/**/*.ts',
    'platform/**/tests/**/*.ts',
    'directing/**/tests/**/*.ts',
  ];

const regenerate = process.argv.includes('--regenerate');

function runEslint() {
  // Prefer caller-supplied eslint binary (hermetic tests), else resolve from REPO_ROOT's node_modules.
  const bin = process.env.LINT_RATCHET_ESLINT_BIN
    || path.join(REPO_ROOT, 'node_modules', '.bin', 'eslint');
  const binForTests = fs.existsSync(bin)
    ? bin
    : path.join(__dirname, '..', '..', 'node_modules', '.bin', 'eslint');
  try {
    const out = execFileSync(
      binForTests,
      ['--format=json', ...LINT_GLOBS],
      { cwd: REPO_ROOT, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 },
    );
    return JSON.parse(out);
  } catch (e) {
    // eslint exits non-zero when it finds problems. stdout still holds JSON.
    if (e.stdout) {
      try { return JSON.parse(e.stdout); } catch (_parseErr) { /* fallthrough */ }
    }
    process.stderr.write(`lint-ratchet: eslint invocation failed: ${e.message}\n`);
    process.exit(3);
  }
}

function countByRule(results) {
  const counts = {};
  for (const file of results) {
    for (const msg of file.messages || []) {
      const rule = msg.ruleId || '__syntax__';
      counts[rule] = (counts[rule] || 0) + 1;
    }
  }
  return counts;
}

function writeBaseline(counts) {
  const payload = {
    generatedAt: new Date().toISOString(),
    note: 'Per-rule ESLint count ratchet (#2462). Counts may only decrease. Run `npm run lint:baseline` to regenerate after a legitimate drop.',
    counts,
  };
  fs.writeFileSync(BASELINE_PATH, JSON.stringify(payload, null, 2) + '\n');
}

function readBaseline() {
  if (!fs.existsSync(BASELINE_PATH)) return null;
  return JSON.parse(fs.readFileSync(BASELINE_PATH, 'utf8'));
}

function main() {
  const results = runEslint();
  const current = countByRule(results);
  const totalCurrent = Object.values(current).reduce((a, b) => a + b, 0);

  if (regenerate) {
    writeBaseline(current);
    process.stdout.write(`lint-ratchet: baseline regenerated (${Object.keys(current).length} rules, ${totalCurrent} total violations).\n`);
    process.exit(0);
  }

  const baseline = readBaseline();
  if (!baseline) {
    process.stderr.write('lint-ratchet: no baseline found. Run `npm run lint:baseline` to create one.\n');
    // #3428 — final stdout line in the nightly's `N pass, N fail` shape so the
    // suite parser reports a real status instead of masking it as DID NOT RUN.
    process.stdout.write('lint-ratchet summary: 0 pass, 1 fail (no baseline)\n');
    process.exit(1);
  }

  const violations = [];
  const newRules = [];
  for (const [rule, count] of Object.entries(current)) {
    const limit = baseline.counts[rule];
    if (limit === undefined) {
      newRules.push({ rule, count });
    } else if (count > limit) {
      violations.push({ rule, count, limit, delta: count - limit });
    }
  }

  const drops = [];
  for (const [rule, limit] of Object.entries(baseline.counts)) {
    const count = current[rule] || 0;
    if (count < limit) drops.push({ rule, count, limit, delta: limit - count });
  }

  // #3710 — ORDER MATTERS here, and it misleads if you don't know it. Climbed
  // counts are reported FIRST and exit 1, so the newRules branch below only runs
  // once every tracked count is at or under baseline. A rule can therefore fire
  // for months while staying invisible: absent from the baseline, but the ratchet
  // never reaches the check that would say so. When you finally pay the counts
  // down and a batch of "new rule IDs" appears, that is FIRST ACTIVATION — those
  // rules were already firing; they are not suppressions being aggregated.
  // Don't assume either reading: run eslint on main and compare per-rule counts.
  // (Silas's review question on #3710 — three rules surfaced exactly this way,
  // and all three were confirmed already firing on main.)
  // #3721 — REPORT BOTH CLASSES, THEN EXIT. Each block used to exit
  // immediately, so the new-rules check was unreachable while any count was over
  // — the masking the #3710 note above describes. It is not only a reading
  // hazard: it costs a full fix-and-rerun cycle per layer. On this card three
  // count violations were paid down, and only THEN did `__syntax__: 2` appear,
  // having fired the whole time (confirmed on main, per the note's own advice).
  // A run should say everything wrong with it in one pass. Exit codes keep their
  // meanings — 1 = counts climbed, which dominates; 2 = new rules only — so
  // callers keying on them are unaffected, and #3428's parseable summary is
  // still the last stdout line, now emitted exactly once.
  if (violations.length > 0) {
    process.stderr.write('lint-ratchet: FAIL — rule counts climbed above baseline:\n');
    for (const v of violations) {
      process.stderr.write(`  ${v.rule}: ${v.count} (baseline ${v.limit}, +${v.delta})\n`);
    }
  }

  if (newRules.length > 0) {
    process.stderr.write('lint-ratchet: FAIL — new rule IDs firing that aren\'t in baseline:\n');
    for (const n of newRules) {
      process.stderr.write(`  ${n.rule}: ${n.count}\n`);
    }
    process.stderr.write('Run `npm run lint:baseline` after review to adopt new rules.\n');
  }

  if (violations.length > 0 || newRules.length > 0) {
    const over = violations.reduce((a, v) => a + v.delta, 0);
    const parts = [];
    if (violations.length > 0) parts.push(`${violations.length} rules over baseline, +${over} violations`);
    if (newRules.length > 0) parts.push(`${newRules.length} new rules not in baseline`);
    process.stdout.write(
      `lint-ratchet summary: 0 pass, ${violations.length + newRules.length} fail (${parts.join('; ')})\n`,
    );
    process.exit(violations.length > 0 ? 1 : 2);
  }

  process.stdout.write(`lint-ratchet: PASS (${totalCurrent} violations, ${Object.keys(current).length} rules).\n`);
  if (drops.length > 0) {
    process.stdout.write('Drops since baseline:\n');
    for (const d of drops.sort((a, b) => b.delta - a.delta)) {
      process.stdout.write(`  ${d.rule}: ${d.count} (was ${d.limit}, -${d.delta})\n`);
    }
    process.stdout.write('Run `npm run lint:baseline` to lock the drops.\n');
  }
  // #3428 — parseable summary LAST (stdout) so the nightly reports a real PASS.
  process.stdout.write(`lint-ratchet summary: 1 pass, 0 fail (${totalCurrent} violations, ${Object.keys(current).length} rules, at/under baseline)\n`);
}

main();
