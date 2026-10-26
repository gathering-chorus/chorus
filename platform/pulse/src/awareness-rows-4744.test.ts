// @domain: awareness
// @test-type: unit
/**
 * #4744 slice 3c — the rows pulse writes so the tree lives in the store (awareness-rows.ts):
 * the same facts the tree is built from, shaped to the Prompt, RoleState and Turn fields.
 */
import { awarenessRows, rowName } from './awareness-rows';

const now = '2026-10-26T10:00:00.000Z';
const lines = [
  { id: '58226', from: 'jeff', to: ['wren'], text: 'pull 4744', sentAt: '2026-10-26T09:50:00.000Z' },
  { id: '58227', from: 'jeff', to: ['wren'], text: 'and pair', sentAt: '2026-10-26T09:50:03.000Z' },
  { id: '58228', from: 'ghost', to: ['wren'], text: 'hi', sentAt: '2026-10-26T09:51:00.000Z' },
];
const spine = [
  { event: 'agent.turn.started', role: 'wren', timestamp: '2026-10-26T05:50:10.000-0400', turn_id: 'wren:t1', woken_by: 'line', woken_by_line: '58227' },
  { event: 'pipeline.run.started', role: 'wren', timestamp: '2026-10-26T09:50:20.000Z', run_id: 'r1', card_id: '4744' },
  { event: 'agent.turn.ended', role: 'wren', timestamp: '2026-10-26T09:52:00.000Z' },
  { event: 'agent.turn.started', role: 'unknown', timestamp: '2026-10-26T09:53:00.000Z', turn_id: 'u1' },
];
const rows = () => awarenessRows({ now, actors: ['jeff', 'wren', 'silas'], lines, spine, lastCalls: { wren: '2026-10-26T09:59:30.000Z', silas: '2026-10-26T09:40:00.000Z' } });

test('a burst of Jeff lines is one Prompt row, named by its first line, sent by and to principals', () => {
  const p = rows().prompts;
  expect(p).toHaveLength(1);
  expect(p[0]).toMatchObject({ name: '58226', promptText: 'pull 4744\nand pair', promptedBy: 'principal-jeff', promptTo: ['principal-wren'], promptedAt: '2026-10-26T09:50:00Z' });
});

test('a turn links the prompt that woke it and the card its run named', () => {
  const t = rows().turns;
  expect(t).toEqual([expect.objectContaining({
    name: 'wren-t1', turnOf: 'principal-wren', answersPrompt: 'prompt-58226', onCard: 'card-4744',
    turnStartedAt: '2026-10-26T09:50:10Z', turnEndedAt: '2026-10-26T09:52:00Z',
  })]);
});

// NEGATIVE PROOF: a turn logged by a role that is not an actor ("unknown", #4740) is never written
test('negative proof: a non-actor turn makes no row', () => {
  expect(rows().turns.map(t => t.turnOf)).not.toContain('principal-unknown');
  expect(rows().turns).toHaveLength(1);
});

test('each actor with a last call gets one RoleState row: acting when it called within the quiet window', () => {
  const r = rows().roleStates;
  expect(r.map(x => [x.aboutPrincipal, x.activity])).toEqual([['principal-wren', 'acting'], ['principal-silas', 'idle']]);
  // the declared state is not derived here, and the row says so instead of guessing
  expect(r.every(x => x.stateValue === 'unmeasured' && x.stateSource === 'observed')).toBe(true);
});

// NEGATIVE PROOF: an actor never seen acting gets no row, rather than a made-up time
test('negative proof: no last call, no RoleState row', () => {
  expect(rows().roleStates.map(x => x.name)).not.toContain('jeff');
});

test('row names keep only what the door accepts', () => {
  expect(rowName('wren-2026-10-26T09:50:10.000Z')).toBe('wren-2026-10-26t09-50-10-000z');
  expect(rowName('wren:t1')).toBe('wren-t1');
});
