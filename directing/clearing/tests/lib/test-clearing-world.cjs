// @test-type: unit — fixture helper, not a suite
// #4417 — the one place a test Clearing gets its world. Every file the server
// would otherwise read or write under the live defaults goes into `dir`:
// HOME (so ~/.chorus — token, room journal, tailer offsets, person sessions),
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
    CHORUS_MESSAGES_DB: path.join(dir, 'messages.db'),
    SHARE_STATE_FILE: path.join(dir, 'share-state.json'),
    CHORUS_INJECT_DRY_RUN: '1',
    PULSE_URL: 'http://127.0.0.1:1',
    NUDGE_BINARY: nudge,
    BUZZ_ROOM_ENABLED: '0',
    // Session-row writes (touchPersonSession) and allow-set reads would reach the
    // live athena-make and Fuseki; a test Clearing talks to neither.
    ATHENA_MAKE_URL: 'http://127.0.0.1:1',
    CHORUS_FUSEKI_QUERY: 'http://127.0.0.1:1/query',
  };
}

const b64u = (b) => b.toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');

/**
 * A signed-in browser for the test Clearing: writes its session secret and one
 * person-session record into the world, and returns the clearing_session cookie
 * value the server will verify (signCookie in solid-oidc.ts). Call before the
 * server starts; it reads the secret once, at load.
 */
function signedInSession(dir, webid) {
  const chorusHome = path.join(dir, 'home', '.chorus');
  fs.mkdirSync(chorusHome, { recursive: true });
  const secret = crypto.randomBytes(32).toString('hex');
  fs.writeFileSync(path.join(chorusHome, 'clearing-session-secret'), secret, { mode: 0o600 });
  const psk = `test-psk-${process.pid}-${Date.now()}`;
  const record = { key: psk, principal: 'jeff', rowName: psk, row: {}, idToken: '', exp: Math.floor(Date.now() / 1000) + 3600, lastSeenWrite: Date.now() };
  fs.writeFileSync(path.join(chorusHome, 'clearing-person-sessions.json'), JSON.stringify({ [psk]: record }));
  const body = b64u(Buffer.from(JSON.stringify({ typ: 'session', webid, iat: Date.now(), psk })));
  return `${body}.${b64u(crypto.createHmac('sha256', secret).update(body).digest())}`;
}

module.exports = { testClearingEnv, signedInSession };
