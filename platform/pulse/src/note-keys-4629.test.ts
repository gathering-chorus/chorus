// @test-type: unit — registry bodies in memory and a fake clock; no network.
// @domain: messages
// @card: #4629
// @owner: wren
/**
 * #4629 AC4 — a note under the wrong key is refused. Whose key signed a room
 * note comes from the key registry (graph-only, as Clearing v2 since #4652).
 * Fail closed: no registry read, or a read older than 30 min, is unverified
 * (Silas 06:33/06:36). The bridge is right only on its own copies (pulse-<id>).
 */
import { randomUUID } from 'crypto';
import { keysFromRegistry, keyVerdict, NoteKeys, NOSTR_TYPE } from './note-keys';

const WREN = 'a'.repeat(64), SILAS = 'b'.repeat(64), BRIDGE = 'c'.repeat(64), STRANGER = 'd'.repeat(64);
const row = (who: string, pubkey: string) => ({ forPrincipal: `principal-${who}`, keyType: NOSTR_TYPE, nostrPubkey: pubkey });
const body = (...rows: unknown[]) => ({ data: rows });
const REG = body(row('wren', WREN), row('silas', SILAS), row('bridge', BRIDGE));

describe('#4629 AC4 keyVerdict', () => {
  const keys = keysFromRegistry(REG);

  test('the author signing with their own registry key is ok', () => {
    expect(keyVerdict(WREN, 'wren', keys, 'trace-x')).toBe('ok');
  });

  test('NEGATIVE PROOF: a valid key claiming another author is wrong-key', () => {
    expect(keyVerdict(SILAS, 'wren', keys, 'trace-x')).toBe('wrong-key');
  });

  test('the bridge is ok on its own copy, and NEGATIVE PROOF: wrong on a sender note', () => {
    expect(keyVerdict(BRIDGE, 'wren', keys, 'pulse-42')).toBe('ok');
    expect(keyVerdict(BRIDGE, 'wren', keys, `trace-${randomUUID()}`)).toBe('wrong-key');
  });

  test('NEGATIVE PROOF: no registry read is unverified, never ok', () => {
    expect(keyVerdict(WREN, 'wren', null, 'trace-x')).toBe('unverified');
  });

  test('a row of another key type, or a name that is not a principal, names no one', () => {
    expect(keysFromRegistry(body({ ...row('wren', WREN), keyType: 'rsa' }, { forPrincipal: 'wren', keyType: NOSTR_TYPE, nostrPubkey: SILAS })).size).toBe(0);
  });
});

describe('#4629 AC4 NoteKeys reads the registry and fails closed', () => {
  test('an unknown key gets one more read before it is called wrong, and that read is rate-limited', async () => {
    let now = 1_000_000;
    let reads = 0;
    let reg: unknown = body(row('wren', WREN));
    const k = new NoteKeys(() => { reads += 1; return Promise.resolve(reg); }, { everyMs: 0, now: () => now });
    await k.refresh();
    const base = reads;
    reg = body(row('wren', WREN), row('silas', SILAS));          // silas was added since the last read
    expect(await k.check(SILAS, 'silas', 'trace-x')).toBe('ok');
    expect(reads).toBe(base + 1);
    expect(await k.check(STRANGER, 'silas', 'trace-y')).toBe('wrong-key'); // within the minute: no new read
    expect(reads).toBe(base + 1);
    now += 61_000;
    await k.check(STRANGER, 'silas', 'trace-z');
    expect(reads).toBe(base + 2);
    k.stop();
  });

  test('NEGATIVE PROOF: a registry that never answers leaves every note unverified', async () => {
    const k = new NoteKeys(() => Promise.reject(new Error('door down')), { everyMs: 0 });
    expect(await k.check(WREN, 'wren', 'trace-x')).toBe('unverified');
    k.stop();
  });

  test('NEGATIVE PROOF: a read older than 30 minutes is not trusted, so a revoked key stops being ok', async () => {
    let now = 1_000_000;
    let up = true;
    const k = new NoteKeys(() => (up ? Promise.resolve(REG) : Promise.reject(new Error('door down'))), { everyMs: 0, now: () => now });
    await k.refresh();
    expect(await k.check(WREN, 'wren', 'trace-x')).toBe('ok');
    up = false;
    now += 31 * 60_000;
    expect(await k.check(WREN, 'wren', 'trace-x')).toBe('unverified');
    k.stop();
  });
});
