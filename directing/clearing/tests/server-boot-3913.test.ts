// @test-type: unit — module-init catch branches under a mocked HOME tempdir; no live services.
/**
 * #3913 — server.ts's module-init CATCH branches: first-boot generation of
 * the bridge token and session secret (lines ~100-116) fire only when the
 * files DON'T exist. The whole suite always ran with them present, so these
 * statements were permanently uncovered — the coverage red's actual cause.
 */
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

// server.ts hardcodes CHORUS_HOME = os.homedir()/.chorus (line 94) — redirect
// homedir for this suite so first-boot writes land in a tempdir, never the
// real ~/.chorus (a test brings its own world, #3528).
const FAKE_HOME = fs.mkdtempSync(path.join(os.tmpdir(), 'clearing-boot-home-'));
jest.mock('os', () => {
  const real = jest.requireActual('os');
  return { ...real, homedir: () => FAKE_HOME };
});

jest.mock('../src/participants', () => ({ Participants: class { getRoles() { return []; } updateContext() {} } }));
jest.mock('../src/tailer', () => {
  const { EventEmitter } = require('events');
  return { ChorusLogTailer: class extends EventEmitter { start() {} stop() {} } };
});

describe('#3913 first-boot secret generation', () => {
  test('missing token + secret files are generated (catch branches covered)', () => {
    const home = path.join(FAKE_HOME, '.chorus');
    jest.isolateModules(() => {
      const prev = { ...process.env };
      process.env.CHORUS_HOME = home;
      process.env.CLEARING_TEST = '1';
      try {
        require('../src/server');
      } finally {
        process.env = prev;
      }
    });
    expect(fs.existsSync(path.join(home, 'bridge-auth-token'))).toBe(true);
    expect(fs.existsSync(path.join(home, 'clearing-session-secret'))).toBe(true);
    const mode = fs.statSync(path.join(home, 'clearing-session-secret')).mode & 0o777;
    expect(mode).toBe(0o600);
  });
});
