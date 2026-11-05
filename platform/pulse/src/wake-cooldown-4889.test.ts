// @test-type: unit — pulse store, admission and worker on a temp db; no live services
// @domain: messages
// @card: #4889
// @owner: silas
// #4889 (folded #4891) — the run-finish wake mark travels as a field: src/service wakeOf reads
// it from the nudge body, src/store keeps it on the row (a migrated `wake` column, so a requeue
// keeps it), and src/delivery-worker hands it to the boundary. The machine-lane cooldown is the
// property pulse.announce.machine_cooldown_ms, read by readCooldown with a TTL, said on the
// spine once per change.
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import Database from 'better-sqlite3';
import { MessageStore } from './store';
import { wakeOf } from './service';
import { DeliveryWorker } from './delivery-worker';
import type { GlobalRead } from '../../chorus-sdk/properties/global';

const tmpDb = () => path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'wake-4889-')), 'messages.db');
const inject = async () => ({ rc: 0, stderr: '' });
type Ev = Array<[string, Record<string, unknown>]>;
const into = (events: Ev) => async (e: string, f: Record<string, unknown>) => { events.push([e, f]); };
const ok = (value: string): GlobalRead => ({ ok: true, value });
const row = (error: string): GlobalRead => ({ ok: false, where: 'row', error });
const door = (error: string): GlobalRead => ({ ok: false, where: 'door', error });

describe('#4889 the wake mark is a field', () => {
  it('wakeOf takes only the marks the boundary knows', () => {
    expect(wakeOf({ wake: 'run-finished' })).toBe('run-finished');
    expect(wakeOf({ wake: 'urgent' })).toBeUndefined();
    expect(wakeOf({})).toBeUndefined();
    expect(wakeOf(undefined)).toBeUndefined();
  });

  it('the store keeps it on the row, so a restart requeue still knows the wake', () => {
    const s = new MessageStore(tmpDb());
    s.sendNudge('silas', 'silas', 'a', 't-1', 'a2r', 'none', undefined, false, 'library/alert', 'run-finished');
    s.sendNudge('silas', 'silas', 'b', 't-2', 'a2r', 'none', undefined, false, 'library/alert');
    expect(s.getPendingDeliveries().map((r) => r.wake)).toEqual(['run-finished', null]);
    s.close();
  });

  it('an old store without the column is migrated in place; its rows read no wake', () => {
    const p = tmpDb();
    new MessageStore(p).close();
    const raw = new Database(p);
    raw.exec('ALTER TABLE messages DROP COLUMN wake');
    raw.prepare("INSERT INTO messages (type, \"from\", \"to\", content, delivery_status) VALUES ('nudge','system','kade','old','pending')").run();
    raw.close();
    const s = new MessageStore(p);
    expect(s.getPendingDeliveries().map((r) => [r.content, r.wake])).toEqual([['old', null]]);
    s.close();
  });
});

describe('#4889 the cooldown is the property, read with a TTL', () => {
  it('a read row sets the cooldown and says so once', async () => {
    const events: Ev = [];
    const w = new DeliveryWorker(new MessageStore(tmpDb()), inject, into(events));
    await w.readCooldown(async () => ok('1800000'));
    await w.readCooldown(async () => ok('1800000'));
    expect(w.cooldownMs()).toBe(1800000);
    expect(events.filter(([e]) => e === 'announce.cooldown.read')).toHaveLength(1);
  });

  it('NEGATIVE PROOF: no row means no cooldown (dedupe off), said once, never a default', async () => {
    const events: Ev = [];
    const w = new DeliveryWorker(new MessageStore(tmpDb()), inject, into(events));
    await w.readCooldown(async () => row('no property row sets pulse.announce.machine_cooldown_ms'));
    await w.readCooldown(async () => row('no property row sets pulse.announce.machine_cooldown_ms'));
    expect(w.cooldownMs()).toBeUndefined();
    const unread = events.filter(([e]) => e === 'announce.cooldown.unread');
    expect(unread).toHaveLength(1);
    expect(unread[0][1]).toMatchObject({ dedupe: 'off' });
  });

  it('NEGATIVE PROOF: a value that is not a positive whole number is unread', async () => {
    const w = new DeliveryWorker(new MessageStore(tmpDb()), inject, into([]));
    for (const v of ['0', '-5', '30m', '1.5']) {
      await w.readCooldown(async () => ok(v));
      expect(w.cooldownMs()).toBeUndefined();
    }
  });

  it('a door that does not answer keeps the last value read, and says so once', async () => {
    const events: Ev = [];
    const w = new DeliveryWorker(new MessageStore(tmpDb()), inject, into(events));
    await w.readCooldown(async () => ok('600000'));
    await w.readCooldown(async () => door('the properties door did not answer'));
    await w.readCooldown(async () => door('the properties door did not answer'));
    expect(w.cooldownMs()).toBe(600000);
    const unread = events.filter(([e]) => e === 'announce.cooldown.unread');
    expect(unread).toHaveLength(1);
    expect(unread[0][1]).toMatchObject({ dedupe: 'last value 600000ms' });
  });

  it('a recovery is said once', async () => {
    const events: Ev = [];
    const w = new DeliveryWorker(new MessageStore(tmpDb()), inject, into(events));
    await w.readCooldown(async () => row('missing'));
    await w.readCooldown(async () => ok('600000'));
    expect(events.map(([e]) => e)).toEqual(['announce.cooldown.unread', 'announce.cooldown.read']);
  });
});
