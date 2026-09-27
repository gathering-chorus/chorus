// @test-type: unit — router, journal and tailer over temp dirs; no live Clearing, no live transcripts
/**
 * #4363 — one message id, full history.
 *
 * What Jeff sees: every message once, in order, including replies written
 * while the Clearing was restarting; and his own words once, not twice (once
 * from the room, again from the terminal transcript).
 */
import fs from 'fs';
import os from 'os';
import path from 'path';

const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'one-id-4363-'));
process.env.CLEARING_PROJECTS_DIR = path.join(TMP, 'projects');
process.env.CLEARING_TAILER_OFFSETS = path.join(TMP, 'offsets.json');
process.env.CLEARING_REPLY_QUIET_MS = '5';
const { MessageRouter } = require('../src/router');
const { SessionTailer } = require('../src/session-tailer');
const { RoomJournal } = require('../src/room-journal');

afterAll(() => fs.rmSync(TMP, { recursive: true, force: true }));

const TS = '2026-09-27T13:00:00Z';

describe('#4363 one id per message', () => {
  // Negative proof: the old router dropped this as a duplicate (same sender,
  // same text, within the last 10). Two real messages must both show.
  test('the same words sent twice are two messages', () => {
    const r = new MessageRouter();
    r.ingest({ id: 'a', from: 'jeff', text: 'go', ts: TS, type: 'jeff-input' });
    r.ingest({ id: 'b', from: 'jeff', text: 'go', ts: TS, type: 'jeff-input' });
    expect(r.getRecent(10)).toHaveLength(2);
  });

  test('the same message arriving twice (same id) shows once', () => {
    const r = new MessageRouter();
    r.ingest({ id: 'x', from: 'wren', text: 'done', ts: TS, type: 'role-response' });
    r.ingest({ id: 'x', from: 'wren', text: 'done', ts: TS, type: 'role-response' });
    expect(r.getRecent(10)).toHaveLength(1);
  });

  test('a message with no id gets one from its content, so a replay still shows once', () => {
    const r = new MessageRouter();
    r.ingest({ from: 'silas', text: 'hi', ts: TS, type: 'role-to-role' });
    r.ingest({ from: 'silas', text: 'hi', ts: TS, type: 'role-to-role' });
    const got = r.getRecent(10, true);
    expect(got).toHaveLength(1);
    expect(got[0].id).toMatch(/^h:/);
  });
});

describe('#4363 Jeff\'s input is stored once', () => {
  test('the transcript echo of a room message is consumed, once', () => {
    const r = new MessageRouter();
    r.expectEcho('kade', '@kade deploy the "fix"');
    expect(r.consumeEcho('kade', 'deploy the \\"fix\\"')).toBe(true);
    expect(r.consumeEcho('kade', 'deploy the \\"fix\\"')).toBe(false);
  });

  test('an echo for another role, or text Jeff typed in the pane, is not consumed', () => {
    const r = new MessageRouter();
    r.expectEcho('kade', 'deploy');
    expect(r.consumeEcho('wren', 'deploy')).toBe(false);
    expect(r.consumeEcho('kade', 'something he typed in the terminal')).toBe(false);
  });
});

describe('#4363 full history through the journal', () => {
  test('pages walk back from the newest, by id', () => {
    const j = new RoomJournal(path.join(TMP, 'room.jsonl'));
    for (let i = 1; i <= 5; i++) j.append({ id: `m${i}`, from: 'jeff', text: `t${i}`, ts: TS, type: 'jeff-input', visible: true });
    expect(j.page(undefined, 2).map((m: { id: string }) => m.id)).toEqual(['m4', 'm5']);
    expect(j.page('m4', 2).map((m: { id: string }) => m.id)).toEqual(['m2', 'm3']);
    expect(j.page('m2', 5).map((m: { id: string }) => m.id)).toEqual(['m1']);
    expect(j.total()).toBe(5);
  });

  test('a journal survives a restart: a new reader sees what the old one wrote', () => {
    const f = path.join(TMP, 'room2.jsonl');
    new RoomJournal(f).append({ id: 'k1', from: 'wren', text: 'x', ts: TS, type: 'role-response', visible: true });
    expect(new RoomJournal(f).page(undefined, 10).map((m: { id: string }) => m.id)).toEqual(['k1']);
  });
});

describe('#4363 a reply written while the Clearing was down is shown when it comes back', () => {
  const dir = path.join(process.env.CLEARING_PROJECTS_DIR as string, '-Users-jeffbridwell-CascadeProjects-chorus-roles-wren');
  const file = path.join(dir, 'sess.jsonl');
  const line = (o: object) => JSON.stringify(o) + '\n';
  const reply = (uuid: string, text: string) => line({ type: 'assistant', uuid, timestamp: TS, message: { content: [{ type: 'text', text }], stop_reason: 'end_turn' } });
  const wait = (ms: number) => new Promise((res) => setTimeout(res, ms));

  test('the second tailer resumes where the first stopped, and shows each reply once', async () => {
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(file, line({ type: 'user', uuid: 'u1', timestamp: TS, message: { content: 'first question' } }));

    const r1 = new MessageRouter();
    const t1 = new SessionTailer(r1, () => undefined);
    t1.start();
    fs.appendFileSync(file, reply('a1', 'first answer'));
    t1.checkNow('wren');
    await wait(50);
    t1.stop();
    // the first run really read the transcript, so its absence below means something
    expect(r1.getRecent(50, true).map((m: { text: string }) => m.text)).toContain('first answer');

    // Clearing is down: a reply lands in the transcript
    fs.appendFileSync(file, line({ type: 'user', uuid: 'u2', timestamp: TS, message: { content: 'second question' } }));
    fs.appendFileSync(file, reply('a2', 'answer written while down'));

    const r2 = new MessageRouter();
    const t2 = new SessionTailer(r2, () => undefined);
    t2.start();
    t2.checkNow('wren');
    await wait(50);
    t2.stop();
    const texts = r2.getRecent(50, true).map((m: { text: string }) => m.text);
    expect(texts).toContain('answer written while down');
    expect(texts).not.toContain('first answer');
  });
});

describe('#4363 the room mints Jeff\'s message id and expects its echo', () => {
  const { processJeffInput } = require('../src/jeff-input');
  test('one id at creation, and the transcript copy in each target pane is not stored again', async () => {
    const r = new MessageRouter();
    await processJeffInput({
      ingest: (m: object) => r.ingest(m),
      newId: () => 'room-1',
      expectEcho: (t: string, text: string) => r.expectEcho(t, text),
      deliver: async () => null,
      targetsOf: () => ['wren', 'kade'],
      now: () => TS,
    }, { text: '@wren @kade ship it', from: 'jeff' });
    expect(r.getRecent(10).map((m: { id: string }) => m.id)).toEqual(['room-1']);
    // what the tailer does when each pane's transcript shows the typed copy
    expect(r.consumeEcho('wren', 'ship it')).toBe(true);
    expect(r.consumeEcho('kade', 'ship it')).toBe(true);
  });

  test('NEGATIVE PROOF: without expectEcho the transcript copy is not consumed, so it would show twice', async () => {
    const r = new MessageRouter();
    await processJeffInput({ ingest: (m: object) => r.ingest(m), deliver: async () => null, targetsOf: () => ['wren'], now: () => TS },
      { text: 'ship it', from: 'jeff' });
    expect(r.consumeEcho('wren', 'ship it')).toBe(false);
  });
});
