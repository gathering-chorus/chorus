// @domain: awareness
// @test-type: unit
/**
 * #4744 slice 3c — the rows pulse writes so the tree lives in the store (awareness-rows.ts):
 * the same facts the tree is built from, shaped to the Prompt, RoleState and Turn fields.
 */
import { readFileSync } from 'fs';
import { join } from 'path';
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
  // pulse's own row per role, never the role-named row a role may already own (#4757)
  expect(r.map(x => x.name)).toEqual(['observed-wren', 'observed-silas']);
  expect(r.map(x => [x.aboutPrincipal, x.activity])).toEqual([['principal-wren', 'acting'], ['principal-silas', 'idle']]);
  // the declared state is not derived here, and the row says so instead of guessing
  expect(r.every(x => x.stateValue === 'unmeasured' && x.stateSource === 'observed')).toBe(true);
});

// NEGATIVE PROOF: an actor never seen acting gets no row, rather than a made-up time
test('negative proof: no last call, no RoleState row', () => {
  expect(rows().roleStates.map(x => x.aboutPrincipal)).not.toContain('principal-jeff');
});

test('row names keep only what the door accepts', () => {
  expect(rowName('wren-2026-10-26T09:50:10.000Z')).toBe('wren-2026-10-26t09-50-10-000z');
  expect(rowName('wren:t1')).toBe('wren-t1');
});

/**
 * #4757 — every row pulse writes uses only fields its class has in the generated pulse API
 * (its model.rs, written by athena-gen from the model). On 10-26 the Prompt rows carried a
 * `comment` the Prompt shape does not have and pulse-api refused all of them as off-model.
 */
function fieldsOf(className: string): Set<string> {
  const model = readFileSync(join(__dirname, '../../../directing/products/pulse/api/src/model.rs'), 'utf8');
  const start = model.indexOf(`name: "${className}", kind:`);
  if (start < 0) throw new Error(`pulse-api serves no ${className}`);
  const block = model.slice(start, model.indexOf('\n    },', start));
  // name, label and status are the door's own keys on every class (rules.rs)
  return new Set(['name', 'label', 'status', ...[...block.matchAll(/FieldDef \{ name: "([A-Za-z]+)"/g)].map(m => m[1])]);
}

test('every Prompt, RoleState and Turn row uses only its class fields in the generated API', () => {
  const r = rows();
  for (const [cls, list] of [['Prompt', r.prompts], ['RoleState', r.roleStates], ['Turn', r.turns]] as const) {
    const allowed = fieldsOf(cls);
    expect(list.length).toBeGreaterThan(0);
    for (const row of list) expect(Object.keys(row).filter(k => !allowed.has(k))).toEqual([]);
  }
});

// NEGATIVE PROOF: a field the class does not have is caught (the 10-26 `comment` on Prompt)
test('negative proof: an off-model field on a row is caught', () => {
  expect(fieldsOf('Prompt').has('comment')).toBe(false);
});
