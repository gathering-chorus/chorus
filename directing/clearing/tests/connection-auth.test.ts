// @test-type: security
/**
 * #3669 — the WebSocket tunnel-bypass hole, as a regression test.
 *
 * Before the fix the Socket.IO gate classified locality by handshake.address
 * alone, so a tunneled WS (cloudflared → 127.0.0.1) was "local" and the bridge
 * token was skipped — an unauthenticated jeff-message command path over the
 * public tunnel. These tests pin the invariant: a tunneled connection is NEVER
 * local, regardless of source address.
 */

import { isTunneled, isLocalAddress, isLocalConnection } from '../src/connection-auth';

describe('#3669: one connection-locality classifier for both transports', () => {
  describe('isTunneled', () => {
    test('cf-connecting-ip marks a tunneled request', () => {
      expect(isTunneled({ 'cf-connecting-ip': '203.0.113.7' })).toBe(true);
    });
    test('cf-ray marks a tunneled request', () => {
      expect(isTunneled({ 'cf-ray': '8abc123-EWR' })).toBe(true);
    });
    test('no cf headers → not tunneled', () => {
      expect(isTunneled({ host: 'clearing.lightlifeurbangardens.com' })).toBe(false);
    });
  });

  describe('isLocalAddress', () => {
    test('loopback is local', () => {
      expect(isLocalAddress('127.0.0.1')).toBe(true);
      expect(isLocalAddress('::1')).toBe(true);
      expect(isLocalAddress('::ffff:127.0.0.1')).toBe(true);
    });
    test('home LAN is local', () => {
      expect(isLocalAddress('192.168.86.42')).toBe(true);
    });
    test('a public address is not local', () => {
      expect(isLocalAddress('203.0.113.7')).toBe(false);
    });
    test('empty address is not local', () => {
      expect(isLocalAddress('')).toBe(false);
    });
  });

  describe('isLocalConnection — the fix', () => {
    test('THE HOLE: tunneled WS from cloudflared 127.0.0.1 is NOT local', () => {
      // This is the exact 2026-07-23 hole: cloudflared originates from loopback,
      // but the cf headers ride the WS upgrade. Pre-fix the socket gate saw only
      // 127.0.0.1 and skipped the token. Must now require the token (= not local).
      const headers = { 'cf-connecting-ip': '203.0.113.7', 'cf-ray': '8abc-EWR' };
      expect(isLocalConnection(headers, '127.0.0.1')).toBe(false);
    });

    test('genuine loopback (no tunnel) stays local', () => {
      expect(isLocalConnection({}, '127.0.0.1')).toBe(true);
    });

    test('genuine home LAN (no tunnel) stays local', () => {
      expect(isLocalConnection({ host: 'jeffs-mac-mini-m1-3.local' }, '192.168.86.42')).toBe(true);
    });

    test('remote non-tunneled address is not local', () => {
      expect(isLocalConnection({}, '203.0.113.7')).toBe(false);
    });

    test('tunneled request from a LAN address is still not local', () => {
      // Defense in depth: a tunnel header wins even over a LAN source.
      expect(isLocalConnection({ 'cf-ray': 'x' }, '192.168.86.42')).toBe(false);
    });
  });
});
