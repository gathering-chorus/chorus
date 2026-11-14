// @domain: awareness
// @test-type: unit — keysFromRegistry and NoteKeys in note-keys.ts against an injected registry body; no live services
// @card: #4965
// @owner: kade
/**
 * #4965 — a registry answer that is not a row list gives no keys, so every
 * note reads unverified; a NoteKeys built with the default period reads the
 * registry at start and stops cleanly.
 */
import { keysFromRegistry, NoteKeys, NOSTR_TYPE } from './note-keys';

const PUB = 'a'.repeat(64);
const row = { keyType: NOSTR_TYPE, forPrincipal: 'principal-kade', nostrPubkey: PUB };

describe('#4965 note-keys reads only a registry row list', () => {
  test('negative: a body with no data list yields no keys at all', () => {
    expect(keysFromRegistry({ data: 'not a list' }).size).toBe(0);
    expect(keysFromRegistry(null).size).toBe(0);
  });

  test('a body with a row list yields the key for its principal', () => {
    expect([...keysFromRegistry({ data: [row] })]).toEqual([[PUB, 'kade']]);
  });

  test('with the default period, the start read verifies a note and stop() ends the re-reads', async () => {
    const reads: number[] = [];
    const keys = new NoteKeys(() => { reads.push(1); return Promise.resolve({ data: [row] }); });
    try {
      await keys.refresh();
      expect(await keys.check(PUB, 'kade')).toBe('ok');
      expect(await keys.check(PUB, 'wren')).toBe('wrong-key');
    } finally {
      keys.stop();
    }
    expect(reads.length).toBeGreaterThanOrEqual(1);
  });

  test('negative: a registry that answers with no list leaves every note unverified', async () => {
    const keys = new NoteKeys(() => Promise.resolve({ data: {} }), { everyMs: 0 });
    await keys.refresh();
    expect(await keys.check(PUB, 'kade')).toBe('unverified');
    keys.stop();
  });
});
