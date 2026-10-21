// @test-type: unit — a temp message store and a fake clock.
// @domain: messages
// @card: #4629
// @owner: wren
/**
 * #4629 S3 + AC3 — pulse delivers a sender-signed nudge from the room, exactly
 * once, whichever of room note / publish-failed / fallback comes first, and a
 * pulse restart mid-hold still delivers it once (the store holds the state).
 */
import fs from 'fs';
import os from 'os';
import path from 'path';
import { MessageStore } from './store';
import { BuzzDelivery, type ReleaseCause } from './buzz-delivery';

const TRACE = '01a15e0e-95a3-7a21-8b32-9637c7ad3757';
const dbFile = () => path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'buzz-delivery-4629-')), 'messages.db');

function harness(file: string, now: () => number) {
  const store = new MessageStore(file);
  const delivered: number[] = [];
  const causes: ReleaseCause[] = [];
  const d = new BuzzDelivery({ store, deliver: (id) => delivered.push(id), onRelease: (_id, c) => causes.push(c), now });
  return { store, d, delivered, causes };
}

describe('#4629 S3 a held nudge is delivered exactly once', () => {
  test('the room note releases it; a second note, a publish-failed and the fallback then do nothing', () => {
    let now = Date.now();
    const { store, d, delivered, causes } = harness(dbFile(), () => now);
    const id = store.sendNudge('wren', 'silas', 'hi', TRACE);
    d.hold(id);
    expect(store.getDeliveryRecord(id).delivery_status).toBe('queued');
    expect(d.fromRoom(id)).toBe(true);
    expect(d.fromRoom(id)).toBe(false);
    expect(d.publishFailed(TRACE)).toBe(false);
    now += 120_000;
    expect(d.sweep()).toBe(0);
    expect(delivered).toEqual([id]);
    expect(causes).toEqual(['room']);
    expect(store.getDeliveryRecord(id).delivery_status).toBe('pending');
  });

  test('a failed publish releases it at once, by trace', () => {
    const { store, d, delivered, causes } = harness(dbFile(), Date.now);
    const id = store.sendNudge('wren', 'silas', 'hi', TRACE);
    d.hold(id);
    expect(d.publishFailed(TRACE)).toBe(true);
    expect([delivered, causes]).toEqual([[id], ['publish-failed']]);
  });

  test('NEGATIVE PROOF: with no note and no release, it is NOT delivered inside the hold window, and IS after it', () => {
    let now = Date.now() + 5_000;
    const { store, d, delivered } = harness(dbFile(), () => now);
    const id = store.sendNudge('wren', 'silas', 'hi', TRACE);
    d.hold(id);
    expect(d.sweep()).toBe(0);
    now += 60_000;
    expect(d.sweep()).toBe(1);
    expect(delivered).toEqual([id]);
  });

  test('NEGATIVE PROOF: the tmux drain never releases a held row', () => {
    const { store, d } = harness(dbFile(), Date.now);
    const id = store.sendNudge('wren', 'silas', 'hi', TRACE);
    d.hold(id);
    expect(store.drainQueued('silas')).toBe(0);
    expect(store.getDeliveryRecord(id).delivery_status).toBe('queued');
  });

  test('AC3: the note released and delivered it, pulse restarts, the reader replays the same note: still one delivery across both', () => {
    let now = Date.now() + 5_000;
    const file = dbFile();
    const before = harness(file, () => now);
    const id = before.store.sendNudge('wren', 'silas', 'replayed after restart', TRACE);
    before.d.hold(id);
    expect(before.d.fromRoom(id)).toBe(true);      // the note releases it
    before.store.markDelivered(id);               // the worker delivered it
    before.store.close();                         // pulse restarts
    now += 60_000;
    const after = harness(file, () => now);
    expect(after.d.sweep()).toBe(0);              // boot sweep: nothing held
    expect(after.d.fromRoom(id)).toBe(false);     // the replayed note (watermark − 60 s) finds nothing to release
    expect([...before.delivered, ...after.delivered]).toEqual([id]);
  });

  test('AC3: pulse restarted mid-hold delivers the message once, at the boot sweep', () => {
    let now = Date.now() + 5_000;
    const file = dbFile();
    const before = harness(file, () => now);
    const id = before.store.sendNudge('wren', 'silas', 'restart mid-send', TRACE);
    before.d.hold(id);
    before.store.close();                       // pulse stops with the row held
    now += 60_000;
    const after = harness(file, () => now);     // pulse starts again on the same store
    expect(after.d.sweep()).toBe(1);            // the boot sweep
    expect(after.d.fromRoom(id)).toBe(false);   // the late room note finds nothing to release
    expect(after.d.sweep()).toBe(0);
    expect(after.delivered).toEqual([id]);
    expect(after.causes).toEqual(['fallback']);
  });
});
