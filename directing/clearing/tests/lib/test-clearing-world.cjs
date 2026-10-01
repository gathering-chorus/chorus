// @test-type: unit — fixture helper, not a suite
// #4417 — the one place a test Clearing gets its world. Every file the server
// would otherwise read or write under the live defaults goes into `dir`:
// HOME (so ~/.chorus — token, room journal, tailer offsets, person sessions),
// the projects dir it tails (live default: every role's transcripts), the
// message file (/tmp/bridge-messages.json), scan and pulse files, the spine
// log and messages.db. Pulse is dead-ported and the nudge binary is a no-op,
// so nothing reaches a role either.
const fs = require('fs');
const path = require('path');

function testClearingEnv(dir, port, token) {
  const home = path.join(dir, 'home');
  fs.mkdirSync(path.join(home, '.chorus', 'clearing'), { recursive: true });
  for (const d of ['projects', 'scan']) fs.mkdirSync(path.join(dir, d), { recursive: true });
  if (token) fs.writeFileSync(path.join(home, '.chorus', 'bridge-auth-token'), token);
  const nudge = path.join(dir, 'nudge');
  fs.writeFileSync(nudge, '#!/bin/sh\nexit 0\n', { mode: 0o755 });
  return {
    HOME: home,
    COMMAND_CHANNEL_PORT: String(port),
    CLEARING_HTTPS_PORT: String(port + 1),
    CLEARING_MSG_FILE: path.join(dir, 'bridge-messages.json'),
    CLEARING_PROJECTS_DIR: path.join(dir, 'projects'),
    CLEARING_TAILER_OFFSETS: path.join(home, '.chorus', 'clearing', 'tailer-offsets.json'),
    CLEARING_JOURNAL: path.join(home, '.chorus', 'clearing', 'room.jsonl'),
    CLEARING_SCAN_DIR: path.join(dir, 'scan'),
    CLEARING_PULSE_FILE: path.join(dir, 'pulse-latest.json'),
    CLEARING_SPINE_FILE: path.join(dir, 'chorus.log'),
    CHORUS_LOG_FILE: path.join(dir, 'chorus.log'),
    CHORUS_MESSAGES_DB: path.join(dir, 'messages.db'),
    SHARE_STATE_FILE: path.join(dir, 'share-state.json'),
    CHORUS_INJECT_DRY_RUN: '1',
    PULSE_URL: 'http://127.0.0.1:1',
    NUDGE_BINARY: nudge,
    BUZZ_ROOM_ENABLED: '0',
  };
}

module.exports = { testClearingEnv };
