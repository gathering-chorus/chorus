// @test-type: unit — notes are signed in-process with derived test keys; no relay, no tailer, no service.
/**
 * #4445 — Jeff, 2026-10-07: "i still never have seen messages from abby in clearing".
 *
 * The Clearing shows role replies by tailing Claude Code transcripts, for wren,
 * silas and kade only. Abby runs Gemini, so she has no transcript to tail; her
 * replies reach the relay as reply notes (t=reply) that the room never asked for.
 * The room now subscribes to reply notes and renders those from untailed roles,
 * and skips a tailed role's reply note so nothing renders twice.
 */

import { buildRoomIdentity, inboundToClearing } from '../src/buzz-room';
import { roomFilter } from '../src/room-replay';
import { derivedSigner } from '../src/buzz-signer';
import type { NostrEvent } from '../src/buzz-bridge';

const SECRET = 'test-secret-4445';

const identity = buildRoomIdentity(['jeff', 'wren', 'silas', 'kade', 'abby-normal'], {
  pubkeyFor: (actor) => derivedSigner(actor, SECRET).pubkey,
  serviceSigner: () => derivedSigner('bridge', SECRET),
});

// A note shaped like chorus-hooks' buzz_reply.rs reply event.
const replyNote = (actor: string, content: string): NostrEvent => {
  const signer = derivedSigner(actor, SECRET);
  const created_at = 1791399651;
  const tags = [['t', 'reply'], ['role', actor], ['hash', 'b55a3e5a98a4d3cc'], ['seq', '3']];
  const { id, sig } = signer.signEvent(JSON.stringify([0, signer.pubkey, created_at, 1, tags, content]));
  return { id, pubkey: signer.pubkey, created_at, kind: 1, tags, content, sig };
};

describe('#4445 Abby\'s replies reach the Clearing from the relay', () => {
  test('the room subscribes to reply notes as well as its topic', () => {
    expect(roomFilter('team', null)['#t']).toEqual(['team', 'reply']);
    expect(roomFilter('team', 1791399000)['#t']).toEqual(['team', 'reply']);
  });

  test('an Abby reply note renders as a role response from abby-normal', () => {
    const r = inboundToClearing(replyNote('abby-normal', 'Abby here, testing the Clearing.'), identity, new Set());
    expect(r.disposition).toBe('rendered');
    expect(r.msg).toMatchObject({ from: 'abby-normal', text: 'Abby here, testing the Clearing.', type: 'role-response' });
  });

  test('NEGATIVE PROOF: a tailed role\'s reply note is skipped, so its reply is not shown twice', () => {
    for (const role of ['wren', 'silas', 'kade']) {
      const r = inboundToClearing(replyNote(role, 'already tailed'), identity, new Set());
      expect(r.disposition).toBe('tailed');
      expect(r.msg).toBeNull();
    }
  });

  test('a tailed role\'s note on the topic (not a reply) still renders', () => {
    const signer = derivedSigner('wren', SECRET);
    const tags = [['t', 'team']];
    const { id, sig } = signer.signEvent(JSON.stringify([0, signer.pubkey, 1791399651, 1, tags, 'room note']));
    const r = inboundToClearing({ id, pubkey: signer.pubkey, created_at: 1791399651, kind: 1, tags, content: 'room note', sig }, identity, new Set());
    expect(r.disposition).toBe('rendered');
  });
});
