// #3010 — chorus_principles_list returns structured JSON content alongside
// the existing prose text. Closes the parse_tool_text fragility at the MCP
// boundary: client reads structuredContent.principles directly, no greedy
// rfind('(') against the prose string.
//
// AC1: structuredContent.principles is an array of {id, label, comment}.
// AC3: For a principle whose comment contains parens (the canonical Hemenway
//      catch-and-store shape), structuredContent.principles[i].id is the
//      real id, NOT a comment fragment.

import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { buildMcpServer, type FetchImpl } from '../src/server';

// Canonical Hemenway principle shape — comment contains nested parens.
// Direct receipt from roles/silas/ontology/chorus.ttl.
const HEMENWAY_COMMENT =
  'Identify, collect, and hold useful flows. Every cycle is an opportunity for yield; ' +
  'every gradient (in slope, charge, temperature, or otherwise) is an opportunity for energy.';

function stubFetchPrinciples(): FetchImpl {
  return async (_url: string) =>
    ({
      ok: true,
      status: 200,
      json: async () => ({
        data: {
          principles: [
            {
              id: 'hemenway-catch-and-store',
              label: 'Catch and store energy and materials',
              comment: HEMENWAY_COMMENT,
            },
            {
              id: 'principle-simple',
              label: 'Simple principle',
              comment: 'No parens here.',
            },
          ],
        },
      }),
    } as unknown as Awaited<ReturnType<FetchImpl>>);
}

test('chorus_principles_list returns structuredContent.principles (#3010 AC1)', async () => {
  const server = buildMcpServer(() => 'wren', { fetchImpl: stubFetchPrinciples() });
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: 'principles-structured-test', version: '1.0' });
  await Promise.all([server.connect(serverTransport), client.connect(clientTransport)]);

  try {
    const result = (await client.callTool({
      name: 'chorus_principles_list',
      arguments: {},
    })) as { content: Array<{ type: string; text: string }>; structuredContent?: { principles?: unknown } };

    assert.ok(
      result.structuredContent !== undefined && result.structuredContent !== null,
      'expected structuredContent on chorus_principles_list response',
    );
    const sc = result.structuredContent as {
      principles?: Array<{ id: string; label?: string; comment?: string }>;
    };
    assert.ok(Array.isArray(sc.principles), 'structuredContent.principles must be an array');
    assert.equal(sc.principles!.length, 2, 'expected 2 principles in the fixture');

    // AC3: Hemenway id is intact, not fragmented to "in slope, charge, temperature, or otherwise".
    const hemenway = sc.principles!.find((p) => p.label === 'Catch and store energy and materials');
    assert.ok(hemenway, 'Hemenway principle should be present');
    assert.equal(
      hemenway!.id,
      'hemenway-catch-and-store',
      `Hemenway id should be 'hemenway-catch-and-store', got '${hemenway!.id}' (parse-fragment bug)`,
    );
    assert.equal(hemenway!.comment, HEMENWAY_COMMENT, 'comment should round-trip unchanged');
  } finally {
    await client.close();
    await server.close();
  }
});
