// @domain: awareness
// @test-type: unit
/**
 * #4744 slice 3c — pulse writes the tree's rows through pulse-api, and only what changed
 * (every replace keeps a version, so rewriting each tick would flood provenance).
 */
import { AwarenessWriter, signature, type Send } from './awareness-writer';
import type { AwarenessRows } from './awareness-rows';

const rows = (activity = 'acting', observedAt = '2026-10-26T09:59:30Z'): AwarenessRows => ({
  prompts: [{ name: '58226', promptText: 'pull 4744', promptedAt: '2026-10-26T09:50:00Z', promptedBy: 'principal-jeff' }],
  roleStates: [{ name: 'wren', aboutPrincipal: 'principal-wren', activity, observedAt, stateValue: 'unmeasured' }],
  turns: [{ name: 'wren-t1', turnOf: 'principal-wren', answersPrompt: 'prompt-58226', turnStartedAt: '2026-10-26T09:50:10Z', lastCallAt: '2026-10-26T09:52:00Z' }],
});

function fake(refuse: Array<[string, number]> = []) {
  const codes = new Map(refuse);
  const calls: string[] = [];
  const store = new Set<string>();
  const answer = (method: string, path: string, name: string): number => {
    const key = method === 'POST' ? `${path}/${name}` : path;
    const refused = codes.get(key);
    if (refused) return refused;
    if (method === 'POST') { if (store.has(key)) return 409; store.add(key); return 201; }
    return store.has(key) ? 200 : 404;
  };
  const send: Send = (method, path, body) => {
    calls.push(`${method} ${path}`);
    return Promise.resolve(answer(method, path, String(body.name)));
  };
  return { send, calls };
}

test('prompts are written before the turns that link them, each row once', async () => {
  const f = fake();
  const w = new AwarenessWriter(f.send, () => Promise.resolve('cred'));
  const r = await w.write(rows());
  expect(f.calls).toEqual(['POST /pulse/awareness/prompts', 'POST /pulse/awareness/rolestates', 'POST /pulse/awareness/turns']);
  expect(r).toMatchObject({ made: 3, failed: 0 });
});

// NEGATIVE PROOF: the same facts on the next tick write nothing
test('negative proof: an unchanged tick writes nothing', async () => {
  const f = fake();
  const w = new AwarenessWriter(f.send, () => Promise.resolve('cred'));
  await w.write(rows());
  f.calls.length = 0;
  const r = await w.write(rows('acting', '2026-10-26T09:59:50Z')); // same 5-minute window
  expect(f.calls).toEqual([]);
  expect(r.unchanged).toBe(3);
});

test('a role going quiet is replaced in place (POST answers 409, then PUT)', async () => {
  const f = fake();
  const w = new AwarenessWriter(f.send, () => Promise.resolve('cred'));
  await w.write(rows());
  f.calls.length = 0;
  const r = await w.write(rows('idle'));
  expect(f.calls).toEqual(['POST /pulse/awareness/rolestates', 'PUT /pulse/awareness/rolestates/wren']);
  expect(r.updated).toBe(1);
});

test('a refused row is counted and named, and tried again next tick', async () => {
  const f = fake([['/pulse/awareness/turns/wren-t1', 422]]);
  const w = new AwarenessWriter(f.send, () => Promise.resolve('cred'));
  const r = await w.write(rows());
  expect(r.failed).toBe(1);
  expect(r.failures).toEqual(['turns/wren-t1: HTTP 422']);
  f.calls.length = 0;
  await w.write(rows());
  expect(f.calls).toEqual(['POST /pulse/awareness/turns']);
});

test('no credential: nothing is sent, every row is a named failure', async () => {
  const f = fake();
  const w = new AwarenessWriter(f.send, () => Promise.reject(new Error('no cred file')));
  const r = await w.write(rows());
  expect(f.calls).toEqual([]);
  expect(r).toMatchObject({ failed: 3, failures: ['credential: no cred file'] });
});

test('a role state is written again when its last call reaches a new 5-minute window', () => {
  const row = { activity: 'acting', observedAt: '2026-10-26T09:59:30Z' };
  expect(signature('roleStates', row)).toBe(signature('roleStates', { ...row, observedAt: '2026-10-26T09:58:00Z' }));
  expect(signature('roleStates', row)).not.toBe(signature('roleStates', { ...row, observedAt: '2026-10-26T10:00:30Z' }));
});
