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
    // #2652 — names list is no longer brittle; just assert these exist.
    expect(names).toContain('chorus_decisions_get');
    expect(names).toContain('chorus_decisions_list');
  });

  // Fixtures match the real shape returned by GET
  // /api/athena/subdomains/loom-decisions/decisions (handlers/loom-decisions.ts):
  // `id` carries rdfs:label, `title` carries rdfs:comment, `uri` is canonical
  // identity. The earlier {id: 'adr-026', label, comment} fixture diverged from
  // the API and masked the body-projection bug fixed in #2716.
  test('chorus_decisions_list delegates fetch to /api/loom/decisions', async () => {
    const fetchMock = jest.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ data: { decisions: [
        { uri: 'https://jeffbridwell.com/chorus#adr-026', id: 'CI architecture', title: 'Three quality layers.', decisionType: 'ADR' },
      ] } }),
    });
    const server = buildMcpServer(() => 'silas', { fetchImpl: fetchMock });
    const handler = getHandler(server, 'tools/call');
    const result = (await handler({ method: 'tools/call', params: { name: 'chorus_decisions_list', arguments: {} } }, {})) as { content: { text: string }[] };
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][0]).toContain('/api/athena/subdomains/loom-decisions/decisions');
    expect(result.content[0].text).toContain('CI architecture');
    expect(result.content[0].text).toContain('adr-026');
  });

  test('chorus_decisions_get matches by slug and returns body from title field', async () => {
    const fetchMock = jest.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ data: { decisions: [
        { uri: 'https://jeffbridwell.com/chorus#adr-026', id: 'CI architecture', title: 'Three quality layers.', decisionType: 'ADR' },
        { uri: 'https://jeffbridwell.com/chorus#dec-2090', id: 'Demo briefs', title: 'Drop files for single-card demos.', decisionType: 'DEC' },
      ] } }),
    });
    const server = buildMcpServer(() => 'silas', { fetchImpl: fetchMock });
    const handler = getHandler(server, 'tools/call');
    const result = (await handler({ method: 'tools/call', params: { name: 'chorus_decisions_get', arguments: { id: 'adr-026' } } }, {})) as { content: { text: string }[] };
    expect(result.content[0].text).toContain('CI architecture');
    expect(result.content[0].text).toContain('Three quality layers');
    expect(result.content[0].text).not.toContain('Demo briefs');
  });

  test('chorus_decisions_get matches case-insensitively (ADR-026 == adr-026)', async () => {
    const fetchMock = jest.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ data: { decisions: [
        { uri: 'https://jeffbridwell.com/chorus#adr-026', id: 'CI architecture', title: 'Three quality layers.', decisionType: 'ADR' },
      ] } }),
    });
    const server = buildMcpServer(() => 'silas', { fetchImpl: fetchMock });
    const handler = getHandler(server, 'tools/call');
    const result = (await handler({ method: 'tools/call', params: { name: 'chorus_decisions_get', arguments: { id: 'ADR-026' } } }, {})) as { content: { text: string }[] };
    expect(result.content[0].text).toContain('Three quality layers');
  });

  test('chorus_decisions_get throws for an unknown id', async () => {
    const fetchMock = jest.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ data: { decisions: [
        { uri: 'https://jeffbridwell.com/chorus#adr-026', id: 'CI', title: 'x' },
      ] } }),
    });
    const server = buildMcpServer(() => 'silas', { fetchImpl: fetchMock });
    const handler = getHandler(server, 'tools/call');
    await expect(
      handler({ method: 'tools/call', params: { name: 'chorus_decisions_get', arguments: { id: 'adr-999' } } }, {}),
    ).rejects.toThrow(/not found/i);
  });
});
