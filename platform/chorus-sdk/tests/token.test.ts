// @test-type: unit — identity mint via a fixture minter script; no live CSS,
// no real credentials (CHORUS_IDENTITY_TOKEN_BIN points into a tmp dir).
/**
 * #3719 — ES256 identity mint for SDK callers (retires the #3619 HS256 mint).
 * Jeff's experience under test: the SDK's envelope-secured POSTs carry a CSS
 * identity token minted through chorus-identity-token; scope is NOT in the
 * token (it is chorus:hasScope model data, #3689). When no credential is
 * available the SDK fails OPEN (returns null, callers send unauthenticated) —
 * a missing identity must never break emit's fire-and-forget contract.
 */
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { mintServiceToken } from '../src/token';

const FIXTURE_JWT = 'eyJhbGciOiJFUzI1NiJ9.eyJ3ZWJpZCI6IngifQ.c2ln'; // 3 parts, content irrelevant here

function fixtureMinter(dir: string, body: string): string {
  const bin = path.join(dir, 'chorus-identity-token');
  fs.writeFileSync(bin, `#!/bin/sh\n${body}\n`, { mode: 0o755 });
  return bin;
}

describe('mintServiceToken (#3719)', () => {
  let dir: string;
  beforeEach(() => { dir = fs.mkdtempSync(path.join(os.tmpdir(), 'ident-')); });
  afterEach(() => {
    delete process.env.CHORUS_IDENTITY_TOKEN_BIN;
    delete process.env.CHORUS_ROLE;
    fs.rmSync(dir, { recursive: true, force: true });
  });

  test('returns the minter script\'s token verbatim', () => {
    process.env.CHORUS_IDENTITY_TOKEN_BIN = fixtureMinter(dir, `printf '%s' '${FIXTURE_JWT}'`);
    expect(mintServiceToken()).toBe(FIXTURE_JWT);
  });

  test('passes the process role to the minter (identity follows the session, not a hardcode)', () => {
    process.env.CHORUS_ROLE = 'silas';
    const outFile = path.join(dir, 'role.out');
    process.env.CHORUS_IDENTITY_TOKEN_BIN = fixtureMinter(
      dir, `printf '%s' "$1" > '${outFile}'; printf '%s' '${FIXTURE_JWT}'`);
    expect(mintServiceToken()).toBe(FIXTURE_JWT);
    expect(fs.readFileSync(outFile, 'utf8')).toBe('silas');
  });

  test('fails OPEN (null) when the minter exits non-zero — never throws', () => {
    process.env.CHORUS_IDENTITY_TOKEN_BIN = fixtureMinter(dir, 'exit 3');
    expect(mintServiceToken()).toBeNull();
  });

  test('fails OPEN (null) on non-JWT output — never emits junk as a bearer', () => {
    process.env.CHORUS_IDENTITY_TOKEN_BIN = fixtureMinter(dir, `printf '%s' 'not a jwt'`);
    expect(mintServiceToken()).toBeNull();
  });

  test('fails OPEN (null) when the minter binary is missing entirely', () => {
    process.env.CHORUS_IDENTITY_TOKEN_BIN = path.join(dir, 'nonexistent');
    expect(mintServiceToken()).toBeNull();
  });
});
