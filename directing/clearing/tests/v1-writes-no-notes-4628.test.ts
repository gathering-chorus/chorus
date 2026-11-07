// @test-type: unit — v1's real tailers and room wiring, a stub socket that counts notes; no relay.
// @domain: messages
// @card: #4628
// @owner: silas
/**
 * #4628 AC2 — Clearing v1 writes nothing to Buzz. Every kind has one writer
 * now (Clearing input: v2; terminal input, nudges, alerts, land notices:
 * pulse; replies: each role's key), and on 10-19 every one of the day's 504
 * doubles had v1 as one of its two writers. v1 still SHOWS the old room; it
 * just never publishes. The same path server.ts runs: tailer → router →
 * publishesToRoom → room.publish.
 */
import os from 'os';
import path from 'path';
import { MessageRouter, publishesToRoom, type ChannelMessage } from '../src/router';
import { ChorusLogTailer } from '../src/tailer';
import { SessionTailer } from '../src/session-tailer';
import { buildRoomIdentity } from '../src/buzz-room';
import { derivedSigner } from '../src/buzz-signer';

const SECRET = 'test-secret-4628';

async function notesV1Writes(feed: (spine: ChorusLogTailer, session: SessionTailer) => void): Promise<number> {
  const { startRoom } = await import('../src/buzz-room-wiring');
  let published = 0;
  const sock = { readyState: 0, send: () => { /* closed: notes queue */ }, on: () => { /* none */ }, close: () => { /* noop */ } };
  const room = startRoom({
    relayUrl: 'ws://counting.invalid:3000',
    topic: 'team',
    ingest: () => { /* nothing arrives */ },
    log: (_l, ev) => { if (ev === 'buzz.room.published') published += 1; },
    connect: () => sock as never,
    cursorFile: path.join(os.tmpdir(), `v1-cursor-4628-${process.pid}`),
    identity: buildRoomIdentity(undefined, {
      pubkeyFor: (actor) => derivedSigner(actor, SECRET).pubkey,
      serviceSigner: () => derivedSigner('bridge', SECRET),
    }),
  });
  const router = new MessageRouter();
  // the server's outbound rule, verbatim (server.ts: `if (room && publishesToRoom(msg)) room.publish(msg)`)
  router.on('message', (msg: ChannelMessage) => { if (publishesToRoom(msg)) room.publish(msg); });
  const spine = new ChorusLogTailer(router);
  const session = new SessionTailer(router);
  feed(spine, session);
  await new Promise((r) => setTimeout(r, 50));
  room.stop();
  return published;
}

const feedThree = (spine: ChorusLogTailer, session: SessionTailer) => {
  const line = (o: object) => (spine as unknown as { processLine(l: string): void }).processLine(JSON.stringify(o));
  // a land, a nudge, and a line Jeff typed in Silas's terminal
  line({ event: 'card.accepted', role: 'silas', card_id: 4630, title: 'Backups', acceptor: 'jeff', timestamp: new Date().toISOString() });
  line({ event: 'nudge.emitted', role: 'kade', payload: 'from=kade,to=silas,chars=4,content=ping' });
  const s = session as unknown as { handleUserMessage(r: string, e: object, ts: string): void; handleAssistantMessage(r: string, e: object, ts: string): void };
  s.handleUserMessage('silas', { uuid: 'u-4628', message: { content: 'ok router is in living room' } }, new Date().toISOString());
  // and Silas's reply, promoted to the room when Jeff types again (v1's reply copy: 413 of 10-19's doubles)
  s.handleAssistantMessage('silas', { uuid: 'a-4628', message: { content: [{ type: 'text', text: 'Got it, the router is in the living room.' }], stop_reason: 'end_turn' } }, new Date().toISOString());
  s.handleUserMessage('silas', { uuid: 'u-4628b', message: { content: '1st floor' } }, new Date().toISOString());
};

describe('#4628 Clearing v1 writes no notes to Buzz', () => {
  test('a land, a nudge, two Jeff terminal lines and a role reply through v1\'s real tailers publish 0 notes', async () => {
    expect(await notesV1Writes(feedThree)).toBe(0);
  });

  test('v1 still shows them in its own room', () => {
    const router = new MessageRouter();
    const seen: ChannelMessage[] = [];
    router.on('message', (m: ChannelMessage) => seen.push(m));
    feedThree(new ChorusLogTailer(router), new SessionTailer(router));
    expect(seen.map((m) => m.text)).toEqual(expect.arrayContaining(['Accepted #4630 — Backups', 'ok router is in living room']));
  });
});
