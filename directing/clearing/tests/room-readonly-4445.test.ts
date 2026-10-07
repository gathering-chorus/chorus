// @test-type: unit — a stub socket and a temp cursor file; no relay, no service.
/**
 * #4445 — a demo variant's Clearing reads the live relay so Jeff can see Abby's
 * replies in the demo, but it must never write to the live room: no notes
 * published, and no replay cursor written over the live Clearing's.
 * Each test runs the same room twice, read-only and not, so the check can tell
 * the two states apart.
 */
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { startRoom } from '../src/buzz-room-wiring';
import { buildRoomIdentity } from '../src/buzz-room';
import { derivedSigner } from '../src/buzz-signer';
import type { NostrEvent } from '../src/buzz-bridge';

const SECRET = 'test-secret-4445-ro';
const identity = buildRoomIdentity(['jeff', 'wren', 'abby-normal'], {
  pubkeyFor: (actor) => derivedSigner(actor, SECRET).pubkey,
  serviceSigner: () => derivedSigner('bridge', SECRET),
});

function abbyReply(): NostrEvent {
  const signer = derivedSigner('abby-normal', SECRET);
  const tags = [['t', 'reply'], ['role', 'abby-normal'], ['seq', '0']];
  const created_at = 1791400000;
  const { id, sig } = signer.signEvent(JSON.stringify([0, signer.pubkey, created_at, 1, tags, 'Abby here']));
  return { id, pubkey: signer.pubkey, created_at, kind: 1, tags, content: 'Abby here', sig };
}

// A world for one room: stub socket that records frames, a temp cursor file.
function world() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'room-ro-4445-'));
  const sent: string[] = [];
  const handlers: Record<string, (x: unknown) => void> = {};
  const ingested: string[] = [];
  const sock = {
    readyState: 1, // OPEN
    send: (frame: string) => { sent.push(frame); },
    on: (ev: string, fn: (x: unknown) => void) => { handlers[ev] = fn; },
    close: () => { /* noop */ },
  };
  return {
    cursorFile: path.join(dir, 'room-cursor-team.json'),
    deps: {
      relayUrl: 'ws://relay.invalid:3000',
      topic: 'team',
      identity,
      ingest: (m: { from: string; text: string }) => { ingested.push(`${m.from}: ${m.text}`); },
      log: () => { /* quiet */ },
      connect: () => sock as never,
    },
    deliver: (ev: NostrEvent) => handlers.message?.(JSON.stringify(['EVENT', 'room', ev])),
    published: () => sent.filter((f) => f.startsWith('["EVENT"')),
    ingested,
    cleanup: () => fs.rmSync(dir, { recursive: true, force: true }),
  };
}

const jeffSays = { from: 'jeff', text: 'hello room', ts: '2026-10-07T21:20:00.000Z', type: 'jeff-input', visible: true } as const;

describe('#4445 read-only room for demo variants', () => {
  beforeAll(() => { process.env.BUZZ_ROOM_SECRET = SECRET; });

  test('read-only: Abby\'s reply still renders, but nothing is published and no cursor is written', async () => {
    const w = world();
    const room = startRoom({ ...w.deps, cursorFile: w.cursorFile, readOnly: true });
    room.publish(jeffSays);
    w.deliver(abbyReply());
    await new Promise((res) => setImmediate(res));
    room.stop();
    expect(w.ingested).toEqual(['abby-normal: Abby here']);
    expect(w.published()).toEqual([]);
    expect(fs.existsSync(w.cursorFile)).toBe(false);
    w.cleanup();
  });

  test('NEGATIVE PROOF: the same room not read-only publishes the note and writes the cursor', async () => {
    const w = world();
    const room = startRoom({ ...w.deps, cursorFile: w.cursorFile });
    room.publish(jeffSays);
    w.deliver(abbyReply());
    await new Promise((res) => setImmediate(res));
    room.stop();
    expect(w.ingested).toEqual(['abby-normal: Abby here']);
    expect(w.published().length).toBe(1);
    expect(fs.existsSync(w.cursorFile)).toBe(true);
    w.cleanup();
  });
});
