/**
 * #2472 — MCP nudge tool unit tests.
 *
 * Tests the buildMcpServer function in isolation: tool registration shape,
 * input validation, and delegation behavior. End-to-end transport behavior
 * is covered by platform/tests/mcp-nudge.test.sh which runs against the live
 * chorus-api process.
 */
import { buildMcpServer } from '../src/mcp/server';
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from '@modelcontextprotocol/sdk/types.js';

describe('#2472 buildMcpServer', () => {
  test('exposes chorus_nudge_message in tools/list', async () => {
    const server = buildMcpServer(() => 'silas');
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/list');
    expect(handler).toBeDefined();
    const result = await handler({ method: 'tools/list', params: {} }, {});
    expect(result.tools).toBeDefined();
    expect(result.tools.length).toBeGreaterThanOrEqual(1);
    const nudge = result.tools.find((t: any) => t.name === 'chorus_nudge_message');
    expect(nudge).toBeDefined();
    expect(nudge?.description).toContain('Chorus role');
    expect(nudge?.inputSchema).toBeDefined();
    expect(nudge?.inputSchema.properties.to.enum).toEqual(['silas', 'wren', 'kade', 'jeff']);
    expect(nudge?.inputSchema.required).toContain('to');
    expect(nudge?.inputSchema.required).toContain('message');
  });

  test('rejects unknown tool name', async () => {
    const server = buildMcpServer(() => 'silas');
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/call');
    expect(handler).toBeDefined();
    await expect(
      handler(
        { method: 'tools/call', params: { name: 'not_a_tool', arguments: {} } },
        {},
      ),
    ).rejects.toThrow(/Unknown tool/);
  });

  test('rejects invalid arguments shape', async () => {
    const server = buildMcpServer(() => 'silas');
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/call');
    await expect(
      handler(
        {
          method: 'tools/call',
          params: { name: 'chorus_nudge_message', arguments: { to: 'bob', message: 'hi' } },
        },
        {},
      ),
    ).rejects.toThrow(/Invalid arguments/);
  });

  test('rejects empty message', async () => {
    const server = buildMcpServer(() => 'silas');
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/call');
    await expect(
      handler(
        {
          method: 'tools/call',
          params: { name: 'chorus_nudge_message', arguments: { to: 'wren', message: '' } },
        },
        {},
      ),
    ).rejects.toThrow(/Invalid arguments/);
  });

  // #2804 — MCP is the canonical invocation path. executeNudge no longer
  // spawns chorus-hook-shim; it POSTs to pulse. Old test (shim-spawn) retired
  // with the bash + shim path; new tests verify POST shape + headers.
  test('#2804 happy-path: POST to pulse with X-Chorus-MCP-Caller + X-Chorus-Trace-Id headers', async () => {
    const calls: Array<{ url: string; init?: { method?: string; headers?: Record<string, string>; body?: string } }> = [];
    const mockFetch = async (url: string, init?: { method?: string; headers?: Record<string, string>; body?: string }) => {
      calls.push({ url, init });
      return { ok: true, status: 200, json: async () => ({ ok: true, id: 1 }), text: async () => '' };
    };

    const server = buildMcpServer(() => 'silas', {
      fetchImpl: mockFetch as never,
    });
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/call');
    const result = await handler(
      {
        method: 'tools/call',
        params: {
          name: 'chorus_nudge_message',
          arguments: { to: 'wren', message: 'hi from #2804' },
        },
      },
      {},
    );
    expect(calls).toHaveLength(1);
    expect(calls[0].url).toMatch(/\/api\/nudge$/);
    expect(calls[0].init?.method).toBe('POST');
    expect(calls[0].init?.headers?.['X-Chorus-MCP-Caller']).toBe('1');
    expect(calls[0].init?.headers?.['X-Chorus-Trace-Id']).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[0-9a-f]{4}-[0-9a-f]{12}$/);
    const body = JSON.parse(calls[0].init?.body || '{}');
    expect(body.from).toBe('silas');
    expect(body.to).toBe('wren');
    expect(body.content).toBe('hi from #2804');
    expect(body.traceId).toMatch(/^[0-9a-f]{8}-/);
    expect(result.content[0].text).toMatch(/silas.*wren.*trace=/);
  });

  // #2814 — permutations: network error, recipient=jeff, dual-emit verification.
  test('#2814 fetch throws (pulse unreachable) → mcp.nudge.failed + thrown error', async () => {
    const stderrLines: string[] = [];
    const origWrite = process.stderr.write.bind(process.stderr);
    process.stderr.write = ((line: string) => { stderrLines.push(line); return true; }) as typeof process.stderr.write;
    try {
      const mockFetch = async () => { throw new Error('ECONNREFUSED'); };
      const server = buildMcpServer(() => 'silas', { fetchImpl: mockFetch as never });
      // @ts-expect-error - private handler access for unit test
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler(
          { method: 'tools/call', params: { name: 'chorus_nudge_message', arguments: { to: 'wren', message: 'hi' } } },
          {},
        ),
      ).rejects.toThrow(/nudge delivery failed.*ECONNREFUSED/);
      expect(stderrLines.some((l) => l.includes('mcp.nudge.failed'))).toBe(true);
    } finally {
      process.stderr.write = origWrite;
    }
  });

  test('#2814 recipient=jeff is accepted (schema enum includes jeff)', async () => {
    const calls: Array<{ url: string; init?: { body?: string } }> = [];
    const mockFetch = async (url: string, init?: { body?: string }) => {
      calls.push({ url, init });
      return { ok: true, status: 200, json: async () => ({ ok: true, id: 1 }), text: async () => '' };
    };
    const server = buildMcpServer(() => 'silas', { fetchImpl: mockFetch as never });
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/call');
    const result = await handler(
      { method: 'tools/call', params: { name: 'chorus_nudge_message', arguments: { to: 'jeff', message: 'human in the loop' } } },
      {},
    );
    expect(calls).toHaveLength(1);
    const body = JSON.parse(calls[0].init?.body || '{}');
    expect(body.to).toBe('jeff');
    expect(result.content[0].text).toMatch(/silas → jeff/);
  });

  test('#2804 catch-branch: pulse POST non-2xx → mcp.nudge.failed + thrown error', async () => {
    const stderrLines: string[] = [];
    const origWrite = process.stderr.write.bind(process.stderr);
    process.stderr.write = ((line: string) => { stderrLines.push(line); return true; }) as typeof process.stderr.write;
    try {
      const mockFetch = async () => {
        return { ok: false, status: 503, json: async () => ({}), text: async () => 'pulse offline' };
      };

      const server = buildMcpServer(() => 'silas', {
        fetchImpl: mockFetch as never,
      });
      // @ts-expect-error - private handler access for unit test
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler(
          {
            method: 'tools/call',
            params: { name: 'chorus_nudge_message', arguments: { to: 'wren', message: 'hi' } },
          },
          {},
        ),
      ).rejects.toThrow(/nudge delivery failed/);
      expect(stderrLines.some((l) => l.includes('mcp.nudge.failed'))).toBe(true);
    } finally {
      process.stderr.write = origWrite;
    }
  });
});
