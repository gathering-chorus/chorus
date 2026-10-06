// #4416 reopened — a small index for tests that check what search ANSWERS, not
// how fast a 500 MB one is. Measured 2026-10-06: in the test app the first
// search for a term took 1.1–2.6 s against the per-run copy of the live index
// (live chorus-api 0.3–0.9 s; the FTS query alone 0.12 s), so under a loaded
// pipeline it went past jest's 5 s and search-freshness failed on 4 runs
// across 3 cards. Same schema, same triggers, the newest rows and every
// watermark: the answer's shape is real, the size is not.
import Database from 'better-sqlite3';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

const SHADOW = /^messages_fts_/; // FTS5 creates these itself

export function smallIndexFrom(source: string, rows = 2000): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'small-index-'));
  const dest = path.join(dir, 'index.db');
  const src = new Database(source, { readonly: true, fileMustExist: true });
  const schema = src
    .prepare("SELECT type, name, sql FROM sqlite_master WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%'")
    .all() as Array<{ type: string; name: string; sql: string }>;
  src.close();
  const db = new Database(dest);
  // the app opens the index readonly and asks for WAL; a readonly handle can
  // only get WAL if the file is already in it
  db.pragma('journal_mode = WAL');
  for (const t of ['table', 'index', 'trigger', 'view']) {
    for (const s of schema.filter((x) => x.type === t && !SHADOW.test(x.name))) db.exec(s.sql);
  }
  db.prepare('ATTACH DATABASE ? AS src').run(source);
  db.exec(`INSERT INTO messages SELECT * FROM src.messages ORDER BY id DESC LIMIT ${Math.max(1, Math.floor(rows))}`);
  db.exec('INSERT INTO watermarks SELECT * FROM src.watermarks');
  db.exec('DETACH DATABASE src');
  db.close();
  return dest;
}
