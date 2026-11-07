// @test-type: unit — fake fetch and fake timers; no live services
// @domain: messages
// @card: #4692
// @owner: silas
/** #4692 — the roles door is read the way a nudge reads it, and a refused read is reported. */
import { rolesDoorTick, startRolesDoorWatch, FetchLike } from './peers';

const ROLES = { kind: 'Role', data: [{ name: 'silas', roleKind: 'agent' }, { name: 'jeff', roleKind: 'human' }] };
// What node clients got from :3360 on 10-21 21:46–05:53 for a roles request.
const DISCOVERY = { kind: 'Discovery', primitives: [{ kind: 'Role', collection: '/v1/roles/roles' }] };
const answering = (body: unknown, ok = true, status = 200): FetchLike => () => Promise.resolve({ ok, status, json: () => Promise.resolve(body) });
const throwing = (msg: string): FetchLike => () => Promise.reject(new Error(msg));

test('a door answering its roles reads ok', async () => {
  expect(await rolesDoorTick(answering(ROLES), 'http://door')).toEqual({ ok: true, peers: 2 });
});

test('NEGATIVE PROOF: the Discovery page instead of the roles list is a failed read (the 10-21 outage)', async () => {
  const r = await rolesDoorTick(answering(DISCOVERY), 'http://door');
  expect(r).toEqual({ ok: false, error: 'the roles door answered with no data list' });
});

test('NEGATIVE PROOF: a busy door and a dead socket are failed reads', async () => {
  expect((await rolesDoorTick(answering({ error: 'busy' }, false, 502), 'http://door')).ok).toBe(false);
  expect(await rolesDoorTick(throwing('read ECONNRESET'), 'http://door')).toEqual({ ok: false, error: 'read ECONNRESET' });
});

describe('the watch', () => {
  beforeEach(() => jest.useFakeTimers());
  afterEach(() => jest.useRealTimers());

  test('reports every failed tick and stays quiet while the door answers', async () => {
    const failures: string[] = [];
    let body: unknown = ROLES;
    const stop = startRolesDoorWatch((e) => { failures.push(e); }, 1000, () => Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve(body) }));
    await jest.advanceTimersByTimeAsync(3000);
    expect(failures).toEqual([]);
    body = DISCOVERY;
    await jest.advanceTimersByTimeAsync(2000);
    expect(failures).toEqual(['the roles door answered with no data list', 'the roles door answered with no data list']);
    stop();
  });

  test('a report that throws never stops the watch', async () => {
    let calls = 0;
    const stop = startRolesDoorWatch(() => { calls++; throw new Error('spine down'); }, 1000, throwing('down'));
    await jest.advanceTimersByTimeAsync(3000);
    expect(calls).toBe(3);
    stop();
  });
});
