/**
 * #2652 (AC8) — MCP cards tools unit tests.
 *
 * Tests buildMcpServer for the six cards verbs added: add, move, done, tag, set, view.
 * Mirrors mcp-nudge.test.ts shape: tool registration + input validation + happy-path
 * delegation with mock execFile. End-to-end behavior covered by manual MCP probe.
 */
import { buildMcpServer } from '../src/mcp/server';

const VERBS = ['add', 'move', 'done', 'tag', 'set', 'view'] as const;

describe('#2652 cards MCP tools', () => {
  test('exposes chorus_cards_<verb> for all six verbs', async () => {
    const server = buildMcpServer(() => 'wren');
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/list');
    const result = await handler({ method: 'tools/list', params: {} }, {});
    const names = result.tools.map((t: { name: string }) => t.name);
    for (const verb of VERBS) {
      expect(names).toContain(`chorus_cards_${verb}`);
    }
  });

  test('chorus_cards_view rejects missing id', async () => {
    const server = buildMcpServer(() => 'wren');
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/call');
    await expect(
      handler(
        { method: 'tools/call', params: { name: 'chorus_cards_view', arguments: {} } },
        {},
      ),
    ).rejects.toThrow(/Invalid arguments/);
  });

  test('chorus_cards_move rejects unknown status', async () => {
    const server = buildMcpServer(() => 'wren');
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/call');
    await expect(
      handler(
        {
          method: 'tools/call',
          params: { name: 'chorus_cards_move', arguments: { id: 1, status: 'BOGUS' } },
        },
        {},
      ),
    ).rejects.toThrow(/Invalid arguments/);
  });

  test('chorus_cards_add spawns cards bin with DEPLOY_ROLE injected', async () => {
    const calls: Array<{ file: string; args: string[]; env?: Record<string, string | undefined> }> = [];
    const mockExec = async (
      file: string,
      args: string[],
      opts: { env?: Record<string, string | undefined> },
    ) => {
      calls.push({ file, args, env: opts.env });
      return { stdout: 'Added #9999: test card', stderr: '' };
    };
    const server = buildMcpServer(() => 'kade', {
      execFileAsync: mockExec as never,
      cardsPath: '/fake/cards',
    });
    // @ts-expect-error - private handler access
    const handler = (server as any)._requestHandlers.get('tools/call');
    const res = await handler(
      {
        method: 'tools/call',
        params: {
          name: 'chorus_cards_add',
          arguments: {
            title: 'test card',
            owner: 'kade',
            priority: 'P3',
            domain: 'chorus',
            type: 'fix',
            origin: 'reactive',
            desc: 'minimal',
          },
        },
      },
      {},
    );
    expect(calls).toHaveLength(1);
    expect(calls[0].file).toBe('/fake/cards');
    expect(calls[0].args[0]).toBe('add');
    expect(calls[0].env?.DEPLOY_ROLE).toBe('kade');
    expect(res.content[0].text).toContain('Added #9999');
  });

  test('chorus_cards_view spawns with --json and view verb', async () => {
    const calls: Array<{ file: string; args: string[]; env?: Record<string, string | undefined> }> = [];
    const mockExec = async (
      file: string,
      args: string[],
      opts: { env?: Record<string, string | undefined> },
    ) => {
      calls.push({ file, args, env: opts.env });
      return { stdout: '{"index":1234,"title":"x"}', stderr: '' };
    };
    const server = buildMcpServer(() => 'wren', {
      execFileAsync: mockExec as never,
      cardsPath: '/fake/cards',
    });
    // @ts-expect-error - private handler access
    const handler = (server as any)._requestHandlers.get('tools/call');
    const res = await handler(
      { method: 'tools/call', params: { name: 'chorus_cards_view', arguments: { id: 1234 } } },
      {},
    );
    expect(calls[0].args).toEqual(['view', '1234', '--json']);
    expect(res.content[0].text).toContain('1234');
  });

  test('chorus_cards_tag includes id + category:value', async () => {
    const calls: Array<{ file: string; args: string[] }> = [];
    const mockExec = async (
      file: string,
      args: string[],
      _opts: { env?: Record<string, string | undefined> },
    ) => {
      calls.push({ file, args });
      return { stdout: 'Tagged', stderr: '' };
    };
    const server = buildMcpServer(() => 'wren', {
      execFileAsync: mockExec as never,
      cardsPath: '/fake/cards',
    });
    // @ts-expect-error - private handler access
    const handler = (server as any)._requestHandlers.get('tools/call');
    await handler(
      {
        method: 'tools/call',
        params: {
          name: 'chorus_cards_tag',
          arguments: { id: 42, category: 'sequence', value: 'werk', op: 'add' },
        },
      },
      {},
    );
    expect(calls[0].args[0]).toBe('sequence-tag');
    expect(calls[0].args).toContain('42');
    expect(calls[0].args).toContain('werk');
  });
});
