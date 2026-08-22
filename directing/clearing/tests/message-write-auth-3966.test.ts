// @test-type: unit — pure auth decision; no socket, no HTTP, no WebID lookup.
/**
 * #3966 — /api/message was anonymous (bound to *:3470, anyone on the LAN could
 * post into the room whose transcripts index into team memory). The write now
 * requires a caller identity: BRIDGE_TOKEN or a CSS session.
 *
 * NEGATIVE PROOF (#3734): the shipped-anonymous state (no token, no session)
 * MUST be refused. If decideMessageWriteAuth returned true for that, the guard
 * would be hollow — the exact hole this card closes. And an empty configured
 * secret must not turn a missing token into an open door.
 */
import { decideMessageWriteAuth } from '../src/server';

const SECRET = 'bridge-secret-abc';

describe('#3966 message-write auth decision', () => {
  it('admits a caller presenting the correct token', () => {
    expect(decideMessageWriteAuth(SECRET, SECRET, false)).toBe(true);
  });

  it('admits an authenticated session regardless of token', () => {
    expect(decideMessageWriteAuth(undefined, SECRET, true)).toBe(true);
  });

  it('NEGATIVE PROOF: anonymous (no token, no session) is refused — the shipped hole', () => {
    expect(decideMessageWriteAuth(undefined, SECRET, false)).toBe(false);
  });

  it('NEGATIVE PROOF: a wrong token with no session is refused', () => {
    expect(decideMessageWriteAuth('not-the-secret', SECRET, false)).toBe(false);
  });

  it('NEGATIVE PROOF: an empty configured secret never admits on token (missing secret ≠ open door)', () => {
    expect(decideMessageWriteAuth('', '', false)).toBe(false);
    expect(decideMessageWriteAuth(undefined, '', false)).toBe(false);
  });
});
