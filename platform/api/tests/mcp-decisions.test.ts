/**
 * #2485 Move 5 — MCP decisions tool unit tests.
 * Mirrors mcp-principles.test.ts.
 */
import { buildMcpServer } from '../src/mcp/server';

type AnyHandler = (req: unknown, ctx: unknown) => Promise<unknown>;

function getHandler(server: ReturnType<typeof buildMcpServer>, name: string): AnyHandler {
  const map = (server as unknown as { _requestHandlers: Map<string, AnyHandler> })._requestHandlers;
  const h = map.get(name);
  if (!h) throw new Error(`handler not found: ${name}`);
  return h;
}

describe('#2485 buildMcpServer decisions tools', () => {
  test('tools/list exposes both decisions tools alongside principles + nudge', async () => {
    const server = buildMcpServer(() => 'silas');
    const handler = getHandler(server, 'tools/list');
    const result = (await handler({ method: 'tools/list', params: {} }, {})) as { tools: { name: string }[] };
    const names = result.tools.map((t) => t.name).sort();
    expect(names).toEqual([
      'chorus_decisions_get',
      'chorus_decisions_list',
      'chorus_nudge_message',
      'chorus_principles_create',
      'chorus_principles_get',
      'chorus_principles_list',
      'chorus_subdomains_get',
      'chorus_subdomains_list',
    ]);
  });

  test('chorus_decisions_list delegates fetch to /api/loom/decisions', async () => {
    const fetchMock = jest.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ data: { decisions: [{ id: 'adr-026', label: 'CI architecture' }] } }),
    });
    const server = buildMcpServer(() => 'silas', { fetchImpl: fetchMock });
    const handler = getHandler(server, 'tools/call');
    const result = (await handler({ method: 'tools/call', params: { name: 'chorus_decisions_list', arguments: {} } }, {})) as { content: { text: string }[] };
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][0]).toContain('/api/athena/subdomains/loom-decisions/decisions');
    expect(result.content[0].text).toContain('adr-026');
  });

  test('chorus_decisions_get returns label + comment for a known id', async () => {
    const fetchMock = jest.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ data: { decisions: [
        { id: 'adr-026', label: 'CI architecture', comment: 'Three quality layers.' },
        { id: 'dec-2090', label: 'Demo briefs', comment: 'Drop files for single-card demos.' },
      ] } }),
    });
    const server = buildMcpServer(() => 'silas', { fetchImpl: fetchMock });
    const handler = getHandler(server, 'tools/call');
    const result = (await handler({ method: 'tools/call', params: { name: 'chorus_decisions_get', arguments: { id: 'adr-026' } } }, {})) as { content: { text: string }[] };
    expect(result.content[0].text).toContain('CI architecture');
    expect(result.content[0].text).toContain('Three quality layers');
    expect(result.content[0].text).not.toContain('Demo briefs');
  });

  test('chorus_decisions_get throws for an unknown id', async () => {
    const fetchMock = jest.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ data: { decisions: [{ id: 'adr-026', label: 'CI' }] } }),
    });
    const server = buildMcpServer(() => 'silas', { fetchImpl: fetchMock });
    const handler = getHandler(server, 'tools/call');
    await expect(
      handler({ method: 'tools/call', params: { name: 'chorus_decisions_get', arguments: { id: 'adr-999' } } }, {}),
    ).rejects.toThrow(/not found/i);
  });
});
