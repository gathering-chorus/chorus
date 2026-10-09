// @test-type: unit — a stub socket, derived test keys and the in-process role list; no relay, no roles API.
// @domain: messages
// @card: #4445
// @owner: wren
/**
 * #4445 — Silas, 2026-10-07 20:20, demo :3481: the read-only room replayed 207
 * notes at startup and every one logged unknown-key. The room dialed at import,
 * before the tile poller's first read of the roles API, so the role list was
 * empty when the replay arrived and no sender could be named. The room now dials
 * only after that first read.
 */
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { startRoom } from '../src/buzz-room-wiring';
import { buildRoomIdentity } from '../src/buzz-room';
import { setRoomRoles } from '../src/room-roles';
import { derivedSigner } from '../src/buzz-signer';
import type { NostrEvent } from '../src/buzz-bridge';

const SECRET = 'test-secret-4445-ready';

function abbyReply(): NostrEvent {
  const signer = derivedSigner('abby-normal', SECRET);
  const tags = [['t', 'reply'], ['role', 'abby-normal'], ['seq', '0']];
  const created_at = 1791400000;
  const { id, sig } = signer.signEvent(JSON.stringify([0, signer.pubkey, created_at, 1, tags, 'Abby here']));
  return { id, pubkey: signer.pubkey, created_at, kind: 1, tags, content: 'Abby here', sig };
}

// One room whose stub relay replays Abby's reply the moment it is dialed.
function world(rolesReady?: Promise<unknown>) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'room-ready-4445-'));
  const ingested: string[] = [];
  const dropped: string[] = [];
  let dials = 0;
  const connect = () => {
    dials += 1;
    const handlers: Record<string, (x: unknown) => void> = {};
    setImmediate(() => handlers.message?.(JSON.stringify(['EVENT', 'room', abbyReply()])));
    return {
      readyState: 1,
      send: () => { /* noop */ },
      on: (ev: string, fn: (x: unknown) => void) => { handlers[ev] = fn; },
      close: () => { /* noop */ },
    } as never;
  };
  const identity = buildRoomIdentity(undefined, {
    pubkeyFor: (actor) => derivedSigner(actor, SECRET).pubkey,
    serviceSigner: () => derivedSigner('bridge', SECRET),
  });
  const room = startRoom({
    relayUrl: 'ws://relay.invalid:3000',
    topic: 'team',
    identity,
    readOnly: true,
    cursorFile: path.join(dir, 'room-cursor-team.json'),
    ingest: (m) => { ingested.push(`${m.from}: ${m.text}`); },
    log: (_level, event, fields) => { if (event === 'buzz.room.not_rendered') dropped.push(String(fields?.disposition)); },
    connect,
    rolesReady,
  });
  return { room, ingested, dropped, dials: () => dials, cleanup: () => fs.rmSync(dir, { recursive: true, force: true }) };
}

const tick = () => new Promise((res) => setImmediate(res));

describe('#4445 the room dials after the role list is read', () => {
  beforeAll(() => { process.env.BUZZ_ROOM_SECRET = SECRET; });
  beforeEach(() => setRoomRoles([]));

  test('the room waits for the roles read, then names Abby in the replay', async () => {
    let rolesRead!: () => void;
    const ready = new Promise<void>((res) => { rolesRead = res; });
    const w = world(ready);
    await tick();
    expect(w.dials()).toBe(0);
    setRoomRoles(['wren', 'silas', 'kade', 'abby-normal']);
    rolesRead();
    await tick(); await tick();
    w.room.stop();
    expect(w.dials()).toBe(1);
    expect(w.ingested).toEqual(['abby-normal: Abby here']);
    expect(w.dropped).toEqual([]);
    w.cleanup();
  });

  test('a failed roles read still dials', async () => {
    const w = world(Promise.reject(new Error('roles API down')));
    await tick(); await tick();
    w.room.stop();
    expect(w.dials()).toBe(1);
    w.cleanup();
  });

  test('NEGATIVE PROOF: dialing before the roles read drops Abby\'s replay as unknown-key', async () => {
    const w = world();
    await tick();
    setRoomRoles(['wren', 'silas', 'kade', 'abby-normal']);
    await tick();
    w.room.stop();
    expect(w.ingested).toEqual([]);
    expect(w.dropped).toEqual(['unknown-key']);
    w.cleanup();
  });
});
