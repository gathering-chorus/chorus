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
import { DeliveryWorker } from './delivery-worker';
import { QUEUED_DELIVERABLE } from './store';

const TRACE = '01a15e0e-95a3-7a21-8b32-9637c7ad3757';
const dbFile = () => path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'buzz-delivery-4629-')), 'messages.db');

function harness(file: string, now: () => number) {
  const store = new MessageStore(file);
  const delivered: number[] = [];
  const causes: ReleaseCause[] = [];
  const lost: number[] = [];
  const d = new BuzzDelivery({ store, rowFor: (id) => store.getPendingDelivery(id), deliver: (row) => delivered.push(row.id), onLost: (id) => lost.push(id), onRelease: (_id, c) => causes.push(c), now });
  return { store, d, delivered, causes, lost };
}

describe('#4629 S3 a held nudge is delivered exactly once', () => {
  test('the room note releases it; a second note, a publish-failed and the fallback then do nothing', () => {
    let now = Date.now();
    const { store, d, delivered, causes } = harness(dbFile(), () => now);
    const id = store.sendNudge('wren', 'silas', 'hi', TRACE, 'r2r', 'none', undefined, true);
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
    const id = store.sendNudge('wren', 'silas', 'hi', TRACE, 'r2r', 'none', undefined, true);
    expect(d.publishFailed(TRACE)).toBe(true);
    expect([delivered, causes]).toEqual([[id], ['publish-failed']]);
  });

  test('NEGATIVE PROOF: with no note and no release, it is NOT delivered inside the hold window, and IS after it', () => {
    let now = Date.now() + 5_000;
    const { store, d, delivered } = harness(dbFile(), () => now);
    const id = store.sendNudge('wren', 'silas', 'hi', TRACE, 'r2r', 'none', undefined, true);
    expect(d.sweep()).toBe(0);
    now += 60_000;
    expect(d.sweep()).toBe(1);
    expect(delivered).toEqual([id]);
  });

  test('NEGATIVE PROOF: no queued reader touches a held row — drain, agent-queued, inbox claim, worker scan', () => {
    const { store } = harness(dbFile(), Date.now);
    const id = store.sendNudge('wren', 'silas', 'hi', TRACE, 'r2r', 'none', undefined, true);
    expect(store.drainQueued('silas')).toBe(0);
    expect(store.getAgentQueued().map((r) => r.id)).not.toContain(id);
    expect(store.claimAgentInbox('silas', 'run-1').map((r) => r.id)).not.toContain(id);
    expect(store.getPendingDeliveries().map((r) => r.id)).not.toContain(id);      // scanAndRequeue's read
    expect(store.getDeliveryRecord(id).delivery_status).toBe('queued');
  });

  test('AC3: the note released and delivered it, pulse restarts, the reader replays the same note: still one delivery across both', () => {
    let now = Date.now() + 5_000;
    const file = dbFile();
    const before = harness(file, () => now);
    const id = before.store.sendNudge('wren', 'silas', 'replayed after restart', TRACE, 'r2r', 'none', undefined, true);
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
    const id = before.store.sendNudge('wren', 'silas', 'restart mid-send', TRACE, 'r2r', 'none', undefined, true);
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

describe('#4629 S3 review fixes (abby, kade 07:26)', () => {
  test('NEGATIVE PROOF: a held nudge is inserted held in one statement, so a worker scan right after the insert delivers nothing', () => {
    const store = new MessageStore(dbFile());
    const id = store.sendNudge('wren', 'silas', 'held at insert', TRACE, 'r2r', 'none', undefined, true);
    expect(store.getPendingDeliveries().map((r) => r.id)).not.toContain(id);   // what scanAndRequeue reads
    expect(store.getDeliveryRecord(id).delivery_status).toBe('queued');
  });

  test('NEGATIVE PROOF: a released row that is not deliverable is reported lost, never dropped silently', () => {
    const lost: number[] = [];
    const delivered: number[] = [];
    const store = { releaseBuzzHeld: () => true, buzzHeldBefore: () => [], getMessageEndsByTrace: () => ({ id: 7 }) };
    const d = new BuzzDelivery({ store, rowFor: () => null, deliver: (r) => delivered.push(r.id), onLost: (id) => lost.push(id), onRelease: () => undefined });
    expect(d.publishFailed(TRACE)).toBe(true);
    expect([delivered, lost]).toEqual([[], [7]]);
  });

  test('a store that already holds a duplicate trace starts without the unique index instead of crashing', () => {
    const file = dbFile();
    const first = new MessageStore(file);
    (first as unknown as { db: { exec(sql: string): void } }).db.exec('DROP INDEX IF EXISTS idx_messages_nudge_trace_unique');
    first.sendNudge('wren', 'silas', 'a', TRACE);
    first.sendNudge('wren', 'kade', 'b', TRACE);
    first.close();
    const err = jest.spyOn(process.stderr, 'write').mockImplementation(() => true);
    expect(() => new MessageStore(file)).not.toThrow();
    expect(err.mock.calls.map((c) => String(c[0])).join('')).toContain('store.trace_index.skipped');
    err.mockRestore();
  });
});

describe('#4629 every queued read in the store goes through QUEUED_DELIVERABLE (Silas 07:45)', () => {
  /** WHERE/AND clauses that read queued rows directly, outside the one rule and the held-row paths. */
  const rawQueuedReads = (src: string): string[] =>
    src.split('\n').filter((line) => /(WHERE|AND|OR|\()\s*\(?\s*delivery_status\s*=\s*\\?'queued\\?'/.test(line)
      && !/buzz_hold|QUEUED_DELIVERABLE|WHERE id\s*=\s*\?/.test(line));

  test('store.ts reads queued rows only through the rule', () => {
    const src = fs.readFileSync(path.join(__dirname, 'store.ts'), 'utf8');
    expect(rawQueuedReads(src)).toEqual([]);
    expect(QUEUED_DELIVERABLE).toContain('buzz_hold=0');
  });

  test('NEGATIVE PROOF: a planted raw queued read is caught', () => {
    const planted = "      'UPDATE messages SET delivery_status = \\'pending\\' WHERE delivery_status = \\'queued\\' AND \"to\" = ?'";
    expect(rawQueuedReads(planted)).toHaveLength(1);
  });
});

describe('#4629 a released row enqueued twice (release + a worker scan) is delivered once', () => {
  test('per-recipient chains are serial and a settled row is skipped', async () => {
    const store = new MessageStore(dbFile());
    const injected: number[] = [];
    const worker = new DeliveryWorker(store, (_to, _c, _f, mid) => { injected.push(Number(String(mid).replace('pulse:', ''))); return Promise.resolve({ rc: 0, stderr: '' }); }, () => Promise.resolve(), [0], () => Promise.resolve());
    const id = store.sendNudge('wren', 'silas', 'released once', TRACE, 'r2r', 'none', undefined, true);
    expect(store.releaseBuzzHeld(id)).toBe(true);
    const row = store.getPendingDelivery(id)!;
    await Promise.all([worker.enqueue(row), worker.scanAndRequeue()]);
    await worker.enqueue(row);
    expect(injected).toEqual([id]);
  });
});

describe('#4629 the buzz_hold column is added without rewriting a row (Silas 07:44)', () => {
  test('an existing store gains the column and keeps every row and status', () => {
    const file = dbFile();
    const old = new MessageStore(file);
    const a = old.sendNudge('kade', 'silas', 'delivered one');
    old.markDelivered(a);
    const b = old.sendNudge('kade', 'wren', 'queued one');
    old.markQueued(b, 'busy');
    old.sendNudge('kade', 'jeff', 'pending one');
    const raw = (old as unknown as { db: { exec(sql: string): void; prepare(sql: string): { all(): unknown[] } } }).db;
    const counts = () => JSON.stringify(raw.prepare('SELECT delivery_status, count(*) n FROM messages GROUP BY 1 ORDER BY 1').all());
    const before = counts();
    raw.exec('ALTER TABLE messages DROP COLUMN buzz_hold');                 // a store from before #4629
    old.close();
    const again = new MessageStore(file);
    const raw2 = (again as unknown as { db: { prepare(sql: string): { all(): unknown[] } } }).db;
    expect(JSON.stringify(raw2.prepare('SELECT delivery_status, count(*) n FROM messages GROUP BY 1 ORDER BY 1').all())).toBe(before);
    expect((raw2.prepare('SELECT count(*) n FROM messages WHERE buzz_hold = 0').all()[0] as { n: number }).n).toBe(3);
  });
});
