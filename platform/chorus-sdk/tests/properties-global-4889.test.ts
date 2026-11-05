// @test-type: unit — the one TS global-property reader, door faked
// @domain: properties
// @card: #4889
// @owner: silas
// Exercises platform/chorus-sdk/properties/global.js and its types, platform/chorus-sdk/properties/global.d.ts (Kade 07:06: one shared reader,
// the same errors as the Rust properties-resolver global_value, a negative proof each).
import { globalValueFromBody, readGlobal, doorUrl } from '../properties/global';

const row = (over: Record<string, unknown> = {}) => ({
  propertyKey: 'pulse.announce.machine_cooldown_ms', propertyScope: 'global', propertyValue: '1800000', ...over,
});
const K = 'pulse.announce.machine_cooldown_ms';
const door = (status: number, body: unknown) =>
  (async () => ({ ok: status === 200, status, json: async () => body })) as unknown as typeof fetch;

describe('#4889 globalValueFromBody', () => {
  it('reads a global row', () => {
    expect(globalValueFromBody({ data: [row()] }, K)).toEqual({ ok: true, value: '1800000' });
  });
  it('NEGATIVE PROOF: no row is an error that names the key, never a default', () => {
    const r = globalValueFromBody({ data: [row({ propertyKey: 'other' })] }, K);
    expect(r).toEqual({ ok: false, where: 'row', error: expect.stringContaining(`no property row sets ${K}`) });
  });
  it('NEGATIVE PROOF: an empty value is an error', () => {
    expect(globalValueFromBody({ data: [row({ propertyValue: '  ' })] }, K)).toEqual({ ok: false, where: 'row', error: `${K}: the row has no value` });
  });
  it('NEGATIVE PROOF: a row that is not global scope is an error', () => {
    expect(globalValueFromBody({ data: [row({ propertyScope: 'role' })] }, K)).toEqual({ ok: false, where: 'row', error: `${K}: the row is not global scope` });
  });
  it('NEGATIVE PROOF: a body with no data list is an error', () => {
    expect(globalValueFromBody({ nope: 1 }, K).ok).toBe(false);
  });
});

describe('#4889 readGlobal through the door', () => {
  it('reads the door', async () => {
    expect(await readGlobal(K, { door: 'http://d', fetchFn: door(200, { data: [row()] }) })).toEqual({ ok: true, value: '1800000' });
  });
  it('NEGATIVE PROOF: a door that does not answer is an error, never a value', async () => {
    const down = (async () => { throw new Error('ECONNREFUSED'); }) as unknown as typeof fetch;
    const r = await readGlobal(K, { door: 'http://d', fetchFn: down });
    expect(r).toEqual({ ok: false, where: 'door', error: expect.stringContaining('did not answer (ECONNREFUSED)') });
  });
  it('NEGATIVE PROOF: a door that answers 500 is an error', async () => {
    expect(await readGlobal(K, { door: 'http://d', fetchFn: door(500, {}) })).toEqual({ ok: false, where: 'door', error: `${K}: the properties door answered 500` });
  });
  it('the door is OWL_URL when set, else athena-make on the Library', () => {
    const was = process.env.OWL_URL;
    delete process.env.OWL_URL;
    expect(doorUrl()).toBe('http://127.0.0.1:3360');
    process.env.OWL_URL = 'http://x:1';
    expect(doorUrl()).toBe('http://x:1');
    if (was === undefined) delete process.env.OWL_URL; else process.env.OWL_URL = was;
  });
});
