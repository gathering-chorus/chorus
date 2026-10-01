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
