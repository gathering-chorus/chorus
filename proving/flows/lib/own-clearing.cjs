// @test-type: unit — fixture helper for the Clearing specs, not a suite
/**
 * #4417 — which Clearing a spec drives.
 *
 * Unset CLEARING_URL used to mean "skip", so the six Clearing specs measured
 * nothing on any nightly while reading as green. Now unset means the spec
 * starts its own Clearing, with its whole world in a temp dir
 * (directing/clearing/tests/lib/test-clearing-world.cjs) and its own credential.
 * Set means that Clearing (a variant), with CLEARING_TOKEN as its credential.
 * Pointing it at the live room is refused: nobody opts back into prod by accident.
 */
const fs = require('fs');
const os = require('os');
const path = require('path');
const { spawn } = require('child_process');
const { testClearingEnv, signedInSession } = require('../../../directing/clearing/tests/lib/test-clearing-world.cjs');

const CLEARING_SRC = path.resolve(__dirname, '..', '..', '..', 'directing', 'clearing');

/** The live room, by any address it answers on. */
function isLiveClearing(url) {
  let u;
  try { u = new URL(url); } catch { return false; }
  if (/(^|\.)lightlifeurbangardens\.com$/i.test(u.hostname)) return true;
  if (/^clearing\./i.test(u.hostname)) return true;
  const port = u.port || (u.protocol === 'https:' ? '443' : '80');
  return port === '3470' || port === '3471';
}

/** opts.spine: lines written to the own Clearing's spine log before it starts.
 *  opts.signedInAs: a WebID; the result's .session() is then a clearing_session cookie for it.
 *  opts.env: extra env for the Clearing (e.g. a stub's address). */
function ownClearing(test, opts = {}) {
  const given = process.env.CLEARING_URL;
  if (given && isLiveClearing(given)) {
    throw new Error(`CLEARING_URL=${given} is the live Clearing — refused (#4417). Unset it and the spec starts its own, or point it at a variant room.`);
  }
  const port = given ? 0 : 20000 + Math.floor(Math.random() * 20000);
  const url = given ? given.replace(/\/$/, '') : `http://127.0.0.1:${port}`;
  const token = given ? (process.env.CLEARING_TOKEN || '') : `test-${process.pid}-${Date.now()}`;
  let child = null;
  let dir = '';
  let sessionCookie = '';

  test.beforeAll(async () => {
    if (given) return;
    const entry = path.join(CLEARING_SRC, 'dist', 'server.js');
    if (!fs.existsSync(entry)) {
      throw new Error(`clearing is not built: ${entry} is missing. Build directing/clearing first (npm run build), or point CLEARING_URL at a variant room.`);
    }
    dir = fs.mkdtempSync(path.join(os.tmpdir(), 'own-clearing-'));
    const env = testClearingEnv(dir, port, token);
    if (opts.spine && opts.spine.length) fs.writeFileSync(env.CHORUS_LOG_FILE, opts.spine.join('\n') + '\n');
    if (opts.signedInAs) sessionCookie = signedInSession(dir, opts.signedInAs);
    child = spawn(process.execPath, [entry], {
      cwd: CLEARING_SRC,
      env: { ...process.env, ...env, ...(opts.env || {}) },
      stdio: ['ignore', 'ignore', 'pipe'],
    });
    let err = '';
    child.stderr.on('data', (d) => { err = (err + String(d)).slice(-2000); });
    const deadline = Date.now() + 30000;
    for (;;) {
      if (await fetch(`${url}/health`).then((r) => r.ok).catch(() => false)) return;
      if (child.exitCode !== null) throw new Error(`own Clearing exited (code ${child.exitCode}): ${err.trim() || 'no stderr'}`);
      if (Date.now() > deadline) throw new Error(`own Clearing did not answer on ${url} in 30s: ${err.trim() || 'no stderr'}`);
      await new Promise((r) => setTimeout(r, 250));
    }
  });

  test.afterAll(() => {
    if (child && child.exitCode === null) child.kill('SIGTERM');
    if (dir) fs.rmSync(dir, { recursive: true, force: true });
  });

  return { url, token, auth: { Authorization: `Bearer ${token}` }, own: !given, session: () => sessionCookie };
}

module.exports = { ownClearing, isLiveClearing };
