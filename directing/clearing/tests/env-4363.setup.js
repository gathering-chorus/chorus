// #4363 — the room journal and the tailer's offsets default to ~/.chorus/clearing.
// Tests import server.ts, which opens the journal at load, so every test process
// gets its own temp paths here: no test reads or writes the live room history.
const fs = require('fs');
const os = require('os');
const path = require('path');
const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'clearing-4363-'));
if (!process.env.CLEARING_JOURNAL) process.env.CLEARING_JOURNAL = path.join(dir, 'room.jsonl');
if (!process.env.CLEARING_TAILER_OFFSETS) process.env.CLEARING_TAILER_OFFSETS = path.join(dir, 'tailer-offsets.json');
// #4417 — a bare SessionTailer's reply.rendered goes to the spine log; without
// this every jest process appended to the live ~/.chorus/chorus.log.
if (!process.env.CHORUS_LOG_FILE) process.env.CHORUS_LOG_FILE = path.join(dir, 'chorus.log');
// #4417 — share-session reads SHARE_STATE_FILE once, at import. Pointed here,
// no jest process can read the live guard key (~/.chorus/share-oidc.json),
// whatever order modules load in. A test that needs a key writes its own.
if (!process.env.SHARE_STATE_FILE) process.env.SHARE_STATE_FILE = path.join(dir, 'share-oidc.json');
// #4417 — 10 test files import server.ts in-process. A signed-in request makes
// it PUT Session rows to athena-make and read the allow-set from Fuseki; in a
// test both go to a closed port, never the live services. A test that needs an
// answer stands up its own stub and sets the variable itself.
if (!process.env.ATHENA_MAKE_URL) process.env.ATHENA_MAKE_URL = 'http://127.0.0.1:9';
if (!process.env.CHORUS_FUSEKI_QUERY) process.env.CHORUS_FUSEKI_QUERY = 'http://127.0.0.1:9/query';
