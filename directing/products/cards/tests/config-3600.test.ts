// @test-type: unit — pure config resolution (bucket aliases, .env/env loading, role
// detection). No network, no Vikunja: process.env + cwd only, restored after each test.
// #3600 — covers config.ts's untested behavior (loadEnv, detectRole fallback,
// resolveBucket aliases) — the real drag under the cards coverage floor.
import * as fs from 'fs';
import { resolveBucket, loadEnv, detectRole, GATHERING } from '../src/config';

jest.mock('fs');
const mockFs = fs as jest.Mocked<typeof fs>;

describe('resolveBucket', () => {
  it('resolves an exact bucket name to its id', () => {
    expect(resolveBucket(GATHERING, 'wip')).toBe(GATHERING.buckets.wip);
    expect(resolveBucket(GATHERING, 'Done')).toBe(GATHERING.buckets.done);
  });

  it('resolves status aliases (case-insensitive) to the canonical bucket', () => {
    expect(resolveBucket(GATHERING, 'in progress')).toBe(GATHERING.buckets.now);
    expect(resolveBucket(GATHERING, 'JT')).toBe(GATHERING.buckets['jeff-tickets']);
    expect(resolveBucket(GATHERING, "won't do")).toBe(GATHERING.buckets['wont-do']);
    expect(resolveBucket(GATHERING, 'parked')).toBe(GATHERING.buckets.ideas);
  });

  it('throws on an unknown status, naming the valid buckets', () => {
    expect(() => resolveBucket(GATHERING, 'bogus-status')).toThrow(/Unknown status "bogus-status"/);
    expect(() => resolveBucket(GATHERING, 'bogus-status')).toThrow(/wip/);
  });
});

describe('detectRole', () => {
  const original = process.env.DEPLOY_ROLE;
  afterEach(() => {
    if (original === undefined) delete process.env.DEPLOY_ROLE;
    else process.env.DEPLOY_ROLE = original;
  });

  it('returns a valid DEPLOY_ROLE verbatim', () => {
    process.env.DEPLOY_ROLE = 'kade';
    expect(detectRole()).toBe('kade');
  });

  it('lower-cases DEPLOY_ROLE before matching', () => {
    process.env.DEPLOY_ROLE = 'SILAS';
    expect(detectRole()).toBe('silas');
  });

  it('accepts the automation role', () => {
    process.env.DEPLOY_ROLE = 'automation';
    expect(detectRole()).toBe('automation');
  });

  it('warns and falls back to cwd parsing when DEPLOY_ROLE is invalid', () => {
    process.env.DEPLOY_ROLE = 'not-a-real-role';
    const warn = jest.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const role = detectRole();
    expect(warn).toHaveBeenCalledWith(expect.stringContaining('falling back to cwd parse'));
    expect(['kade', 'wren', 'silas', 'automation']).toContain(role);
    warn.mockRestore();
  });

  it('warns (unset variant) and falls back when DEPLOY_ROLE is missing', () => {
    delete process.env.DEPLOY_ROLE;
    const warn = jest.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const role = detectRole();
    expect(warn).toHaveBeenCalledWith(expect.stringContaining('DEPLOY_ROLE unset'));
    expect(['kade', 'wren', 'silas', 'automation']).toContain(role);
    warn.mockRestore();
  });
});

describe('loadEnv', () => {
  const url = process.env.VIKUNJA_URL;
  const token = process.env.VIKUNJA_TOKEN;
  afterEach(() => {
    if (url === undefined) delete process.env.VIKUNJA_URL; else process.env.VIKUNJA_URL = url;
    if (token === undefined) delete process.env.VIKUNJA_TOKEN; else process.env.VIKUNJA_TOKEN = token;
  });

  it('returns url + token straight from env when both are set (no .env read)', () => {
    process.env.VIKUNJA_URL = 'http://vikunja.test:3456';
    process.env.VIKUNJA_TOKEN = 'tok-abc';
    expect(loadEnv()).toEqual({ url: 'http://vikunja.test:3456', token: 'tok-abc' });
  });

  describe('.env-file fallback (env vars unset)', () => {
    const role = process.env.DEPLOY_ROLE;
    beforeEach(() => {
      delete process.env.VIKUNJA_URL;
      delete process.env.VIKUNJA_TOKEN;
      process.env.DEPLOY_ROLE = 'kade';
      jest.clearAllMocks();
    });
    afterEach(() => {
      if (role === undefined) delete process.env.DEPLOY_ROLE; else process.env.DEPLOY_ROLE = role;
    });

    it('reads url + role-specific token from a .env file', () => {
      mockFs.existsSync.mockReturnValue(true);
      mockFs.readFileSync.mockReturnValue(
        'VIKUNJA_URL=http://envfile:3456\nVIKUNJA_TOKEN_KADE=kade-token\n# comment\n',
      );
      expect(loadEnv()).toEqual({ url: 'http://envfile:3456', token: 'kade-token' });
    });

    it('defaults the url to localhost:3456 when the .env omits VIKUNJA_URL', () => {
      mockFs.existsSync.mockReturnValue(true);
      mockFs.readFileSync.mockReturnValue('VIKUNJA_TOKEN=generic-token\n');
      expect(loadEnv()).toEqual({ url: 'http://localhost:3456', token: 'generic-token' });
    });

    it('throws when the .env file has no usable token', () => {
      mockFs.existsSync.mockReturnValue(true);
      mockFs.readFileSync.mockReturnValue('VIKUNJA_URL=http://x:1\n');
      expect(() => loadEnv()).toThrow(/No Vikunja token found/);
    });

    it('throws when no .env file exists anywhere', () => {
      mockFs.existsSync.mockReturnValue(false);
      expect(() => loadEnv()).toThrow(/No .env file found/);
    });
  });
});
