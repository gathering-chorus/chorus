/**
 * context-roles handler tests (#2234 Step 3).
 */

import { fetchContextRoles, type ContextRolesDeps } from '../../src/handlers/context-roles';

function stubSparql(): ContextRolesDeps['sparql'] {
  return { query: async () => ({ results: { bindings: [] } }) };
}

describe('fetchContextRoles', () => {
  it('returns all three known roles in stable order', async () => {
    const r = await fetchContextRoles({
      sparql: stubSparql(),
      readState: (role) => ({ role, state: 'idle' }),
      tailSpine: () => null,
    }, '/api/chorus/context/roles');
    expect(r.status).toBe(200);
    const names = r.body.data.roles.map((x) => x.name);
    expect(names).toEqual(['silas', 'wren', 'kade']);
  });

  it('merges state record + spine event per role', async () => {
    const r = await fetchContextRoles({
      sparql: stubSparql(),
      readState: (role) =>
        role === 'silas'
          ? { role, state: 'building', card: 2234 }
          : { role, state: 'idle' },
      tailSpine: (role) =>
        role === 'silas'
          ? { timestamp: '2026-04-19T14:15:00-04:00', role, event: 'card.demo.started' }
          : null,
    }, '/api/chorus/context/roles');
    const silas = r.body.data.roles.find((x) => x.name === 'silas')!;
    expect(silas.state).toBe('building');
    expect(silas.card).toBe(2234);
    expect(silas.lastEvent).toBe('card.demo.started');
    expect(silas.lastActivity).toBe('2026-04-19T14:15:00-04:00');
    const wren = r.body.data.roles.find((x) => x.name === 'wren')!;
    expect(wren.lastEvent).toBeNull();
  });

  it('missing state record → state "unknown", card/gemba null', async () => {
    const r = await fetchContextRoles({
      sparql: stubSparql(),
      readState: () => null,
      tailSpine: () => null,
    }, '/api/chorus/context/roles');
    const kade = r.body.data.roles.find((x) => x.name === 'kade')!;
    expect(kade.state).toBe('unknown');
    expect(kade.card).toBeNull();
    expect(kade.gemba).toBeNull();
    expect(kade.lastEvent).toBeNull();
  });

  it('envelope is system-scoped (no domain / subdomain / step / product fields)', async () => {
    const r = await fetchContextRoles({
      sparql: stubSparql(),
      readState: () => null,
      tailSpine: () => null,
    }, '/api/chorus/context/roles');
    expect(r.body).not.toHaveProperty('domain');
    expect(r.body).not.toHaveProperty('subdomain');
    // stampHeader with null domainId returns only timestamp, so envelope has
    // only timestamp + source + data at the top level.
    const keys = Object.keys(JSON.parse(JSON.stringify(r.body))).sort();
    expect(keys).toEqual(['data', 'source', 'timestamp']);
  });

  it('source URL is passed through verbatim to the envelope', async () => {
    const r = await fetchContextRoles({
      sparql: stubSparql(),
      readState: () => null,
      tailSpine: () => null,
    }, '/api/chorus/context/roles');
    expect(r.body.source).toBe('/api/chorus/context/roles');
  });

  it('stale=false when lastActivity is just under 15min threshold', async () => {
    const now = new Date('2026-04-19T12:00:00Z');
    const justFresh = new Date(now.getTime() - 14 * 60 * 1000 - 59000).toISOString(); // 14:59 ago
    const r = await fetchContextRoles({
      sparql: stubSparql(),
      readState: () => ({ role: 'silas', state: 'building' }),
      tailSpine: (role) => role === 'silas' ? { timestamp: justFresh, role: 'silas', event: 'tool' } : null,
      now: () => now,
    }, '/api/chorus/context/roles');
    const silas = r.body.data.roles.find((x) => x.name === 'silas')!;
    expect(silas.stale).toBe(false);
  });

  it('stale=true when lastActivity is just over 15min threshold', async () => {
    const now = new Date('2026-04-19T12:00:00Z');
    const justStale = new Date(now.getTime() - 15 * 60 * 1000 - 1000).toISOString(); // 15:01 ago
    const r = await fetchContextRoles({
      sparql: stubSparql(),
      readState: () => ({ role: 'silas', state: 'building' }),
      tailSpine: (role) => role === 'silas' ? { timestamp: justStale, role: 'silas', event: 'tool' } : null,
      now: () => now,
    }, '/api/chorus/context/roles');
    const silas = r.body.data.roles.find((x) => x.name === 'silas')!;
    expect(silas.stale).toBe(true);
  });

  it('state with gemba surfaces gemba field', async () => {
    const r = await fetchContextRoles({
      sparql: stubSparql(),
      readState: (role) =>
        role === 'kade'
          ? { role, state: 'observing', gemba: 'silas' }
          : { role, state: 'idle' },
      tailSpine: () => null,
    }, '/api/chorus/context/roles');
    const kade = r.body.data.roles.find((x) => x.name === 'kade')!;
    expect(kade.state).toBe('observing');
    expect(kade.gemba).toBe('silas');
    expect(kade.card).toBeNull();
  });

  // --- #2193 AC5: derived_state + drift_state ---

  it('derived_state populated when readInferred returns a record', async () => {
    const nowMs = Date.UTC(2026, 3, 21, 12, 0, 0);
    const r = await fetchContextRoles({
      sparql: stubSparql(),
      readState: (role) => ({ role, state: 'building', card: 2193 }),
      tailSpine: () => null,
      readInferred: (role) => role === 'kade'
        ? { card: 2193, state: 'building', ts: Math.floor(nowMs / 1000) - 30, wip_count: 1, recent_commit_count: 2 }
        : null,
      now: () => new Date(nowMs),
    }, '/api/chorus/context/roles');
    const kade = r.body.data.roles.find((x) => x.name === 'kade')!;
    expect(kade.derived_state).toEqual({
      state: 'building',
      card: 2193,
      wip_count: 1,
      recent_commit_count: 2,
    });
    expect(kade.drift_state).toEqual({
      divergent: false,
      inferred_stale: false,
      card_declared: 2193,
      card_inferred: 2193,
    });
  });

  it('drift_state.divergent=true when declared and inferred cards disagree (and inferred is fresh)', async () => {
    const nowMs = Date.UTC(2026, 3, 21, 12, 0, 0);
    const r = await fetchContextRoles({
      sparql: stubSparql(),
      readState: (role) => ({ role, state: 'building', card: 2193 }),
      tailSpine: () => null,
      readInferred: () => ({ card: 2188, state: 'building', ts: Math.floor(nowMs / 1000) - 30 }),
      now: () => new Date(nowMs),
    }, '/api/chorus/context/roles');
    const kade = r.body.data.roles.find((x) => x.name === 'kade')!;
    expect(kade.drift_state.divergent).toBe(true);
    expect(kade.drift_state.card_declared).toBe(2193);
    expect(kade.drift_state.card_inferred).toBe(2188);
  });

  it('drift_state.inferred_stale=true when inferred.ts > 5min old', async () => {
    const nowMs = Date.UTC(2026, 3, 21, 12, 0, 0);
    const r = await fetchContextRoles({
      sparql: stubSparql(),
      readState: (role) => ({ role, state: 'building', card: 2193 }),
      tailSpine: () => null,
      readInferred: () => ({ card: 2193, state: 'building', ts: Math.floor(nowMs / 1000) - 400 }),  // 400s ago
      now: () => new Date(nowMs),
    }, '/api/chorus/context/roles');
    const kade = r.body.data.roles.find((x) => x.name === 'kade')!;
    expect(kade.drift_state.inferred_stale).toBe(true);
    // Stale inferred suppresses divergent signal (can't compare stale data)
    expect(kade.drift_state.divergent).toBe(false);
  });

  it('derived_state is null when readInferred returns null', async () => {
    const r = await fetchContextRoles({
      sparql: stubSparql(),
      readState: (role) => ({ role, state: 'building', card: 2193 }),
      tailSpine: () => null,
      readInferred: () => null,
    }, '/api/chorus/context/roles');
    const kade = r.body.data.roles.find((x) => x.name === 'kade')!;
    expect(kade.derived_state).toBeNull();
    expect(kade.drift_state.inferred_stale).toBe(true);
  });

  it('backward-compatible — omitted readInferred behaves like null (all stale)', async () => {
    const r = await fetchContextRoles({
      sparql: stubSparql(),
      readState: (role) => ({ role, state: 'building', card: 2193 }),
      tailSpine: () => null,
    }, '/api/chorus/context/roles');
    const kade = r.body.data.roles.find((x) => x.name === 'kade')!;
    expect(kade.derived_state).toBeNull();
    expect(kade.drift_state.inferred_stale).toBe(true);
  });
});
