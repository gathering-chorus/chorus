// @test-type: unit — signal is fixture-data: pure parser + follower with fake io
// #3883 — werk.phase events from the act run log. Fixture lines are copied
// VERBATIM from a real run log (3880-…-32-land.log, 2026-08-14) — the parser
// is proven against what act actually prints, not what we wish it printed.
import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { parsePhaseTransitions, followRun, PhaseEvent } from '../src/werk-phase';

const REAL = [
  '[werk/werk] ⭐ Run Main build',
  '[werk/werk]   ✅  Success - Main build [1m3.24599575s]',
  '[werk/werk] ⭐ Run Main test',
  '[werk/werk]   | werk-test: rc=0 (BLOCKING — a red affected unit halts the land)',
  '[werk/werk]   ❌  Failure - Main test [16m43.914797542s]',
].join('\n');

test('#3883 parses start/pass/fail from verbatim act output', () => {
  assert.deepEqual(parsePhaseTransitions(REAL), [
    { phase: 'build', state: 'running' },
    { phase: 'build', state: 'pass', elapsed: '1m3.24599575s' },
    { phase: 'test', state: 'running' },
    { phase: 'test', state: 'fail', elapsed: '16m43.914797542s' },
  ]);
});

// NEGATIVE PROOF (#3734): noise must parse to NOTHING — a parser that matched
// pipe-lines would flood the spine with garbage phases.
test('#3883 act noise and MEMBRANE panics produce zero events', () => {
  const noise = [
    '[werk/werk]   | MEMBRANE REFUSED (#3615): build context …',
    '[werk/werk]   |   ✅ gc-one-home-per-subject — 0 violations',
    'time="2026-08-14" level=info msg="Using docker host"',
  ].join('\n');
  assert.equal(parsePhaseTransitions(noise).length, 0);
});

test('#3883 followRun emits each transition once and stops at WERK_EXIT', async () => {
  const chunks = [REAL, '[werk/werk] 🏁  Job failed\nWERK_EXIT=1\n', 'SHOULD NEVER BE READ'];
  let reads = 0;
  const emitted: PhaseEvent[] = [];
  const stop = followRun(
    async () => chunks[reads++] ?? '',
    async (e) => { emitted.push(e); },
    20, // fast interval for the test — real wiring uses 15s
  );
  await new Promise(r => setTimeout(r, 120));
  stop();
  assert.equal(emitted.length, 4);
  assert.equal(reads, 2); // stopped at WERK_EXIT — no further reads
});
