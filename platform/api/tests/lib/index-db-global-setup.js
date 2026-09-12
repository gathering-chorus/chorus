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

// #4152 AC5 — under the nightly the runner already sets CHORUS_DB_PATH to its
// suite world (<tmp>/werk-suite-world-<slot>/index.db), a 36 KB empty schema.
// The freshness and search tests read the index, so they answered 503 / 500
// on every run since the runner took the npm lane (2026-09-11 19:16). A
// world path that is missing or holds no index rows is seeded the same way.
const SEED_BELOW_BYTES = 1024 * 1024; // an empty schema is ~36 KB; the live index is ~490 MB

function needsSeed(p) {
  try { return !fs.existsSync(p) || fs.statSync(p).size < SEED_BELOW_BYTES; } catch { return true; }
}

async function backupLiveInto(live, dest) {
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  const Database = require('better-sqlite3');
  const src = new Database(live, { readonly: true, fileMustExist: true });
  try {
    await src.backup(dest); // online backup: includes the WAL, never locks writers
  } finally {
    src.close();
  }
}

module.exports = async function indexDbGlobalSetup() {
  const live = path.join(os.homedir(), '.chorus', 'index.db');
  if (!fs.existsSync(live)) return; // nothing to isolate; the app answers 503 by itself
  const preset = process.env.CHORUS_DB_PATH;
  if (preset) {
    if (path.resolve(preset) === path.resolve(live)) return; // the harness refuses this itself
    if (needsSeed(preset)) {
      for (const sfx of ['-wal', '-shm']) { try { fs.rmSync(preset + sfx, { force: true }); } catch {} }
      await backupLiveInto(live, preset);
      process.env.CHORUS_TEST_INDEX_SEEDED = preset;
    }
    return;
  }
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'chorus-api-test-index-'));
  const copy = path.join(dir, 'index.db');
  await backupLiveInto(live, copy);
  process.env.CHORUS_DB_PATH = copy;
  process.env.CHORUS_TEST_INDEX_DIR = dir;
};
module.exports.needsSeed = needsSeed;
module.exports.backupLiveInto = backupLiveInto;
