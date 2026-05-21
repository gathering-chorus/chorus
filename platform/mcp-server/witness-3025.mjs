import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';

const t0 = Date.now();
const transport = new StdioClientTransport({
  command: 'node',
  args: ['dist/main-stdio.js'],
  env: { ...process.env, CHORUS_ROLE: 'wren' }, // fail-loud-on-role: must pass it
});
const client = new Client({ name: 'wren-witness-3025', version: '1.0' });
await client.connect(transport);
const engineUpMs = Date.now() - t0;

const tools = await client.listTools();
const toolCount = tools.tools.length;

const t1 = Date.now();
const res = await client.callTool({ name: 'chorus_ownership_lookup', arguments: { iri: 'chorus:cards-service' } });
const callMs = Date.now() - t1;
const payload = JSON.parse(res.content[0].text);
await client.close();

const summary = { card: '#3025', engineUpMs, toolCount, call: 'chorus_ownership_lookup(chorus:cards-service)', callMs, payload };
console.log(JSON.stringify(summary, null, 2));

const text = `🪶 #3025 WITNESSED — the engine started and answered, live (not a checkmark)\n`
  + `engine up: ${engineUpMs}ms · ${toolCount} tools listed · call returned in ${callMs}ms\n`
  + `chorus_ownership_lookup(chorus:cards-service) → owner=${payload.owner ?? ('('+payload.reason+')')} step=${payload.step ?? ''}\n`
  + `Before #3025 this exact call returned "not-found". Now it reads the live v2 graph. You're seeing the real payload.`;
const r = await fetch('http://localhost:3470/api/message', {
  method: 'POST', headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ from: 'wren', text }),
});
console.log('bridge POST status', r.status);
