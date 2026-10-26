// @domain: awareness
// @test-type: unit
/**
 * #4757 — each WIP card's step on pulse's header, from /cws's own lines (chorus-werk-status --json).
 */
import { cardStepRows, type CwsRow } from './card-steps';

const wip = (over: Partial<CwsRow> = {}): CwsRow => ({
  card: '4747', role: 'kade', step: 'land: borg-queen-proves', live: true, since: '2026-10-26T22:30:00Z', run: 'cicd run 2', ...over,
});

test("a WIP card's row carries /cws's step and when it began, live", () => {
  const [r] = cardStepRows([wip()], []);
  expect(r).toMatchObject({
    name: 'step-4747', cardId: '4747', aboutPrincipal: 'principal-kade', pipelineStep: 'land: borg-queen-proves',
    stepSince: '2026-10-26T22:30:00Z', runOrdinal: 'cicd run 2', stepLive: 'live',
  });
  expect(String(r.comment)).toContain('#4747 kade land: borg-queen-proves');
});

test('a card with no run yet has no since and no run, never a made-up time', () => {
  const [r] = cardStepRows([wip({ step: 'no run yet', live: false, since: '', run: '' })], []);
  expect(r.stepSince).toBeUndefined();
  expect(r.runOrdinal).toBeUndefined();
  expect(r.pipelineStep).toBe('no run yet');
});

// NEGATIVE PROOF: a card that left WIP is marked gone, so the header (stepLive=live) drops it;
// one still in WIP is never marked gone
test('negative proof: a card that left WIP goes, one still in WIP stays live', () => {
  const stored = [
    { name: 'step-4747', cardId: '4747', aboutPrincipal: 'principal-kade', pipelineStep: 'build', stepSince: '', runOrdinal: '' },
    { name: 'step-4704', cardId: '4704', aboutPrincipal: 'principal-silas', pipelineStep: 'landed', stepSince: '2026-10-26T20:00:00Z', runOrdinal: 'cicd run 1' },
  ];
  const rows = cardStepRows([wip()], stored);
  expect(rows.map(r => `${r.name} ${r.stepLive}`)).toEqual(['step-4747 live', 'step-4704 gone']);
  const gone = rows[1];
  expect(gone).toMatchObject({ cardId: '4704', pipelineStep: 'landed', stepSince: '2026-10-26T20:00:00Z' });
  expect(Object.values(gone)).not.toContain('');
});
