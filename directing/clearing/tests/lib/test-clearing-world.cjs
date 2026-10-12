// @test-type: unit — fixture helper, not a suite
// #4417 — the one place a test Clearing gets its world. Every file the server
// would otherwise read or write under the live defaults goes into `dir`:
// HOME (so ~/.chorus — token, room journal, tailer offsets),
// the projects dir it tails (live default: every role's transcripts), the
// message file (/tmp/bridge-messages.json), scan and pulse files, the spine
// log and messages.db. Pulse is dead-ported and the nudge binary is a no-op,
// so nothing reaches a role either.
const crypto = require('crypto');
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
    CLEARING_AUDIO_DIR: path.join(dir, 'audio-uploads'),
    CLEARING_UPLOAD_DIR: path.join(dir, 'uploads'),
    CLEARING_PROJECTS_DIR: path.join(dir, 'projects'),
    CLEARING_TAILER_OFFSETS: path.join(home, '.chorus', 'clearing', 'tailer-offsets.json'),
    CLEARING_JOURNAL: path.join(home, '.chorus', 'clearing', 'room.jsonl'),
    CLEARING_SCAN_DIR: path.join(dir, 'scan'),
    CLEARING_PULSE_FILE: path.join(dir, 'pulse-latest.json'),
    CLEARING_SPINE_FILE: path.join(dir, 'chorus.log'),
    CHORUS_LOG_FILE: path.join(dir, 'chorus.log'),
    // the stream pane reads CHORUS_SPINE, then CHORUS_HOME/chorus.log; a shell's
    // CHORUS_HOME is the repo, so pin both or the pane reads whatever it finds.
    CHORUS_SPINE: path.join(dir, 'chorus.log'),
    CHORUS_HOME: path.join(home, '.chorus'),
    CLEARING_CHORUS_HOME: path.join(home, '.chorus'),
    CHORUS_MESSAGES_DB: path.join(dir, 'messages.db'),
    SHARE_STATE_FILE: path.join(dir, 'share-state.json'),
    CHORUS_INJECT_DRY_RUN: '1',
    PULSE_URL: 'http://127.0.0.1:1',
    NUDGE_BINARY: nudge,
    BUZZ_ROOM_ENABLED: '0',
    // Allow-set reads and any athena-make call would reach the live services;
    // a test Clearing talks to neither.
    ATHENA_MAKE_URL: 'http://127.0.0.1:1',
    CHORUS_FUSEKI_QUERY: 'http://127.0.0.1:1/query',
    // the account page's password change talks to the identity server (CSS);
    // a test Clearing must never reach the live one.
    CSS_LOCAL_BASE: 'http://127.0.0.1:1',
    // the room's AI chat (participants.ts) calls the Anthropic API; a test Clearing never does.
    ANTHROPIC_BASE_URL: 'http://127.0.0.1:1',
    ANTHROPIC_API_KEY: 'test-no-key',
  };
}

const b64u = (b) => b.toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');

/**
 * #4412 — a browser signed in at the Chorus door, for the test Clearing: writes
 * the door's cookie key into the world (SHARE_STATE_FILE) and returns the
 * chorus_share_session value the server will verify. The Clearing has no
 * sign-in of its own; this is the only session it trusts.
 */
function doorSession(dir, webid) {
  const key = crypto.randomBytes(32);
  fs.writeFileSync(path.join(dir, 'share-state.json'), JSON.stringify({ cookie_key: b64u(key) }), { mode: 0o600 });
  const payload = b64u(Buffer.from(JSON.stringify({ webid, exp: Math.floor(Date.now() / 1000) + 3600 })));
  return `${payload}.${b64u(crypto.createHmac('sha256', key).update(payload).digest())}`;
}

module.exports = { testClearingEnv, doorSession };
