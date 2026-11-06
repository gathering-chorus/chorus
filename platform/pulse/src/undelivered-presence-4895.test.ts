// @test-type: unit — pulse worker on a temp db; inject faked
// @domain: messages
// @card: #4895
// @owner: silas
// #4895 AC1 — a delivery that fails for good on a dead pane asks the principal (the
// Presence's only writer) to mark the role unreachable, once, naming the pane. A delivery
// that lands, or a failure that is not a dead pane, asks nothing.
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { MessageStore } from './store';
import { DeliveryWorker } from './delivery-worker';

const tmpDb = () => path.join(fs.mkdtempSync(path.join(os.tmpdir(), 'undelivered-4895-')), 'messages.db');
const noSleep = () => Promise.resolve();

describe('#4895 a dead pane makes the presence unreachable at once', () => {
  it('a pane still dead after the sweep waits asks the principal once, with the role and pane', async () => {
    const store = new MessageStore(tmpDb());
    const w = new DeliveryWorker(store, () => Promise.resolve({ rc: 1, stderr: "0:638: execution error: can't find pane: %16 (1)" }), () => Promise.resolve(), [1, 1], noSleep);
    const asked: Array<[string, string]> = [];
    w.onUndelivered = (role, pane) => { asked.push([role, pane]); };
    const id = store.sendNudge('wren', 'abby-normal', 'are you there');
    await w.enqueue({ id, from: 'wren', to: 'abby-normal', content: 'are you there', delivery_attempts: 0 });
    expect(asked).toEqual([['abby-normal', '%16']]);
    store.close();
  });

  it('NEGATIVE PROOF: a delivery that lands asks nothing', async () => {
    const store = new MessageStore(tmpDb());
    const w = new DeliveryWorker(store, () => Promise.resolve({ rc: 0, stderr: '' }), () => Promise.resolve(), [1, 1], noSleep);
    const asked: Array<[string, string]> = [];
    w.onUndelivered = (role, pane) => { asked.push([role, pane]); };
    const id = store.sendNudge('wren', 'abby-normal', 'are you there');
    await w.enqueue({ id, from: 'wren', to: 'abby-normal', content: 'are you there', delivery_attempts: 0 });
    expect(asked).toEqual([]);
    store.close();
  });

  it('NEGATIVE PROOF: a failure that is not a dead pane (retried, then failed) asks nothing', async () => {
    const store = new MessageStore(tmpDb());
    const w = new DeliveryWorker(store, () => Promise.resolve({ rc: 1, stderr: 'osascript timed out' }), () => Promise.resolve(), [1, 1], noSleep);
    const asked: Array<[string, string]> = [];
    w.onUndelivered = (role, pane) => { asked.push([role, pane]); };
    const id = store.sendNudge('wren', 'abby-normal', 'are you there');
    await w.enqueue({ id, from: 'wren', to: 'abby-normal', content: 'are you there', delivery_attempts: 0 });
    expect(asked).toEqual([]);
    store.close();
  });
});
