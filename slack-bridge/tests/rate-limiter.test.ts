import { RateLimiter } from '../src/rate-limiter';

// Mock metrics to avoid prom-client initialization issues in tests
jest.mock('../src/metrics', () => ({
  metrics: {
    rateLimited: { inc: jest.fn() },
  },
}));

describe('RateLimiter', () => {
  let limiter: RateLimiter;

  beforeEach(() => {
    limiter = new RateLimiter(3, 5, 100); // 3/role/hr, 5 global/hr, 100ms debounce
  });

  describe('role limits', () => {
    it('allows requests within limit', () => {
      expect(limiter.canProceed('silas', 'C1').allowed).toBe(true);
      limiter.record('silas', 'C1');
      expect(limiter.canProceed('silas', 'C2').allowed).toBe(true);
      limiter.record('silas', 'C2');
      expect(limiter.canProceed('silas', 'C3').allowed).toBe(true);
      limiter.record('silas', 'C3');
    });

    it('blocks when role limit exceeded', () => {
      for (let i = 0; i < 3; i++) {
        limiter.record('silas', `C${i}`);
      }
      const result = limiter.canProceed('silas', 'C99');
      expect(result.allowed).toBe(false);
      expect(result.reason).toBe('role_limit');
    });

    it('notifies rate limit only once', () => {
      for (let i = 0; i < 3; i++) limiter.record('silas', `C${i}`);

      const first = limiter.canProceed('silas', 'C99');
      expect(first.notifyRateLimit).toBe(true);

      const second = limiter.canProceed('silas', 'C99');
      expect(second.notifyRateLimit).toBe(false);
    });

    it('different roles have independent limits', () => {
      for (let i = 0; i < 3; i++) limiter.record('silas', `C${i}`);
      expect(limiter.canProceed('silas', 'C99').allowed).toBe(false);
      expect(limiter.canProceed('wren', 'C99').allowed).toBe(true);
    });
  });

  describe('global limits', () => {
    it('blocks when global limit exceeded', () => {
      limiter.record('silas', 'C1');
      limiter.record('silas', 'C2');
      limiter.record('silas', 'C3');
      limiter.record('wren', 'C4');
      limiter.record('wren', 'C5');

      const result = limiter.canProceed('kade', 'C99');
      expect(result.allowed).toBe(false);
      expect(result.reason).toBe('global_limit');
    });
  });

  describe('channel debounce', () => {
    it('blocks rapid messages to same channel', () => {
      limiter.record('silas', 'C1');
      const result = limiter.canProceed('silas', 'C1');
      expect(result.allowed).toBe(false);
      expect(result.reason).toBe('channel_debounce');
    });

    it('allows after debounce window passes', async () => {
      limiter.record('silas', 'C1');
      await new Promise(resolve => setTimeout(resolve, 150));
      expect(limiter.canProceed('silas', 'C1').allowed).toBe(true);
    });
  });
});
