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

  // #2474 — happy-path delegation coverage
  test('spawns shim with DEPLOY_ROLE=from + [nudge, to, msg] and returns success text', async () => {
    const calls: Array<{ file: string; args: string[]; env?: Record<string, string | undefined> }> = [];
    const mockExec = async (file: string, args: string[], opts: { env?: Record<string, string | undefined> }) => {
      calls.push({ file, args, env: opts.env });
      return { stdout: 'nudge queued for wren\n', stderr: '' };
    };

    const server = buildMcpServer(() => 'silas', {
      execFileAsync: mockExec,
      shimPath: '/test/shim/path',
    });
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/call');
    const result = await handler(
      {
        method: 'tools/call',
        params: {
          name: 'chorus_nudge_message',
          arguments: { to: 'wren', message: 'hi from #2474' },
        },
      },
      {},
    );
    expect(calls).toHaveLength(1);
    expect(calls[0].file).toBe('/test/shim/path');
    expect(calls[0].args).toEqual(['nudge', 'wren', 'hi from #2474']);
    expect(calls[0].env?.DEPLOY_ROLE).toBe('silas');
    expect(result.content[0].text).toMatch(/silas.*wren/);
  });

  test('catch-branch fires mcp.nudge.failed when shim exits non-zero', async () => {
    const stderrLines: string[] = [];
    const origWrite = process.stderr.write.bind(process.stderr);
    process.stderr.write = ((line: string) => { stderrLines.push(line); return true; }) as typeof process.stderr.write;
    try {
      const mockExec = async (): Promise<{ stdout: string; stderr: string }> => {
        throw new Error('shim binary missing');
      };

      const server = buildMcpServer(() => 'silas', {
        execFileAsync: mockExec,
        shimPath: '/does/not/exist',
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
