import { PodBriefClient } from '../src/pod-brief-client';

// Mock logger to suppress output during tests
jest.mock('../src/logger', () => ({
  log: jest.fn(),
}));

// Mock global fetch
const mockFetch = jest.fn();
global.fetch = mockFetch;

describe('PodBriefClient', () => {
  let client: PodBriefClient;

  beforeEach(() => {
    jest.clearAllMocks();
    client = new PodBriefClient('http://localhost:3000', 'bridge', 'test-secret');
  });

  describe('getServiceToken', () => {
    it('fetches a token from the auth endpoint', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ token: 'jwt-token-123', expiresIn: 3600 }),
      });

      const token = await client.getServiceToken();

      expect(token).toBe('jwt-token-123');
      expect(mockFetch).toHaveBeenCalledWith(
        'http://localhost:3000/api/auth/service-token',
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ agentId: 'bridge', agentSecret: 'test-secret' }),
        })
      );
    });

    it('returns cached token on subsequent calls', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ token: 'cached-token', expiresIn: 3600 }),
      });

      await client.getServiceToken();
      const token2 = await client.getServiceToken();

      expect(token2).toBe('cached-token');
      expect(mockFetch).toHaveBeenCalledTimes(1); // Only one fetch
    });

    it('returns null on HTTP error', async () => {
      mockFetch.mockResolvedValueOnce({ ok: false, status: 401, statusText: 'Unauthorized' });

      const token = await client.getServiceToken();
      expect(token).toBeNull();
    });

    it('returns null on network error', async () => {
      mockFetch.mockRejectedValueOnce(new Error('ECONNREFUSED'));

      const token = await client.getServiceToken();
      expect(token).toBeNull();
    });
  });

  describe('writeBriefToPod', () => {
    it('writes a brief with PUT to the correct path', async () => {
      // First call: token
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ token: 'jwt-123', expiresIn: 3600 }),
      });
      // Second call: PUT
      mockFetch.mockResolvedValueOnce({ ok: true, status: 201 });

      const result = await client.writeBriefToPod('silas', 'test-brief.md', '# Brief\nContent');

      expect(result).toBe(true);
      expect(mockFetch).toHaveBeenCalledTimes(2);
      expect(mockFetch).toHaveBeenLastCalledWith(
        'http://localhost:3000/api/service/pods/jeff/coordination/briefs/to-architect/test-brief.md',
        expect.objectContaining({
          method: 'PUT',
          headers: expect.objectContaining({
            'Authorization': 'Bearer jwt-123',
            'Content-Type': 'text/markdown',
          }),
          body: '# Brief\nContent',
        })
      );
    });

    it('maps role names to correct pod inbox paths', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ token: 'jwt-123', expiresIn: 3600 }),
      });

      // Write to wren (pm)
      mockFetch.mockResolvedValueOnce({ ok: true, status: 201 });
      await client.writeBriefToPod('wren', 'brief.md', 'content');
      expect(mockFetch).toHaveBeenLastCalledWith(
        expect.stringContaining('/to-pm/brief.md'),
        expect.anything()
      );

      // Write to kade (engineer)
      mockFetch.mockResolvedValueOnce({ ok: true, status: 201 });
      await client.writeBriefToPod('kade', 'brief.md', 'content');
      expect(mockFetch).toHaveBeenLastCalledWith(
        expect.stringContaining('/to-engineer/brief.md'),
        expect.anything()
      );
    });

    it('returns false for unknown role', async () => {
      const result = await client.writeBriefToPod('unknown', 'brief.md', 'content');
      expect(result).toBe(false);
      expect(mockFetch).not.toHaveBeenCalled();
    });

    it('returns false on HTTP error', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ token: 'jwt-123', expiresIn: 3600 }),
      });
      mockFetch.mockResolvedValueOnce({ ok: false, status: 403, statusText: 'Forbidden' });

      const result = await client.writeBriefToPod('silas', 'brief.md', 'content');
      expect(result).toBe(false);
    });

    it('returns false when token acquisition fails', async () => {
      mockFetch.mockRejectedValueOnce(new Error('Network error'));

      const result = await client.writeBriefToPod('silas', 'brief.md', 'content');
      expect(result).toBe(false);
    });
  });

  describe('readBriefFromPod', () => {
    it('reads a brief with GET from the correct path', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ token: 'jwt-123', expiresIn: 3600 }),
      });
      mockFetch.mockResolvedValueOnce({
        ok: true,
        text: async () => '# Brief Content',
      });

      const content = await client.readBriefFromPod('kade', 'test-brief.md');

      expect(content).toBe('# Brief Content');
      expect(mockFetch).toHaveBeenLastCalledWith(
        'http://localhost:3000/api/service/pods/jeff/coordination/briefs/to-engineer/test-brief.md',
        expect.objectContaining({
          method: 'GET',
          headers: expect.objectContaining({ 'Authorization': 'Bearer jwt-123' }),
        })
      );
    });

    it('returns null on 404', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ token: 'jwt-123', expiresIn: 3600 }),
      });
      mockFetch.mockResolvedValueOnce({ ok: false, status: 404, statusText: 'Not Found' });

      const content = await client.readBriefFromPod('kade', 'missing.md');
      expect(content).toBeNull();
    });

    it('returns null for unknown role', async () => {
      const content = await client.readBriefFromPod('unknown', 'brief.md');
      expect(content).toBeNull();
    });
  });

  describe('writeStateToPod', () => {
    it('writes a state file with PUT to the correct path', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ token: 'jwt-123', expiresIn: 3600 }),
      });
      mockFetch.mockResolvedValueOnce({ ok: true, status: 201 });

      const result = await client.writeStateToPod('kade', 'current-work.md', '# Current Work');

      expect(result).toBe(true);
      expect(mockFetch).toHaveBeenLastCalledWith(
        'http://localhost:3000/api/service/pods/jeff/coordination/state/kade/current-work.md',
        expect.objectContaining({
          method: 'PUT',
          headers: expect.objectContaining({
            'Authorization': 'Bearer jwt-123',
            'Content-Type': 'text/markdown',
          }),
          body: '# Current Work',
        })
      );
    });

    it('uses correct path for each role', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ token: 'jwt-123', expiresIn: 3600 }),
      });

      for (const role of ['silas', 'wren', 'kade']) {
        mockFetch.mockResolvedValueOnce({ ok: true, status: 201 });
        await client.writeStateToPod(role, 'tech-debt.md', 'content');
        expect(mockFetch).toHaveBeenLastCalledWith(
          `http://localhost:3000/api/service/pods/jeff/coordination/state/${role}/tech-debt.md`,
          expect.anything()
        );
      }
    });

    it('returns false for unknown role', async () => {
      const result = await client.writeStateToPod('unknown', 'file.md', 'content');
      expect(result).toBe(false);
      expect(mockFetch).not.toHaveBeenCalled();
    });

    it('returns false on HTTP error', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ token: 'jwt-123', expiresIn: 3600 }),
      });
      mockFetch.mockResolvedValueOnce({ ok: false, status: 403, statusText: 'Forbidden' });

      const result = await client.writeStateToPod('kade', 'file.md', 'content');
      expect(result).toBe(false);
    });

    it('returns false on network error', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ token: 'jwt-123', expiresIn: 3600 }),
      });
      mockFetch.mockRejectedValueOnce(new Error('ECONNREFUSED'));

      const result = await client.writeStateToPod('silas', 'file.md', 'content');
      expect(result).toBe(false);
    });
  });

  describe('readStateFromPod', () => {
    it('reads a state file with GET from the correct path', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ token: 'jwt-123', expiresIn: 3600 }),
      });
      mockFetch.mockResolvedValueOnce({
        ok: true,
        text: async () => '# Current Work\n\nIn progress...',
      });

      const content = await client.readStateFromPod('kade', 'current-work.md');

      expect(content).toBe('# Current Work\n\nIn progress...');
      expect(mockFetch).toHaveBeenLastCalledWith(
        'http://localhost:3000/api/service/pods/jeff/coordination/state/kade/current-work.md',
        expect.objectContaining({
          method: 'GET',
          headers: expect.objectContaining({ 'Authorization': 'Bearer jwt-123' }),
        })
      );
    });

    it('returns null on 404', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ token: 'jwt-123', expiresIn: 3600 }),
      });
      mockFetch.mockResolvedValueOnce({ ok: false, status: 404, statusText: 'Not Found' });

      const content = await client.readStateFromPod('wren', 'missing.md');
      expect(content).toBeNull();
    });

    it('returns null for unknown role', async () => {
      const content = await client.readStateFromPod('unknown', 'file.md');
      expect(content).toBeNull();
    });

    it('returns null on network error', async () => {
      mockFetch.mockResolvedValueOnce({
        ok: true,
        json: async () => ({ token: 'jwt-123', expiresIn: 3600 }),
      });
      mockFetch.mockRejectedValueOnce(new Error('Connection refused'));

      const content = await client.readStateFromPod('silas', 'file.md');
      expect(content).toBeNull();
    });
  });
});
