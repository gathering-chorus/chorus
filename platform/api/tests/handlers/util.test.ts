/**
 * handler util `run()` — unit tests (#2173 AC4).
 */

import { run } from '../../src/handlers/util';

describe('run()', () => {
  test('sync function return → 200 with body', async () => {
    const r = await run(() => ({ ok: true }));
    expect(r.status).toBe(200);
    expect(r.body).toEqual({ ok: true });
  });

  test('async function return → 200 with resolved body', async () => {
    const r = await run(async () => 42);
    expect(r.status).toBe(200);
    expect(r.body).toBe(42);
  });

  test('sync throw → 500 with error message', async () => {
    const r = await run(() => { throw new Error('boom'); });
    expect(r.status).toBe(500);
    expect(r.body).toEqual({ error: 'boom' });
  });

  test('async reject → 500 with error message', async () => {
    const r = await run(async () => { throw new Error('nope'); });
    expect(r.status).toBe(500);
    expect(r.body).toEqual({ error: 'nope' });
  });

  test('non-Error throw stringifies', async () => {
    const r = await run(() => { throw 'raw string'; });
    expect(r.status).toBe(500);
    expect(r.body).toEqual({ error: 'raw string' });
  });

  test('null return is a valid 200 body', async () => {
    const r = await run(() => null);
    expect(r.status).toBe(200);
    expect(r.body).toBeNull();
  });

  test('undefined return is a valid 200 body', async () => {
    const r = await run(() => undefined);
    expect(r.status).toBe(200);
    expect(r.body).toBeUndefined();
  });
});
