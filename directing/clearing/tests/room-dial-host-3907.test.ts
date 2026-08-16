// @test-type: unit — captures the dial arguments; no socket, no relay.
/**
 * #3907 — the subscriber has to announce the right Host.
 *
 * The relay virtual-hosts by Host header. Reaching it through the loopback
 * tunnel means dialing 127.0.0.1, so the relay sees a Host it does not serve
 * and answers 404. The socket opens; the subscription never does; the room
 * looks merely quiet. Kade found this against the live relay.
 *
 * NEGATIVE PROOF (#3734): `dials with no Host when none is configured` is the
 * state the fix exists to leave behind, and `passes the configured Host` is the
 * state it creates. Both assert on the ARGUMENTS handed to the socket
 * constructor, because that is the only place the bug ever existed — every
 * existing room test injects a stub connect() and so could never see it. A
 * suite that green-lit this file for weeks is the reason this test asserts on
 * the dial rather than on behaviour downstream of it.
 */
import { startRoom } from '../src/buzz-room-wiring';

// The signer refuses to derive keys from a default secret (#3618), so the test
// brings its own — a throwaway value that exists only in this process.
process.env.BUZZ_ROOM_SECRET = process.env.BUZZ_ROOM_SECRET ?? 'test-only-secret-3907';

interface Dial { url: string; opts: unknown }

function fakeSocket() {
  return {
    on() { /* no events in this test */ },
    send() { /* never called */ },
    close() { /* no-op */ },
  } as never;
}

function dialsFrom(relayHost?: string, env: NodeJS.ProcessEnv = {}): Dial[] {
  const dials: Dial[] = [];
  const prev = process.env.BUZZ_RELAY_HOST_HEADER;
  if (env.BUZZ_RELAY_HOST_HEADER === undefined) delete process.env.BUZZ_RELAY_HOST_HEADER;
  else process.env.BUZZ_RELAY_HOST_HEADER = env.BUZZ_RELAY_HOST_HEADER;

  const room = startRoom({
    relayUrl: 'ws://127.0.0.1:13000/',
    topic: 'team',
    ingest: () => { /* nothing arrives in this test */ },
    log: () => { /* quiet */ },
    cursorFile: '/tmp/does-not-matter-cursor.json',
    relayHost,
    connect: ((url: string, opts: unknown) => { dials.push({ url, opts }); return fakeSocket(); }) as never,
  });
  room.stop();

  if (prev === undefined) delete process.env.BUZZ_RELAY_HOST_HEADER;
  else process.env.BUZZ_RELAY_HOST_HEADER = prev;
  return dials;
}

describe('#3907 dial — the relay is told which host we mean', () => {
  it('passes the configured Host so a tunnelled dial is not answered 404', () => {
    const [dial] = dialsFrom('192.168.86.242:3000');
    expect(dial.url).toBe('ws://127.0.0.1:13000/');
    expect(dial.opts).toEqual({ headers: { Host: '192.168.86.242:3000' } });
  });

  it('reads the Host from the environment when the caller does not pass one', () => {
    const [dial] = dialsFrom(undefined, { BUZZ_RELAY_HOST_HEADER: 'bedroom:3000' });
    expect(dial.opts).toEqual({ headers: { Host: 'bedroom:3000' } });
  });

  it('NEGATIVE: with nothing configured the dial carries no Host — the 404 state', () => {
    // This is what shipped: the relay sees Host 127.0.0.1, serves nothing, and
    // the room is silent with no error anywhere. Keeping it asserted means the
    // fallback cannot quietly become the default again.
    const [dial] = dialsFrom(undefined, {});
    expect(dial.opts).toEqual({});
  });

  it('an explicit Host wins over the environment', () => {
    const [dial] = dialsFrom('explicit:3000', { BUZZ_RELAY_HOST_HEADER: 'env:3000' });
    expect(dial.opts).toEqual({ headers: { Host: 'explicit:3000' } });
  });
});
