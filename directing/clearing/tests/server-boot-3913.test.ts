// @test-type: unit — module-init catch branches under a mocked HOME tempdir; no live services.
// @domain: messages — placed by chorus-crawl's domain rules (#4538)
// @card: #3913
// @owner: wren
/**
 * #3913 — server.ts's module-init CATCH branches: first-boot generation of
 * the bridge token (#4412 removed the session secret) fire only when the
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
  test('a missing token file is generated (catch branch covered)', () => {
    const home = path.join(FAKE_HOME, '.chorus');
    jest.isolateModules(() => {
      const prev = { ...process.env };
      process.env.CHORUS_HOME = home;
      process.env.CLEARING_TEST = '1';
      // #4417 — the setup file gives every test a Clearing home; this suite is
      // about the default (homedir/.chorus), so clear the override here.
      delete process.env.CLEARING_CHORUS_HOME;
      try {
        require('../src/server');
      } finally {
        process.env = prev;
      }
    });
    expect(fs.existsSync(path.join(home, 'bridge-auth-token'))).toBe(true);
    // #4412 — the Clearing has no session cookie of its own, so no secret for one.
    expect(fs.existsSync(path.join(home, 'clearing-session-secret'))).toBe(false);
  });
});
