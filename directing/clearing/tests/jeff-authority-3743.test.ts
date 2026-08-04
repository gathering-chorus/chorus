// @test-type: unit — injected fakes only; no Fuseki, no live session, brings its own world.
/**
 * #3743 — jeff-authority binds to the principal, not to a name string.
 * Negative proofs (per #3734) for the two live-source lines Kade verified:
 *   router.ts:100/108 — startsWith('jeff') granted authority to any name
 *                       merely beginning with "jeff" ('jeffrey' read as Jeff)
 *   tailer.ts:95      — an ABSENT acceptor defaulted to 'jeff' (fail-open to
 *                       the highest authority in the system)
 */

import { MessageRouter } from '../src/router';
import { ChorusLogTailer } from '../src/tailer';

describe('#3743 router: jeff is an exact identity, not a prefix', () => {
  let router: MessageRouter;
  beforeEach(() => { router = new MessageRouter(); });

  test('NEGATIVE: from "jeffrey" must NOT classify as jeff-input', () => {
    router.ingest({ from: 'jeffrey', text: 'hello room', ts: new Date().toISOString(), type: 'role-response' });
    const last = router.getRecent(5, true).pop()!;
    expect(last.type).not.toBe('jeff-input');
  });

  test('NEGATIVE: an "Accepted #123" from "jeffrey" must NOT be treated as Jeff\'s acceptance', () => {
    router.ingest({ from: 'jeffrey', text: 'Accepted #123 — some card', ts: new Date().toISOString(), type: 'role-response' });
    const last = router.getRecent(5, true).pop()!;
    expect(last.type).not.toBe('accept-request');
  });

  test('from "jeff" (exact) still classifies as jeff-input', () => {
    router.ingest({ from: 'jeff', text: 'hello room', ts: new Date().toISOString(), type: 'role-response' });
    const last = router.getRecent(5, true).pop()!;
    expect(last.type).toBe('jeff-input');
  });
});

describe('#3743 tailer: an absent acceptor is refused, never resolved to jeff', () => {
  test('NEGATIVE: card.accepted with no acceptor field must NOT attribute to jeff', () => {
    const ingested: Array<{ from: string }> = [];
    const fakeRouter = { ingest: (m: { from: string }) => { ingested.push(m); } };
    const tailer = new ChorusLogTailer(fakeRouter as never);
    // Reach the private handler the way the class routes spine entries to it.
    (tailer as never as { handleCardAccepted: (p: Record<string, unknown>, role: string) => void })
      .handleCardAccepted({ card_id: '999', title: 't' }, 'wren');
    expect(ingested).toHaveLength(1);
    expect(ingested[0].from).not.toBe('jeff');
  });

  test('an explicit acceptor is preserved', () => {
    const ingested: Array<{ from: string }> = [];
    const fakeRouter = { ingest: (m: { from: string }) => { ingested.push(m); } };
    const tailer = new ChorusLogTailer(fakeRouter as never);
    (tailer as never as { handleCardAccepted: (p: Record<string, unknown>, role: string) => void })
      .handleCardAccepted({ card_id: '999', acceptor: 'jeff' }, 'wren');
    expect(ingested[0].from).toBe('jeff');
  });
});
