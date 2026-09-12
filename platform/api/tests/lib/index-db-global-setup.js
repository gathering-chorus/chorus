// #4152 — the in-process harness must never open the live ~/.chorus/index.db.
// Under the nightly the api tests ran beside the live chorus-api and the cards
// package on that one file; freshness answered 503, search 500, "database is
// locked" (2026-09-12, 9 api cases + 1 cards case, all green by hand).
//
// jest globalSetup runs ONCE in the parent before workers fork, and workers
// inherit process.env, so one online backup here serves every test file.
// Cost: one ~500 MB SQLite backup per `jest` run (a few seconds on SSD).
// CHORUS_DB_PATH already set (a werk variant, a hand run) is honoured as-is.
const fs = require('fs');
const os = require('os');
const path = require('path');

module.exports = async function indexDbGlobalSetup() {
  if (process.env.CHORUS_DB_PATH) return;
  const live = path.join(os.homedir(), '.chorus', 'index.db');
  if (!fs.existsSync(live)) return; // nothing to isolate; the app answers 503 by itself
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'chorus-api-test-index-'));
  const copy = path.join(dir, 'index.db');
  const Database = require('better-sqlite3');
  const src = new Database(live, { readonly: true, fileMustExist: true });
  try {
    await src.backup(copy); // online backup: includes the WAL, never locks writers
  } finally {
    src.close();
  }
  process.env.CHORUS_DB_PATH = copy;
  process.env.CHORUS_TEST_INDEX_DIR = dir;
};
