#!/usr/bin/env node
// mcp-tool-description-lint.js — enforce the description-as-doc-surface contract
// on every MCP tool exposed by chorus-api.
//
// #2475 thread 5 of 5. The description string IS the doc surface — Claude reads
// it every time it reaches for the tool, far more often than CLAUDE.md fragments.
// Drift in the description is doc rot at runtime.
//
// Constraints (the "shape"):
//   1. Min length 80 chars — under that, the description is missing context.
//   2. Disposition guidance — the description must tell Claude WHEN to use the
//      tool. Heuristic: contains "Use this" / "Use to" / "Send" or similar verb
//      framing. Pure capability statements without WHEN guidance fail.
//   3. Anti-pattern clause — the description must include a "Do NOT" / "Don't"
//      / "do not use" clause naming what NOT to use it for. Without this,
//      Claude will reach for the friendliest tool for everything (the moat).
//   4. Polymorphic enum clarity — if a property has an enum, its description
//      must clarify what each value means (e.g., "AI roles vs human") OR be
//      single-purpose enough not to need it. Heuristic: enum length >= 3
//      requires a description with at least one comma-separated clarifier or
//      "vs" / "or" disjunction.
//
// Exit 0 = pass. Exit 1 = at least one violation, with detail.
//
// Source-of-truth: parses platform/api/src/mcp/server.ts (tools array). When
// a second module exposes tools, extend SOURCES below.

const fs = require('fs');
const path = require('path');

const REPO_ROOT = path.resolve(__dirname, '..', '..');
const SOURCES = [
  path.join(REPO_ROOT, 'platform', 'api', 'src', 'mcp', 'server.ts'),
];

const violations = [];

function lintDescription(toolName, desc) {
  const issues = [];
  if (typeof desc !== 'string' || desc.length < 80) {
    issues.push(`description too short (${desc?.length ?? 0} chars, min 80)`);
  }
  // Disposition: tells Claude when to reach for the tool
  if (!/(Use (this|to)|Send|Post|Fetch|Query|Notify|Coordinate|Get )/i.test(desc || '')) {
    issues.push(`no disposition guidance (no "Use this", "Send", etc. — Claude can\'t tell when to use it)`);
  }
  // Anti-pattern clause
  if (!/(Do NOT|Don'?t|do not use)/i.test(desc || '')) {
    issues.push(`no anti-pattern clause ("Do NOT use for ..." — without this Claude reaches for the friendliest tool)`);
  }
  return issues;
}

function lintEnumDescription(toolName, propName, prop) {
  const issues = [];
  if (!Array.isArray(prop.enum) || prop.enum.length < 3) return issues;
  const desc = prop.description || '';
  // Clarifier heuristic: at least one disjunction (" — ", "vs", "or", ",")
  // that suggests categorization within the enum.
  if (!/(—|vs|, |\bor\b|: )/i.test(desc)) {
    issues.push(`property "${propName}" has ${prop.enum.length}-value enum but description "${desc}" lacks polymorphism clarifier`);
  }
  return issues;
}

function extractToolsFromSource(srcPath) {
  // Lightweight: read source as text, locate `tools: [` in setRequestHandler,
  // require the file via a tsx transpile would need build. Instead grep for
  // the specific shape used in mcp/server.ts: an exported tool def named
  // *_TOOL_DEF or a literal {name, description, inputSchema} in the array.
  if (!fs.existsSync(srcPath)) return [];
  const src = fs.readFileSync(srcPath, 'utf8');

  // Match each tool literal: { name: '...', description: '...', inputSchema: {...} }
  // Description value can be a single string or string-concat across lines.
  const tools = [];
  // Find patterns like:  name: 'chorus_nudge_message',  description: '...',  inputSchema: { ... },
  const toolRegex = /name:\s*'([^']+)'\s*,\s*description:\s*'((?:\\.|[^'\\])*)'/g;
  let m;
  while ((m = toolRegex.exec(src)) !== null) {
    tools.push({ name: m[1], description: m[2].replace(/\\'/g, "'"), source: srcPath });
  }
  return tools;
}

function extractEnumDescriptionsFromSource(srcPath, toolName) {
  // Lightweight enum description extraction: find the inputSchema for the tool
  // and extract enum + description fields. Best-effort — not a full TS parse.
  if (!fs.existsSync(srcPath)) return [];
  const src = fs.readFileSync(srcPath, 'utf8');

  // Find the tool block and extract properties with enum + description.
  const toolBlockRegex = new RegExp(`name:\\s*'${toolName}'[\\s\\S]*?required:\\s*\\[`, 'm');
  const blockMatch = toolBlockRegex.exec(src);
  if (!blockMatch) return [];
  const block = blockMatch[0];

  const props = [];
  // Each property: name: { type: 'string', enum: [...], description: '...' }
  const propRegex = /(\w+):\s*\{\s*type:\s*'string'[^}]*?\benum:\s*\[([^\]]+)\][^}]*?\bdescription:\s*'([^']+)'/g;
  let m;
  while ((m = propRegex.exec(block)) !== null) {
    const enumValues = m[2].split(',').map((s) => s.trim().replace(/['"]/g, '')).filter(Boolean);
    props.push({ name: m[1], enum: enumValues, description: m[3] });
  }
  return props;
}

function main() {
  let totalTools = 0;
  for (const src of SOURCES) {
    const tools = extractToolsFromSource(src);
    totalTools += tools.length;
    for (const tool of tools) {
      const descIssues = lintDescription(tool.name, tool.description);
      const enumProps = extractEnumDescriptionsFromSource(src, tool.name);
      const enumIssues = enumProps.flatMap((p) => lintEnumDescription(tool.name, p.name, p));
      const all = [...descIssues, ...enumIssues];
      if (all.length > 0) {
        violations.push({ tool: tool.name, source: tool.source, issues: all });
      }
    }
  }

  if (totalTools === 0) {
    console.error('mcp-tool-description-lint: WARN — no tools found in any source. Check SOURCES list.');
    process.exit(0);
  }

  if (violations.length === 0) {
    console.log(`mcp-tool-description-lint: PASS — ${totalTools} tool(s) meet description shape.`);
    process.exit(0);
  }

  console.error(`mcp-tool-description-lint: FAIL — ${violations.length} tool(s) violate description shape:\n`);
  for (const v of violations) {
    console.error(`  ${v.tool}  (${path.relative(REPO_ROOT, v.source)})`);
    for (const issue of v.issues) {
      console.error(`    - ${issue}`);
    }
    console.error('');
  }
  console.error('See platform/scripts/mcp-tool-description-lint.js for the shape contract.');
  process.exit(1);
}

main();
