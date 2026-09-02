// @test-type: unit — injects readEvents/listWipCards/sparql stubs; no spine file, no live service, brings its own world.
/**
 * context-roles handler tests — #4028: /api/chorus/context/roles serves ONLY
 * the state derived from the streams. There is no declared file to read and
 * no "unknown" to answer.
 */

import { fetchContextRoles, type ContextRolesDeps } from '../../src/handlers/context-roles';
import type { SpineLine } from '../../src/derive-role-state';

const T0 = Date.parse('2026-09-02T15:00:00-04:00');
const at = (offsetMin: number) => new Date(T0 - offsetMin * 60_000).toISOString();

function stubSparql(): ContextRolesDeps['sparql'] {
  return { query: async () => ({ results: { bindings: [] } }) };
}

function deps(over: Partial<ContextRolesDeps> = {}): ContextRolesDeps {
  return {
    sparql: stubSparql(),
    readEvents: () => [],
    listWipCards: () => [],
    now: () => new Date(T0),
    ...over,
  };
}

describe('fetchContextRoles (#4028 — derived, never declared)', () => {
  it('returns all three known roles in stable order', async () => {
    const r = await fetchContextRoles(deps(), '/api/chorus/context/roles');
    expect(r.status).toBe(200);
    expect(r.body.data.roles.map((x) => x.name)).toEqual(['silas', 'wren', 'kade']);
  });

  it('a role with tool calls in the window is building on its board card; lastEvent/lastActivity come from the streams', async () => {
    const events: SpineLine[] = [
      { timestamp: at(1), role: 'silas', event: 'hook.decision' },
      { timestamp: at(3), role: 'silas', event: 'context.inject.request' },
    ];
    const r = await fetchContextRoles(deps({
      readEvents: (role) => events.filter((e) => e.role === role),
      listWipCards: () => [{ id: 4058, owner: 'Silas' }],
    }), '/api/chorus/context/roles');
    const silas = r.body.data.roles.find((x) => x.name === 'silas')!;
    expect(silas.state).toBe('building');
    expect(silas.card).toBe(4058);
    expect(silas.lastEvent).toBe('hook.decision');
    expect(silas.lastActivity).toBe(at(1));
    expect(silas.stale).toBe(false);
    expect(silas.source).toBe('streams');
  });

  it('a role with no events is idle, not "unknown" — and stale is true', async () => {
    const r = await fetchContextRoles(deps(), '/api/chorus/context/roles');
    const kade = r.body.data.roles.find((x) => x.name === 'kade')!;
    expect(kade.state).toBe('idle');
    expect(kade.card).toBeNull();
    expect(kade.gemba).toBeNull();
    expect(kade.stale).toBe(true);
    expect(r.body.data.roles.some((x) => x.state === 'unknown')).toBe(false);
  });

  it('AC3 negative proof: a role.state.changed event saying building, with silent streams for 20 min, is idle', async () => {
    const r = await fetchContextRoles(deps({
      readEvents: (role) => role === 'wren'
        ? [{ timestamp: at(20), role: 'wren', event: 'role.state.changed', payload: 'state=building' }]
        : [],
      listWipCards: () => [{ id: 4045, owner: 'Wren' }],
    }), '/api/chorus/context/roles');
    const wren = r.body.data.roles.find((x) => x.name === 'wren')!;
    expect(wren.state).toBe('idle');
  });

  it('blocked comes from the stream with its detail, and the row carries it', async () => {
    const r = await fetchContextRoles(deps({
      readEvents: (role) => role === 'wren'
        ? [{ timestamp: at(2), role: 'wren', event: 'role.blocked', detail: 'waiting on Jeff' }]
        : [],
    }), '/api/chorus/context/roles');
    const wren = r.body.data.roles.find((x) => x.name === 'wren')!;
    expect(wren.state).toBe('blocked');
    expect(wren.detail).toBe('waiting on Jeff');
  });

  it('consumer shape is unchanged: derived_state and drift_state still exist, drift is never divergent (nothing to drift against)', async () => {
    const r = await fetchContextRoles(deps({
      readEvents: (role) => role === 'kade' ? [{ timestamp: at(1), role: 'kade', event: 'agent.action' }] : [],
      listWipCards: () => [{ id: 4063, owner: 'Kade' }],
    }), '/api/chorus/context/roles');
    const kade = r.body.data.roles.find((x) => x.name === 'kade')!;
    expect(kade.derived_state).toEqual({ state: 'building', card: 4063, wip_count: 1, recent_commit_count: null });
    expect(kade.drift_state.divergent).toBe(false);
    expect(kade.drift_state.card_inferred).toBe(4063);
  });

  it('envelope is system-scoped and the source URL passes through', async () => {
    const r = await fetchContextRoles(deps(), '/api/chorus/context/roles');
    const keys = Object.keys(JSON.parse(JSON.stringify(r.body))).sort();
    expect(keys).toEqual(['data', 'source', 'timestamp']);
    expect(r.body.source).toBe('/api/chorus/context/roles');
  });

  it('role field mirrors name so consumers can key off either', async () => {
    const r = await fetchContextRoles(deps(), '/api/chorus/context/roles');
    for (const row of r.body.data.roles) expect(row.role).toBe(row.name);
  });
});
