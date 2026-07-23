// @test-type: unit — gateDecision is a pure decision table; no server, no fs.
/**
 * #3667 — Domains/Streams tabs empty over the public tunnel.
 *
 * Jeff's experience under test: on cellular, the Streams tab (/api/stream)
 * and Domains tab (/api/flow) load with his login token; the admin trio
 * (/api/restart, /api/commands/, /api/session/) stays hard local-only no
 * matter what token a remote request carries; domain detail arrives through
 * a relative proxy route, never a browser-side localhost:3340 fetch.
 */

import { gateDecision } from '../src/server-auth';

describe('gateDecision — tunnel auth decision table (#3667)', () => {
  test('remote + valid token: GET /api/stream and /api/flow pass', () => {
    expect(gateDecision('/api/stream', 'GET', false, true)).toBe('pass');
    expect(gateDecision('/api/flow', 'GET', false, true)).toBe('pass');
  });

  test('remote + valid token: domain-detail proxy passes', () => {
    expect(gateDecision('/api/domain-detail/chorus', 'GET', false, true)).toBe('pass');
  });

  test('remote without token: read pair requires auth, not hard-forbid', () => {
    expect(gateDecision('/api/stream', 'GET', false, false)).toBe('auth-required');
    expect(gateDecision('/api/flow', 'GET', false, false)).toBe('auth-required');
  });

  test('admin trio stays hard local-only even with a valid token', () => {
    expect(gateDecision('/api/restart', 'GET', false, true)).toBe('forbid');
    expect(gateDecision('/api/commands/kade', 'GET', false, true)).toBe('forbid');
    expect(gateDecision('/api/session/wren', 'GET', false, true)).toBe('forbid');
  });

  test('only GET is opened on the read pair — writes stay forbidden remotely', () => {
    expect(gateDecision('/api/stream', 'POST', false, true)).toBe('forbid');
    expect(gateDecision('/api/flow', 'POST', false, true)).toBe('forbid');
  });

  test('local requests pass everything, unchanged', () => {
    for (const p of ['/api/stream', '/api/flow', '/api/restart', '/api/commands/kade', '/api/session/wren', '/health', '/metrics', '/api/debug']) {
      expect(gateDecision(p, 'GET', true, false)).toBe('pass');
    }
  });

  test('local-only paths still forbid remote regardless of token', () => {
    for (const p of ['/health', '/metrics', '/api/debug']) {
      expect(gateDecision(p, 'GET', false, true)).toBe('forbid');
    }
  });
});
