/**
 * chorus-pulse-latest handler — unit tests (#2188).
 */
import { fetchChorusPulseLatest } from '../../src/handlers/chorus-pulse-latest';

describe('fetchChorusPulseLatest (#2188)', () => {
  test('readPulse returns null → 404', () => {
    const r = fetchChorusPulseLatest({ readPulse: () => null });
    expect(r.status).toBe(404);
    const body = r.body as { error: string };
    expect(body.error).toMatch(/no pulse snapshot/i);
  });

  test('valid JSON → 200 with parsed body', () => {
    const pulse = { roles: { wren: { state: 'waiting' } } };
    const r = fetchChorusPulseLatest({ readPulse: () => JSON.stringify(pulse) });
    expect(r.status).toBe(200);
    expect(r.body).toEqual(pulse);
  });

  test('unparseable JSON → 500 with error', () => {
    const r = fetchChorusPulseLatest({ readPulse: () => '{not-json' });
    expect(r.status).toBe(500);
    const body = r.body as { error: string };
    expect(typeof body.error).toBe('string');
    expect(body.error.length).toBeGreaterThan(0);
  });

  test('readPulse throws → 500 with error message', () => {
    const r = fetchChorusPulseLatest({ readPulse: () => { throw new Error('disk gone'); } });
    expect(r.status).toBe(500);
    const body = r.body as { error: string };
    expect(body.error).toBe('disk gone');
  });

  test('non-Error throw stringifies', () => {
    const r = fetchChorusPulseLatest({ readPulse: () => { throw 'EACCES'; } });
    expect(r.status).toBe(500);
    const body = r.body as { error: string };
    expect(body.error).toBe('EACCES');
  });

  test('empty-object pulse still 200', () => {
    const r = fetchChorusPulseLatest({ readPulse: () => '{}' });
    expect(r.status).toBe(200);
    expect(r.body).toEqual({});
  });
});
