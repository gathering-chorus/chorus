// @domain: awareness
// @test-type: unit — MessageStore in store.ts against temp sqlite files under os.tmpdir(); no live services
// @card: #4965
// @owner: kade
/**
 * #4965 — the store refuses a non-prod context that names no store of its own,
 * whichever marker says so; answers who is owed a reply; counts what it holds;
 * and leaves a messages table of a shape it does not know alone.
 */
import fs from 'fs';
import os from 'os';
import path from 'path';
import Database from 'better-sqlite3';
import { MessageStore } from './store';

const KEYS = ['CHORUS_CONTEXT', 'NODE_ENV', 'BATS_TEST_TMPDIR', 'CARGO', 'CI', 'GITHUB_ACTIONS', 'CHORUS_MESSAGES_DB'] as const;
let saved: Record<string, string | undefined>;
let dir: string;

beforeEach(() => {
  saved = Object.fromEntries(KEYS.map((k) => [k, process.env[k]]));
  for (const k of KEYS) delete process.env[k];
  dir = fs.mkdtempSync(path.join(os.tmpdir(), 'store-cov-4965-'));
});
afterEach(() => {
  for (const k of KEYS) { if (saved[k] === undefined) delete process.env[k]; else process.env[k] = saved[k]; }
  fs.rmSync(dir, { recursive: true, force: true });
});

describe('#4965 the store membrane names the context that tried the prod store', () => {
  test('negative: CHORUS_CONTEXT=build with no store path is refused as build', () => {
    process.env.CHORUS_CONTEXT = 'build';
    expect(() => new MessageStore()).toThrow(/MEMBRANE REFUSED \(#3615\): build context/);
  });

  test('negative: no CHORUS_CONTEXT but NODE_ENV=test is refused as test', () => {
    process.env.NODE_ENV = 'test';
    expect(() => new MessageStore()).toThrow(/MEMBRANE REFUSED \(#3615\): test context/);
  });

  test('negative: no CHORUS_CONTEXT, no test marker, CI set is refused as build', () => {
    process.env.CI = '1';
    expect(() => new MessageStore()).toThrow(/MEMBRANE REFUSED \(#3615\): build context/);
  });

  test('an explicit prod context opens the store CHORUS_MESSAGES_DB names', () => {
    process.env.CHORUS_CONTEXT = 'prod';
    process.env.CI = '1'; // explicit prod wins over the ambient build marker
    const file = path.join(dir, 'seam.db');
    process.env.CHORUS_MESSAGES_DB = file;
    const s = new MessageStore();
    s.sendNudge('kade', 'wren', 'opened');
    s.close();
    expect(fs.existsSync(file)).toBe(true);
    const d = new Database(file, { readonly: true });
    expect((d.prepare('SELECT content FROM messages').get() as { content: string }).content).toBe('opened');
    d.close();
  });
});

describe('#4965 the store answers owed replies, chat partners and counts', () => {
  test('only an unacknowledged r2r nudge that expects something is owed', () => {
    const s = new MessageStore(path.join(dir, 'owed.db'));
    try {
      const owed = s.sendNudge('kade', 'silas', 'review?', undefined, 'r2r', 'reply');
      s.sendNudge('kade', 'silas', 'fyi', undefined, 'r2r', 'none');
      s.sendNudge('system', 'silas', 'alert', undefined, 'a2r', 'reply');
      const rows = s.getOwedNudges();
      expect(rows.map((r) => [r.id, r.from, r.to, r.content, r.expects])).toEqual([[owed, 'kade', 'silas', 'review?', 'reply']]);
    } finally { s.close(); }
  });

  test('negative: a store whose messages table lacks the envelope columns owes nothing, rather than throwing', () => {
    const s = new MessageStore(path.join(dir, 'broken.db'));
    try {
      s.sendNudge('kade', 'silas', 'review?', undefined, 'r2r', 'reply');
      expect(s.getOwedNudges()).toHaveLength(1);
      (s as unknown as { db: Database.Database }).db.exec('ALTER TABLE messages DROP COLUMN nudge_expects');
      expect(s.getOwedNudges()).toEqual([]);
    } finally { s.close(); }
  });

  test('negative: a chat message to a chat that does not exist is addressed to "unknown"', () => {
    const s = new MessageStore(path.join(dir, 'chat.db'));
    try {
      const id = s.startChat('kade', 'wren', 'pair');
      s.chatMessage(id, 'kade', 'to wren');
      s.chatMessage('no-such-chat', 'kade', 'to nobody');
      const byContent = Object.fromEntries(s.queryMessages({ type: 'chat' }).map((m) => [m.content, m.to]));
      expect(byContent).toEqual({ 'to wren': 'wren', 'to nobody': 'unknown' });
    } finally { s.close(); }
  });

  test('stats count every row, the unacknowledged ones, and each type', () => {
    const s = new MessageStore(path.join(dir, 'stats.db'));
    try {
      s.sendNudge('kade', 'wren', 'one');
      s.sendNudge('kade', 'wren', 'two');
      s.recordBoardEvent('kade', 'moved #1');
      expect(s.getStats()).toEqual({ total: 3, pending: 3, byType: { nudge: 2, 'board-event': 1 } });
    } finally { s.close(); }
  });
});

describe('#4965 the queued-state migration leaves an unknown table shape alone', () => {
  test('negative: a delivery_status check it does not recognise is not rewritten', () => {
    const fresh = path.join(dir, 'fresh.db');
    new MessageStore(fresh).close();
    const f = new Database(fresh, { readonly: true });
    const ddl = (f.prepare("SELECT sql FROM sqlite_master WHERE type='table' AND name='messages'").get() as { sql: string }).sql;
    f.close();
    expect(ddl).toContain("'queued'");
    const odd = ddl.replace(/'pending','delivered','failed','queued'/, "'pending','delivered','failed','parked'");
    expect(odd).not.toContain("'queued'");
    const file = path.join(dir, 'odd.db');
    const d = new Database(file);
    d.exec(odd);
    d.close();
    new MessageStore(file).close();
    const after = new Database(file, { readonly: true });
    const kept = (after.prepare("SELECT sql FROM sqlite_master WHERE type='table' AND name='messages'").get() as { sql: string }).sql;
    after.close();
    expect(kept).toContain("'parked'");
    expect(kept).not.toContain("'queued'");
  });
});
