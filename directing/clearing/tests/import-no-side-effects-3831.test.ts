// @test-type: unit — imports the module and calls exported fns; the handle checks run in short-lived subprocesses. No ports, no network, no fixtures.
/**
 * #3831 — importing the server must not start the server.
 *
 * The clearing suite passed 567 tests and then hung until the nightly killed
 * it at 1800 seconds. Three nights. The tests were green the entire time,
 * which is the part worth remembering: the failure was invisible to the thing
 * built to see it.
 *
 * Cause: src/server.ts started a filesystem watcher, a log tailer and two
 * timers at MODULE level. Four test files import it for one function each, and
 * every one inherited live handles nothing could close.
 *
 * The handle checks are SUBPROCESSES on purpose. "Does the process exit?" is
 * the actual question; a grep for `setInterval` would pass the day somebody
 * starts a watcher a different way. A subprocess either exits or it does not.
 */

import { execFileSync } from 'child_process';
import fs from 'fs';
import path from 'path';
import { socketSessionAuthed, startBackgroundWork } from '../src/server';

const CLEARING = path.join(__dirname, '..');
/** socketSessionAuthed takes the raw Cookie header. Empty = nobody signed in. */
const NO_COOKIE = '';

// #4073 — this file flipped 4 times in 21 nightly runs, every time because
// ./dist/server.js was absent or stale on the machine, never because the
// product changed. The test brings its own world (#3528): build once here.
beforeAll(() => {
  if (!fs.existsSync(path.join(CLEARING, 'dist', 'server.js'))) {
    execFileSync('npm', ['run', 'build', '--silent'], { cwd: CLEARING, stdio: 'ignore' });
  }
}, 180000);

/** Run a snippet in a fresh node process; true if it exited on its own. */
function exitsCleanly(snippet: string, timeoutMs = 15000): boolean {
  try {
    execFileSync(process.execPath, ['-e', snippet], {
      cwd: CLEARING,
      timeout: timeoutMs,
      stdio: 'pipe',
      env: { ...process.env, CLEARING_PROJECTS_DIR: '/tmp/import-3831-empty', PULSE_URL: 'http://127.0.0.1:1' },
    });
    return true;
  } catch {
    // A timeout means the process was still alive — the hang, reproduced.
    return false;
  }
}

describe('#3831 importing the module does not start it', () => {
  test('a bare import is usable AND leaves a process able to exit', async () => {
    // Two halves of one claim. THIS FILE imports src/server for a single
    // function, exactly as the four offending suites do — so the function
    // working here is half the evidence, and a fresh process exiting is the
    // other. Kade's bisect reproducer is the subprocess line: before the fix
    // it hangs to the timeout.
    await expect(socketSessionAuthed(NO_COOKIE)).resolves.toBe(false);
    expect(exitsCleanly("require('./dist/server.js');")).toBe(true);
  });

  test('import-and-use exits too — the shape the offending suites had', async () => {
    await expect(socketSessionAuthed(NO_COOKIE)).resolves.toBe(false);
    expect(exitsCleanly(`
      const m = require('./dist/server.js');
      if (typeof m.socketSessionAuthed !== 'function') { process.exit(3); }
    `)).toBe(true);
  });

  test('the background work starts the tailers when CALLED, and only then', () => {
    // The work did not disappear, it moved to a caller that means it. Called
    // with stubs, so this proves the WIRING rather than that an export exists.
    // If someone flattens startBackgroundWork into a no-op, the live Clearing
    // stops tailing sessions and the room silently goes quiet — green tests,
    // dead feature, the exact family this card is in.
    const started: string[] = [];
    startBackgroundWork({
      tailer: { start: () => started.push('tailer') },
      sessionTailer: { start: () => started.push('sessionTailer') },
      timers: false,
    });
    expect(started.sort()).toEqual(['sessionTailer', 'tailer']);
  });

  test('NEGATIVE PROOF: a module-level interval WOULD hang, and a watcher WOULD too', async () => {
    // The guard proves it can fail — twice, for the two shapes that actually
    // occur. Without these, the tests above prove only that node exits, which
    // node does. The fs.watchFile case is the one that bit us: FSEVENTWRAP,
    // from SessionTailer.start(), reintroducible without ever typing
    // setInterval.
    await expect(socketSessionAuthed(NO_COOKIE)).resolves.toBe(false);
    expect(exitsCleanly('setInterval(() => {}, 1000);', 4000)).toBe(false);
    expect(exitsCleanly("require('fs').watchFile(__filename, () => {});", 4000)).toBe(false);
  });
});
