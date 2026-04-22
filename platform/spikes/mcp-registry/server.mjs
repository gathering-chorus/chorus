#!/usr/bin/env node
/**
 * Chorus MCP Registry — SPIKE server
 *
 * Card: #2452
 * Goal: prove that MCP can replace URL-guessing/source-grepping as the
 * discovery mechanism for Chorus capabilities.
 *
 * Exposes 3 capabilities:
 *   - Resource  chorus://principles           → GET /api/loom/principles (read-only)
 *   - Tool      enumerate_skills              → SPARQL query for chorus:Skill instances
 *   - Tool      invoke_skill_pull             → stub return (schema validation only)
 *
 * Transport: stdio (simplest; the MCP inspector or a stdio client can talk to it).
 * Not production. Do not wire into live session boot.
 */

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListResourcesRequestSchema,
  ListToolsRequestSchema,
  ReadResourceRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import { generateRegistry } from "./generator.mjs";

const CHORUS_API = process.env.CHORUS_API_BASE ?? "http://localhost:3340";
const FUSEKI = process.env.FUSEKI_BASE ?? "http://localhost:3030";

// --- Capability definitions (static for spike; generation-from-graph is the next card) ---

const RESOURCES = [
  {
    uri: "chorus://principles",
    name: "Loom Principles",
    description:
      "Current team principles, served from the graph via /api/loom/principles. " +
      "Read this instead of paraphrasing from CLAUDE.md.",
    mimeType: "application/json",
  },
];

// Static tools kept as hard-coded for now (enumerate_skills + invoke_skill_pull).
// The generated tools from chorus:Skill + chorus:Gate instances are merged in at startup.
const STATIC_TOOLS = [
  {
    name: "enumerate_skills",
    description:
      "List every chorus:Skill instance declared in the authorization graph. " +
      "Returns { skills: [{id, label, implementedIn, description}] }. " +
      "Use this instead of grepping ~/.claude/skills/ or guessing skill names.",
    inputSchema: {
      type: "object",
      properties: {
        include_utility: {
          type: "boolean",
          description:
            "If true, also includes chorus:UtilitySkill instances (observation/utility skills). Default false.",
          default: false,
        },
      },
    },
  },
  {
    name: "invoke_skill_pull",
    description:
      "Pull a card to WIP (equivalent to /pull <card-id>). SPIKE: returns a dry-run preview, " +
      "does not actually move the card. Real invocation would delegate to chorus-hook-shim.",
    inputSchema: {
      type: "object",
      properties: {
        card_id: {
          type: "integer",
          description: "Card ID to pull (e.g., 2452)",
        },
        role: {
          type: "string",
          enum: ["wren", "silas", "kade"],
          description: "Role pulling the card",
        },
      },
      required: ["card_id", "role"],
    },
  },
];

// Generated tools are populated at startup from the authorization graph.
// In production this would be an event-bus cache invalidated on /api/athena/reload
// (per Silas's #2452 feedback). For the spike: generate once, keep in memory.
let GENERATED_TOOLS = [];
try {
  const registry = await generateRegistry({ includeUtility: false });
  GENERATED_TOOLS = registry.tools;
  console.error(
    `[mcp-registry] generated ${GENERATED_TOOLS.length} tools from graph at ${registry.generatedAt}`,
  );
} catch (err) {
  console.error(`[mcp-registry] FAIL-LOUD on generator: ${err.message}`);
  console.error(`[mcp-registry] serving static tools only (${STATIC_TOOLS.length})`);
}

const TOOLS = [...STATIC_TOOLS, ...GENERATED_TOOLS];

// --- Server setup ---

const server = new Server(
  { name: "chorus-mcp-registry-spike", version: "0.0.1" },
  { capabilities: { resources: {}, tools: {} } },
);

// Resource handlers

server.setRequestHandler(ListResourcesRequestSchema, async () => ({
  resources: RESOURCES,
}));

server.setRequestHandler(ReadResourceRequestSchema, async (req) => {
  const uri = req.params.uri;
  if (uri === "chorus://principles") {
    const resp = await fetch(`${CHORUS_API}/api/loom/principles`);
    if (!resp.ok) {
      throw new Error(`Upstream API error: ${resp.status} ${resp.statusText}`);
    }
    const body = await resp.text();
    return {
      contents: [
        {
          uri,
          mimeType: "application/json",
          text: body,
        },
      ],
    };
  }
  throw new Error(`Unknown resource: ${uri}`);
});

// Tool handlers

server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: TOOLS,
}));

server.setRequestHandler(CallToolRequestSchema, async (req) => {
  const { name, arguments: args } = req.params;

  if (name === "enumerate_skills") {
    const includeUtility = Boolean(args?.include_utility);
    const sparql = `
      PREFIX chorus: <https://jeffbridwell.com/chorus#>
      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
      SELECT ?skill ?label ?comment ?impl
      WHERE {
        GRAPH <urn:chorus:ontology> {
          ?skill a chorus:Skill .
          ${includeUtility ? "" : "FILTER NOT EXISTS { ?skill a chorus:UtilitySkill }"}
          OPTIONAL { ?skill rdfs:label ?label }
          OPTIONAL { ?skill rdfs:comment ?comment }
          OPTIONAL { ?skill chorus:implementedIn ?impl }
        }
      }
      ORDER BY ?skill
    `;
    const url = `${FUSEKI}/pods/query`;
    const resp = await fetch(url, {
      method: "POST",
      headers: {
        "Content-Type": "application/x-www-form-urlencoded",
        Accept: "application/sparql-results+json",
      },
      body: new URLSearchParams({ query: sparql }).toString(),
    });
    if (!resp.ok) {
      throw new Error(`Fuseki error: ${resp.status}`);
    }
    const data = await resp.json();
    const skills = (data.results?.bindings ?? []).map((b) => ({
      id: b.skill?.value?.split("#").pop() ?? null,
      label: b.label?.value ?? null,
      description: b.comment?.value ?? null,
      implementedIn: b.impl?.value ?? null,
    }));
    return {
      content: [
        {
          type: "text",
          text: JSON.stringify({ count: skills.length, skills }, null, 2),
        },
      ],
    };
  }

  if (name === "invoke_skill_pull") {
    const cardId = args?.card_id;
    const role = args?.role;
    if (typeof cardId !== "number" || !role) {
      throw new Error("invoke_skill_pull requires card_id (integer) and role");
    }
    // SPIKE: dry-run only. Real implementation would exec chorus-hook-shim or cards CLI.
    return {
      content: [
        {
          type: "text",
          text: JSON.stringify(
            {
              dry_run: true,
              would_execute:
                `bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards move ${cardId} WIP ` +
                `&& role-state ${role} building card=${cardId}`,
              note:
                "SPIKE boundary — no actual state change. Real invocation requires auth + transactional commit; out of scope for #2452.",
            },
            null,
            2,
          ),
        },
      ],
    };
  }

  // Generated tools (chorus.skill.* / chorus.gate.*) — dispatch by namespace.
  // SPIKE: returns a dry-run preview with the implementedIn path. Real invocation
  // would delegate to chorus-hook-shim or exec the skill; scope-deferred per card.
  const generated = GENERATED_TOOLS.find((t) => t.name === name);
  if (generated) {
    return {
      content: [
        {
          type: "text",
          text: JSON.stringify(
            {
              dry_run: true,
              tool: name,
              source: generated._source,
              note:
                "SPIKE boundary — generated tool. Real invocation requires " +
                "auth + invocation-safety follow-on (#2452 card sequence item 4).",
            },
            null,
            2,
          ),
        },
      ],
    };
  }

  throw new Error(`Unknown tool: ${name}`);
});

// --- Run ---

const transport = new StdioServerTransport();
await server.connect(transport);
// Server runs until stdin closes.
