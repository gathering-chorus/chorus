// @test-type: unit — signal:security is the subject under test (the auth gate), pure functions, no live server
// #3485 — AC4: POST /api/nudge accepts only the MCP server, authenticated by a
// shared secret (not a guessable header). Pins the gate predicate:
//   - matching secret → authorized
//   - missing/wrong secret → rejected (the pre-#3485 guessable header is gone)
//   - PULSE_ALLOW_DIRECT_POST=1 → authorized (test/migration escape)

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { resolvePulseSecret, secretsMatch, callerIsAuthorized } from './pulse-secret';

// secretPath() reads CHORUS_PULSE_SECRET_FILE at call-time, so setting it
// before the first resolvePulseSecret() call is sufficient.
const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'pulse-secret-'));
const SECRET_FILE = path.join(TMP, 'pulse-nudge.secret');

beforeAll(() => {
  process.env.CHORUS_PULSE_SECRET_FILE = SECRET_FILE;
  delete process.env.CHORUS_PULSE_SECRET;
  delete process.env.PULSE_ALLOW_DIRECT_POST;
});

afterAll(() => {
  fs.rmSync(TMP, { recursive: true, force: true });
});

describe('#3485 pulse shared-secret', () => {
  it('resolvePulseSecret generates a 0600 secret file on first use', () => {
    const s = resolvePulseSecret();
    expect(s && s.length >= 32).toBe(true);
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- SECRET_FILE is a test-controlled constant path, not caller input.
    expect(fs.existsSync(SECRET_FILE)).toBe(true);
    // eslint-disable-next-line security/detect-non-literal-fs-filename -- SECRET_FILE is a test-controlled constant path, not caller input.
    expect(fs.statSync(SECRET_FILE).mode & 0o777).toBe(0o600);
  });

  it('secretsMatch is exact (length + value); missing/empty rejected', () => {
    const s = resolvePulseSecret() as string;
    expect(secretsMatch(s, s)).toBe(true);
    expect(secretsMatch(undefined, s)).toBe(false);
    expect(secretsMatch('', s)).toBe(false);
    expect(secretsMatch(s + 'x', s)).toBe(false);
    expect(secretsMatch('deadbeef', s)).toBe(false);
  });

  it('callerIsAuthorized: only the matching secret passes', () => {
    const s = resolvePulseSecret() as string;
    expect(callerIsAuthorized(s)).toBe(true);
    expect(callerIsAuthorized(undefined)).toBe(false);
    expect(callerIsAuthorized('not-the-secret')).toBe(false);
    // the pre-#3485 spoofable approach (any value) must NOT pass anymore
    expect(callerIsAuthorized('1')).toBe(false);
  });

  it('callerIsAuthorized: PULSE_ALLOW_DIRECT_POST=1 opts out (tests/migration)', () => {
    process.env.PULSE_ALLOW_DIRECT_POST = '1';
    expect(callerIsAuthorized(undefined)).toBe(true);
    delete process.env.PULSE_ALLOW_DIRECT_POST;
  });
});
