// @test-type: unit — sendCardApprovalNudge with the pulse messaging API stubbed;
// no live socket/session. Pure SDK-logic assertion on the delivered payload.
/**
 * sendCardApprovalNudge — #2924 AC1.
 *
 * The bouncer must deliver the [card-approval] block into the requesting
 * agent's session via the pulse messaging API. Pickup file remains as the
 * fallback surface but is no longer the only path.
 *
 * Tests exercise the POST shape (URL, headers, body), success path,
 * non-OK response, and thrown-fetch path. Pulse itself is not started in
 * these tests — fetch is mocked.
 */

import { sendCardApprovalNudge } from '../src/sdk';

describe('sendCardApprovalNudge (#2924 AC1)', () => {
  test('POSTs to pulse with from/to/content/traceId and required headers', async () => {
    const captured: Array<{
      url: string;
      method?: string;
      headers?: Record<string, string>;
      body?: string;
    }> = [];
    const fakeFetch: Parameters<typeof sendCardApprovalNudge>[0]['fetchImpl'] =
      async (url, init) => {
        captured.push({
          url,
          method: init?.method,
          headers: init?.headers,
          body: init?.body,
        });
        return { ok: true, status: 202, text: async () => '' };
      };

    const message = '[card-approval] wren → jeff\n\nI need you to approve a card.';
    const result = await sendCardApprovalNudge({
      from: 'wren',
      to: 'wren',
      message,
      pulseUrl: 'http://test-pulse/api/nudge',
      fetchImpl: fakeFetch,
    });

    expect(result.delivered).toBe(true);
    expect(result.status).toBe(202);
    expect(typeof result.traceId).toBe('string');
    expect(captured).toHaveLength(1);
    expect(captured[0].url).toBe('http://test-pulse/api/nudge');
    expect(captured[0].method).toBe('POST');
    expect(captured[0].headers?.['Content-Type']).toBe('application/json');
    expect(captured[0].headers?.['X-Chorus-MCP-Caller']).toBe('1');
    expect(captured[0].headers?.['X-Chorus-Trace-Id']).toBe(result.traceId);

    const body = JSON.parse(captured[0].body || '{}');
    expect(body.from).toBe('wren');
    expect(body.to).toBe('wren');
    expect(body.content).toBe(message);
    expect(body.traceId).toBe(result.traceId);
  });

  test('returns delivered:false when pulse returns non-OK', async () => {
    const fakeFetch: Parameters<typeof sendCardApprovalNudge>[0]['fetchImpl'] =
      async () => ({ ok: false, status: 503, text: async () => 'pulse down' });

    const result = await sendCardApprovalNudge({
      from: 'wren',
      to: 'wren',
      message: 'm',
      fetchImpl: fakeFetch,
    });

    expect(result.delivered).toBe(false);
    expect(result.status).toBe(503);
    expect(result.error).toContain('pulse down');
  });

  test('returns delivered:false when fetch throws', async () => {
    const fakeFetch: Parameters<typeof sendCardApprovalNudge>[0]['fetchImpl'] =
      async () => { throw new Error('econnrefused'); };

    const result = await sendCardApprovalNudge({
      from: 'wren',
      to: 'wren',
      message: 'm',
      fetchImpl: fakeFetch,
    });

    expect(result.delivered).toBe(false);
    expect(result.error).toContain('econnrefused');
  });

  test('returns delivered:false with no-fetch-impl when fetch unavailable', async () => {
    const origFetch = (globalThis as any).fetch;
    (globalThis as any).fetch = undefined;
    try {
      const result = await sendCardApprovalNudge({
        from: 'wren',
        to: 'wren',
        message: 'm',
      });
      expect(result.delivered).toBe(false);
      expect(result.error).toBe('no-fetch-impl');
    } finally {
      (globalThis as any).fetch = origFetch;
    }
  });
});
