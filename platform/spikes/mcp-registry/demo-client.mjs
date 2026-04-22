#!/usr/bin/env node
/**
 * Chorus MCP Registry — SPIKE demo client
 *
 * Spawns the server over stdio, runs the anti-hacking JX demo:
 *   1. LIST resources  → proves discovery (no URL guessing)
 *   2. READ the principles resource → gets current principles via MCP, not curl
 *   3. LIST tools      → proves tool discovery
 *   4. CALL enumerate_skills → gets all non-utility skills from graph
 *   5. CALL invoke_skill_pull (dry-run) → proves tool invocation contract
 *
 * Output is human-readable so a reviewer can see the "no hacking needed" story.
 */

import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));

const transport = new StdioClientTransport({
  command: "node",
  args: [join(here, "server.mjs")],
});

const client = new Client(
  { name: "chorus-mcp-demo-client", version: "0.0.1" },
  { capabilities: {} },
);

const sep = (label) => {
  console.log(`\n${"=".repeat(72)}`);
  console.log(`  ${label}`);
  console.log("=".repeat(72));
};

try {
  await client.connect(transport);
  sep("Connected to Chorus MCP registry (spike)");

  // --- 1. LIST resources ---
  sep("1. listResources — what can I read?");
  const resources = await client.listResources();
  for (const r of resources.resources) {
    console.log(`  ${r.uri}  — ${r.name}`);
    console.log(`    ${r.description}`);
  }

  // --- 2. READ principles ---
  sep("2. readResource chorus://principles — pull current principles");
  const read = await client.readResource({ uri: "chorus://principles" });
  const body = read.contents?.[0]?.text ?? "(empty)";
  const parsed = JSON.parse(body);
  const principles = parsed?.data?.principles ?? [];
  console.log(`  count: ${principles.length}`);
  for (const p of principles.slice(0, 5)) {
    console.log(`  - ${p.label ?? p.id ?? "(unlabeled)"}`);
  }
  if (principles.length > 5) console.log(`  ... and ${principles.length - 5} more`);

  // --- 3. LIST tools ---
  sep("3. listTools — what can I invoke?");
  const tools = await client.listTools();
  for (const t of tools.tools) {
    console.log(`  ${t.name}`);
    console.log(`    ${t.description.split("\n")[0]}`);
  }

  // --- 4. CALL enumerate_skills ---
  sep("4. callTool enumerate_skills — find chorus:Skill without grepping");
  const enumerated = await client.callTool({
    name: "enumerate_skills",
    arguments: { include_utility: false },
  });
  const skillBody = enumerated.content?.[0]?.text ?? "{}";
  const skillData = JSON.parse(skillBody);
  console.log(`  count: ${skillData.count}`);
  for (const s of skillData.skills.slice(0, 8)) {
    console.log(`  - ${s.label}  →  ${s.implementedIn ?? "(no impl path)"}`);
  }
  if (skillData.skills.length > 8) console.log(`  ... and ${skillData.skills.length - 8} more`);

  // --- 5. CALL invoke_skill_pull (dry-run) ---
  sep("5. callTool invoke_skill_pull — contract check (dry-run)");
  const pulled = await client.callTool({
    name: "invoke_skill_pull",
    arguments: { card_id: 9999, role: "wren" },
  });
  console.log(pulled.content?.[0]?.text);

  sep("DONE — discovery + read + tool-call succeeded via MCP, no URL guessing");
} catch (err) {
  console.error("DEMO FAILED:", err.message);
  process.exitCode = 1;
} finally {
  await client.close();
}
