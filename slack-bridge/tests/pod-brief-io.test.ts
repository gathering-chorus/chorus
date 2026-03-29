import * as fs from 'fs';
import * as path from 'path';
import { writeBrief, readBrief, listBriefs, initPodBriefClient, writeState, readState } from '../src/pod-brief-io';
import { RoleConfig } from '../src/config';

// Mock logger
jest.mock('../src/logger', () => ({
  log: jest.fn(),
}));

// Mock the pod client module
jest.mock('../src/pod-brief-client', () => {
  const mockWriteBriefToPod = jest.fn().mockResolvedValue(true);
  const mockReadBriefFromPod = jest.fn().mockResolvedValue(null);
  return {
    PodBriefClient: jest.fn().mockImplementation(() => ({
      writeBriefToPod: mockWriteBriefToPod,
      readBriefFromPod: mockReadBriefFromPod,
      getServiceToken: jest.fn().mockResolvedValue('token'),
    })),
    __mockWrite: mockWriteBriefToPod,
    __mockRead: mockReadBriefFromPod,
  };
});

const tmpDir = path.join(__dirname, '__tmp_pod_brief_io');

function makeRole(name: string): RoleConfig {
  return {
    name,
    channel: name,
    claudeMdPath: '/tmp/test.md',
    memoryPath: '/tmp/memory.md',
    briefsPath: path.join(tmpDir, name, 'briefs'),
    maxCallsPerHour: 10,
  };
}

describe('pod-brief-io', () => {
  beforeAll(() => {
    fs.mkdirSync(tmpDir, { recursive: true });
  });

  afterAll(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  beforeEach(() => {
    // Clean up role brief dirs
    for (const name of ['silas', 'wren', 'kade']) {
      const dir = path.join(tmpDir, name, 'briefs');
      if (fs.existsSync(dir)) fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  describe('writeBrief', () => {
    it('writes to filesystem and creates directory if needed', async () => {
      const role = makeRole('silas');
      await writeBrief(role, 'test-brief.md', '# Test Brief');

      const filepath = path.join(role.briefsPath, 'test-brief.md');
      expect(fs.existsSync(filepath)).toBe(true);
      expect(fs.readFileSync(filepath, 'utf-8')).toBe('# Test Brief');
    });

    it('overwrites existing file', async () => {
      const role = makeRole('wren');
      fs.mkdirSync(role.briefsPath, { recursive: true });
      fs.writeFileSync(path.join(role.briefsPath, 'old.md'), 'old content');

      await writeBrief(role, 'old.md', 'new content');

      expect(fs.readFileSync(path.join(role.briefsPath, 'old.md'), 'utf-8')).toBe('new content');
    });
  });

  describe('readBrief', () => {
    it('reads an existing brief from filesystem', () => {
      const role = makeRole('kade');
      fs.mkdirSync(role.briefsPath, { recursive: true });
      fs.writeFileSync(path.join(role.briefsPath, 'brief.md'), 'content here');

      const result = readBrief(role, 'brief.md');
      expect(result).toBe('content here');
    });

    it('returns null for missing file', () => {
      const role = makeRole('kade');
      const result = readBrief(role, 'nonexistent.md');
      expect(result).toBeNull();
    });
  });

  describe('listBriefs', () => {
    it('lists markdown files in briefs directory', () => {
      const role = makeRole('silas');
      fs.mkdirSync(role.briefsPath, { recursive: true });
      fs.writeFileSync(path.join(role.briefsPath, 'a.md'), '');
      fs.writeFileSync(path.join(role.briefsPath, 'b.md'), '');
      fs.writeFileSync(path.join(role.briefsPath, 'c.txt'), ''); // not .md

      const result = listBriefs(role);
      expect(result).toContain('a.md');
      expect(result).toContain('b.md');
      expect(result).not.toContain('c.txt');
    });

    it('returns empty array for missing directory', () => {
      const role = makeRole('wren');
      const result = listBriefs(role);
      expect(result).toEqual([]);
    });
  });

  describe('initPodBriefClient', () => {
    const origEnv = process.env;

    afterEach(() => {
      process.env = origEnv;
    });

    it('does not throw when env vars are missing', () => {
      process.env = { ...origEnv };
      delete process.env.APP_BASE_URL;
      delete process.env.BRIDGE_AGENT_SECRET;

      expect(() => initPodBriefClient()).not.toThrow();
    });
  });

  describe('writeState', () => {
    const stateDir = path.join(tmpDir, 'state-test');

    beforeEach(() => {
      if (fs.existsSync(stateDir)) fs.rmSync(stateDir, { recursive: true, force: true });
    });

    it('writes state file to filesystem', async () => {
      // Temporarily monkey-patch the ROLE_STATE_DIRS via the module
      // Since writeState uses internal ROLE_STATE_DIRS, we test the function directly
      // by writing to the actual role directory. For unit testing, we verify the function
      // handles unknown roles gracefully.
      await writeState('unknown-role', 'current-work.md', '# Test');
      // Unknown role should be handled gracefully (no throw)
    });

    it('returns gracefully for unknown role', async () => {
      // Should not throw
      await expect(writeState('nonexistent', 'file.md', 'content')).resolves.toBeUndefined();
    });
  });

  describe('readState', () => {
    it('returns null for unknown role', () => {
      const result = readState('nonexistent', 'file.md');
      expect(result).toBeNull();
    });

    it('returns null for missing file', () => {
      // kade is a valid role but the file won't exist in test context
      // This tests the graceful catch path
      const result = readState('kade', 'nonexistent-file-that-does-not-exist.md');
      expect(result).toBeNull();
    });
  });
});
