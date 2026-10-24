// @domain: awareness
// @test-type: unit — buildTree / joinPrompts / sliceFor in platform/pulse/src/pulse-tree.ts over fact fixtures; no live services
// @card: #4638
// @owner: wren
import { buildTree, joinPrompts, sliceFor, QUIET_AFTER_S, type TreeInput } from './pulse-tree';

const ACTORS = ['jeff', 'wren', 'silas', 'kade', 'abby-normal'];
const NOW = '2026-10-24T20:50:00Z';

function input(): TreeInput {
  return {
    now: NOW,
    actors: ACTORS,
    lines: [
      { id: 'm1', from: 'jeff', to: ['wren'], text: '/pull 4638', sentAt: '2026-10-24T20:41:00Z' },
      { id: 'm2', from: 'jeff', to: [], text: 'i spent all day trying to get u', sentAt: '2026-10-24T20:41:03Z' },
      { id: 'm3', from: 'jeff', to: ['silas'], text: '@silas r u stuk', sentAt: '2026-10-24T20:47:00Z' },
      { id: 'm4', from: 'bridge', to: ['wren'], text: '[alert] a message was stored twice', sentAt: '2026-10-24T20:48:00Z' },
      { id: 'm5', from: '', to: ['wren'], text: 'no author', sentAt: '2026-10-24T20:48:30Z' },
    ],
    turns: [
      { id: 't1', role: 'wren', run: 'wren-run-1', startedAt: '2026-10-24T20:41:04Z', lastCallAt: '2026-10-24T20:49:30Z', wokenByLine: 'm2', card: 4638 },
      { id: 't2', role: 'silas', run: 'silas-run-1', startedAt: '2026-10-24T20:47:01Z', endedAt: '2026-10-24T20:47:30Z', lastCallAt: '2026-10-24T20:47:20Z', wokenByLine: 'm3', card: 4710 },
    ],
    runs: [{ id: 'cicd-034x', card: 4638, pipeline: 'cicd', step: 'test', status: 'running', startedByLine: 'm1' }],
    notes: [
      { id: 'n1', turn: 't1', card: 4638, text: 'radius measured: no Prompt, no Turn', at: '2026-10-24T20:45:00Z' },
      { id: 'n2', turn: 't1', text: 'presence rows: delete was refused', at: '2026-10-24T20:44:00Z' },
    ],
  };
}

describe('#4638 the live tree', () => {
  it('(Jeff 23 14:56) a burst within a few seconds is one prompt, and a turn woken by any line answers the joined prompt', () => {
    const t = buildTree(input());
    const p = t.prompts.find(x => x.lines.includes('m1'))!;
    expect(p.lines).toEqual(['m1', 'm2']);
    expect(p.text).toBe('/pull 4638\ni spent all day trying to get u');
    expect(p.roles.map(r => r.role)).toEqual(['wren']);
    expect(p.roles[0].turns.map(x => x.id)).toEqual(['t1']);
  });

  it('negative proof: lines further apart than the burst gap are two prompts', () => {
    const i = input();
    i.lines[1].sentAt = '2026-10-24T20:41:30Z';
    expect(joinPrompts(i.lines, ACTORS).prompts.filter(p => p.by === 'jeff')).toHaveLength(3);
  });

  it('(Jeff 19 09:32) card is the carrier: its run and its note hang off the card, a card-less note stays on the turn', () => {
    const wren = buildTree(input()).prompts.find(x => x.lines.includes('m1'))!.roles[0];
    expect(wren.cards).toHaveLength(1);
    expect(wren.cards[0].card).toBe(4638);
    expect(wren.cards[0].notes.map(n => n.id)).toEqual(['n1']);
    expect(wren.turns[0].notes.map(n => n.id)).toEqual(['n2']);
  });

  it('(Jeff 19 09:58) a run is reachable from the prompt that started it', () => {
    const p = buildTree(input()).prompts.find(x => x.id === 'prompt-m1')!;
    expect(p.runs.map(r => r.id)).toEqual(['cicd-034x']);
    expect(p.roles[0].cards[0].runs.map(r => r.id)).toEqual(['cicd-034x']);
  });

  it('(Jeff 19 09:26) each role says moving or quiet with seconds since its last call', () => {
    const roles = buildTree(input()).roles;
    expect(roles).toEqual([
      { role: 'silas', state: 'quiet', secondsSinceLastCall: 160 },
      { role: 'wren', state: 'moving', secondsSinceLastCall: 30 },
    ]);
  });

  it('negative proof: a role with no call for the quiet threshold shows quiet, one second short shows moving', () => {
    const i = input();
    i.turns[0].lastCallAt = new Date(Date.parse(NOW) - QUIET_AFTER_S * 1000).toISOString();
    expect(buildTree(i).roles.find(r => r.role === 'wren')!.state).toBe('quiet');
    i.turns[0].lastCallAt = new Date(Date.parse(NOW) - (QUIET_AFTER_S - 1) * 1000).toISOString();
    expect(buildTree(i).roles.find(r => r.role === 'wren')!.state).toBe('moving');
  });

  it('(#4637) a non-actor never makes a prompt; it is counted by name, an empty sender as (none)', () => {
    const t = buildTree(input());
    expect(t.prompts.every(p => ACTORS.includes(p.by))).toBe(true);
    expect(t.refusedSenders).toEqual({ bridge: 1, '(none)': 1 });
  });

  it('newest prompt first, and only the last N', () => {
    const t = buildTree({ ...input(), lastN: 1 });
    expect(t.prompts.map(p => p.id)).toEqual(['prompt-m3']);
  });

  it("(Jeff 19 09:34) a role's slice holds only its own branch and is smaller than the whole tree", () => {
    const t = buildTree(input());
    const s = sliceFor(t, 'silas');
    expect(s.me!.state).toBe('quiet');
    expect(s.prompts.map(p => p.id)).toEqual(['prompt-m3']);
    expect(JSON.stringify(s)).not.toContain('4638');
    expect(JSON.stringify(s).length).toBeLessThan(JSON.stringify(t).length);
  });
});

describe('#4638 the tree over pulse (live wiring)', () => {
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const request = require('supertest');
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const { createApp } = require('./service');
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const { MessageStore } = require('./store');
  const recent = (s: number) => new Date(Date.now() - s * 1000).toISOString();
  const actorStates = () => ({
    getActorStates: () => ({
      derivedAt: new Date().toISOString(),
      actors: [
        { aboutPrincipal: 'principal-jeff', stream: [{ action: 'jeff.input.surfaced', timestamp: recent(1) }] },
        { aboutPrincipal: 'principal-wren', stream: [{ action: 'agent.action', timestamp: recent(10) }, { action: 'agent.action', timestamp: recent(40) }] },
        { aboutPrincipal: 'principal-silas', stream: [{ action: 'agent.action', timestamp: recent(600) }] },
        { aboutPrincipal: 'principal-kade', stream: [] },
      ],
    }),
  });

  it('GET /api/tree joins a burst sent to two roles into one prompt, the shared line kept once', async () => {
    const store = new MessageStore(':memory:');
    store.sendJeffInput('wren', '@wren @silas pull 4638');
    store.sendJeffInput('silas', '@wren @silas pull 4638');
    store.sendJeffInput('wren', 'and pair');
    const res = await request(createApp(store, undefined, undefined, actorStates())).get('/api/tree');
    expect(res.status).toBe(200);
    expect(res.body.prompts).toHaveLength(1);
    expect(res.body.prompts[0].text).toBe('@wren @silas pull 4638\nand pair');
    expect(res.body.prompts[0].to.sort()).toEqual(['silas', 'wren']);
    expect(res.body.prompts[0].roles.map((r: { role: string; state: string }) => `${r.role}:${r.state}`)).toEqual(['silas:quiet', 'wren:moving']);
  });

  it('each role says moving or quiet from its newest action; a role with no action is not invented', async () => {
    const store = new MessageStore(':memory:');
    const res = await request(createApp(store, undefined, undefined, actorStates())).get('/api/tree');
    const by = Object.fromEntries(res.body.roles.map((r: { role: string }) => [r.role, r]));
    expect(by.wren.state).toBe('moving');
    expect(by.wren.secondsSinceLastCall).toBeLessThan(20);
    expect(by.silas.state).toBe('quiet');
    expect(by.kade).toBeUndefined();
  });

  it("GET /api/tree/:role is that role's slice only", async () => {
    const store = new MessageStore(':memory:');
    store.sendJeffInput('silas', '@silas r u stuk');
    const app = createApp(store, undefined, undefined, actorStates());
    const wren = (await request(app).get('/api/tree/wren')).body;
    const silas = (await request(app).get('/api/tree/silas')).body;
    expect(wren.me.role).toBe('wren');
    expect(wren.prompts).toEqual([]);
    expect(silas.prompts.map((p: { text: string }) => p.text)).toEqual(['@silas r u stuk']);
  });

  it('negative proof: with no actor-state service the tree has no roles and refuses every line by name', async () => {
    const store = new MessageStore(':memory:');
    store.sendJeffInput('wren', 'hello');
    const res = await request(createApp(store)).get('/api/tree');
    expect(res.body.roles).toEqual([]);
    expect(res.body.prompts).toEqual([]);
    expect(res.body.refusedSenders).toEqual({ jeff: 1 });
  });
});
