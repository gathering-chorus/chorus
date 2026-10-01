// #4417 — every pulse jest process gets its own world: the shared secret, the
// spine log and the messages store default to ~/.chorus and platform/pulse,
// which are the live ones. A test that wants a value sets it itself.
const fs = require('fs');
const os = require('os');
const path = require('path');
const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'pulse-test-'));
if (!process.env.CHORUS_PULSE_SECRET_FILE) process.env.CHORUS_PULSE_SECRET_FILE = path.join(dir, 'pulse-nudge.secret');
if (!process.env.CHORUS_LOG_FILE) process.env.CHORUS_LOG_FILE = path.join(dir, 'chorus.log');
