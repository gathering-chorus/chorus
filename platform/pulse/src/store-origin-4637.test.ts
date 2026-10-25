// @test-type: integration — a real SQLite store in a tempdir, read back raw
// @domain: messages
// @card: #4637
// #4637 AC3 (Abby + Kade review, run 3) — a message's origin is written by the
// INSERT that stores it. The first cut stored the row and then ran a second
// UPDATE, so a crash between them left a row with origin NULL: the hole AC3 closes.
import Database from 'better-sqlite3';
import fs from 'fs';
import os from 'os';
import path from 'path';
import { MessageStore } from './store';

let dir: string;
let dbPath: string;
let store: MessageStore;

beforeEach(() => {
  dir = fs.mkdtempSync(path.join(os.tmpdir(), 'origin-4637-'));
  dbPath = path.join(dir, 'messages.db');
  store = new MessageStore(dbPath);
});
afterEach(() => fs.rmSync(dir, { recursive: true, force: true }));

const originOf = (id: number): string | null =>
  (new Database(dbPath, { readonly: true }).prepare('SELECT origin FROM messages WHERE id = ?').get(id) as { origin: string | null }).origin;

test('a nudge and a jeff-input carry their origin from the insert itself', () => {
  const n = store.sendNudge('kade', 'silas', 'hi', undefined, 'r2r', 'none', undefined, false, 'library/terminal');
  const j = store.sendJeffInput('silas', 'go', 'trace-1', 'phone/clearing-v2');
  expect(originOf(n)).toBe('library/terminal');
  expect(originOf(j)).toBe('phone/clearing-v2');
});

// NEGATIVE PROOF: a row stored without an origin reads NULL, so the check above
// separates the two states; and no second write path exists to fill it later.
test('negative proof: no origin given is NULL, and there is no after-the-fact setter', () => {
  expect(originOf(store.sendJeffInput('silas', 'go'))).toBeNull();
  expect((store as unknown as Record<string, unknown>).setOrigin).toBeUndefined();
});
