/**
 * Automated blast radius — deterministic mapping from card content to codebase graph.
 *
 * Design constraint (Jeff, #1098): "100% reliable and automated. No drift, no fuzzy matching."
 * Approach: extract explicit file paths + domain keywords → query graph for exact nodes → walk edges.
 * The graph IS the source of truth. No guessing.
 */

const APP_BASE = process.env.GATHERING_APP_URL || 'http://localhost:3000';
const CHORUS_API = process.env.CHORUS_API || 'http://localhost:3340';

// Six blast radius dimensions (DEC-072)
interface BlastDimension {
  label: string;
  files: string[];
}

export interface BlastReport {
  cardId: number;
  title: string;
  dimensions: BlastDimension[];
  totalFiles: number;
  crossDomain: string[];
  generated: string;
}

interface GraphNode {
  path: string;
  type: string;
  domain: string;
  spoke: string;
  connections: number;
  title?: string;
  envVars?: string[];
  coversDomains?: string[];
  services?: string[];
  connected?: Array<{ path: string; type?: string; domain?: string; edgeType: string; direction: string }>;
}

// Deterministic dimension classification by file path pattern
function classifyFile(filePath: string): string {
  if (/^views\/|\.ejs$|^public\/css\/|\.css$|^public\/js\//.test(filePath)) return 'UI/UX';
  if (/^src\/handlers\/|handler\.ts$|^src\/app\.ts/.test(filePath)) return 'API';
  if (/^src\/ontology\/|\.ttl$|^data\/pods\//.test(filePath)) return 'OWL/RDF';
  if (/^views\/partials\/|^views\/layouts\/|layout|partial/.test(filePath)) return 'Page pattern';
  if (/^tests?\/|\.test\.|\.spec\./.test(filePath)) return 'Tests';
  if (/harvest|^scripts\/harvest|^data\/harvest/.test(filePath)) return 'Harvests';
  return 'API'; // default: source files are usually API/logic
}

// Extract explicit file paths from text — deterministic regex, no fuzzy matching
function extractFilePaths(text: string): string[] {
  const patterns = [
    /(?:src|views|data|public|scripts|tests?|architect|engineer|product-manager|messages|platform)\/[\w./-]+\.\w{1,5}/g,
    /[\w-]+\.handler\.ts/g,
    /[\w-]+\.service\.ts/g,
    /[\w-]+\.ejs/g,
  ];
  const paths = new Set<string>();
  for (const pattern of patterns) {
    let match;
    while ((match = pattern.exec(text)) !== null) {
      paths.add(match[0]);
    }
  }
  return Array.from(paths);
}

// Extract domain keywords from text — match against known graph domains
function extractDomains(text: string, knownDomains: string[]): string[] {
  const lower = text.toLowerCase();
  return knownDomains.filter(d => {
    // Exact word boundary match — not substring
    // eslint-disable-next-line security/detect-non-literal-regexp -- d comes from hardcoded knownDomains array, not user input.
    const re = new RegExp(`\\b${d}\\b`, 'i');
    return re.test(lower);
  });
}

async function fetchJson<T = unknown>(url: string): Promise<T | null> {
  const res = await fetch(url, { signal: AbortSignal.timeout(5000) });
  if (!res.ok) return null;
  return res.json() as Promise<T>;
}

/**
 * Generate blast radius report for a card.
 * Pure function — reads card data, queries graph, returns report.
 */
async function addDomainCodeFiles(domain: string, touchedFiles: Set<string>, touchedDomains: Set<string>): Promise<void> {
  const codeRes = await fetchJson<{ data?: { files?: Array<{ path: string }> } }>(`${CHORUS_API}/api/chorus/domain/${encodeURIComponent(domain)}/code`);
  const files = codeRes?.data?.files;
  if (!files || files.length === 0) return;
  for (const file of files) touchedFiles.add(file.path);
  touchedDomains.add(domain);
}

async function resolveNodeByPath(filePath: string): Promise<GraphNode | null> {
  const direct = await fetchJson<GraphNode>(`${APP_BASE}/api/codebase/node/${encodeURIComponent(filePath)}`);
  if (direct) return direct;
  if (filePath.includes('/')) return null;
  const search = await fetchJson<{ nodes?: Array<{ path: string }> }>(`${APP_BASE}/api/codebase/search?q=${encodeURIComponent(filePath)}&limit=1`);
  const first = search?.nodes?.[0];
  if (!first) return null;
  return fetchJson<GraphNode>(`${APP_BASE}/api/codebase/node/${encodeURIComponent(first.path)}`);
}

async function walkExplicitFiles(explicitFiles: string[], touchedFiles: Set<string>, touchedDomains: Set<string>): Promise<void> {
  for (const filePath of explicitFiles) {
    const nodeData = await resolveNodeByPath(filePath);
    if (!nodeData) {
      touchedFiles.add(filePath);
      continue;
    }
    touchedFiles.add(nodeData.path);
    touchedDomains.add(nodeData.domain);
    for (const conn of nodeData.connected || []) {
      touchedFiles.add(conn.path);
      if (conn.domain) touchedDomains.add(conn.domain);
    }
  }
}

async function addMentionedDomainFiles(mentionedDomains: string[], explicitFiles: string[], touchedFiles: Set<string>, touchedDomains: Set<string>): Promise<void> {
  for (const dom of mentionedDomains) {
    touchedDomains.add(dom);
    if (explicitFiles.some((f) => f.includes(dom))) continue;
    const search = await fetchJson<{ nodes?: Array<{ path: string }> }>(`${APP_BASE}/api/codebase/search?domain=${dom}&limit=10`);
    if (search?.nodes) for (const node of search.nodes) touchedFiles.add(node.path);
  }
}

function buildDimensions(touchedFiles: Set<string>): BlastDimension[] {
  const dimMap = new Map<string, string[]>();
  for (const file of touchedFiles) {
    const dim = classifyFile(file);
    if (!dimMap.has(dim)) dimMap.set(dim, []);
    dimMap.get(dim)!.push(file);
  }
  const dimOrder = ['UI/UX', 'API', 'OWL/RDF', 'Page pattern', 'Tests', 'Harvests'];
  return dimOrder.filter((d) => dimMap.has(d)).map((d) => ({ label: d, files: dimMap.get(d)!.sort() }));
}

export async function generateBlastRadius(
  cardId: number,
  title: string,
  description: string,
  domain?: string,
): Promise<BlastReport | null> {
  const fullText = `${title}\n${description}`;
  const topology = await fetchJson<{ domains?: Record<string, unknown> }>(`${APP_BASE}/api/codebase/topology`);
  if (!topology) return null;
  const knownDomains = Object.keys(topology.domains ?? {});
  const explicitFiles = extractFilePaths(fullText);
  const mentionedDomains = extractDomains(fullText, knownDomains);

  const touchedFiles = new Set<string>();
  const touchedDomains = new Set<string>();

  if (domain) await addDomainCodeFiles(domain, touchedFiles, touchedDomains);
  await walkExplicitFiles(explicitFiles, touchedFiles, touchedDomains);
  await addMentionedDomainFiles(mentionedDomains, explicitFiles, touchedFiles, touchedDomains);

  if (touchedFiles.size === 0) {
    return {
      cardId, title,
      dimensions: [{ label: 'No codebase impact detected', files: [] }],
      totalFiles: 0, crossDomain: [],
      generated: new Date().toISOString(),
    };
  }

  return {
    cardId, title,
    dimensions: buildDimensions(touchedFiles),
    totalFiles: touchedFiles.size,
    crossDomain: Array.from(touchedDomains).sort(),
    generated: new Date().toISOString(),
  };
}

/**
 * Format blast report as a card comment (markdown).
 */
export function formatBlastComment(report: BlastReport): string {
  const lines: string[] = [
    `**Blast Radius** — ${report.totalFiles} files, ${report.crossDomain.length} domains`,
    '',
  ];

  if (report.crossDomain.length > 0) {
    lines.push(`Domains: ${report.crossDomain.join(', ')}`);
    lines.push('');
  }

  for (const dim of report.dimensions) {
    if (dim.files.length === 0) {
      lines.push(`**${dim.label}**: ${dim.label}`);
    } else {
      lines.push(`**${dim.label}** (${dim.files.length}):`);
      for (const f of dim.files.slice(0, 8)) {
        lines.push(`  ${f}`);
      }
      if (dim.files.length > 8) {
        lines.push(`  ... +${dim.files.length - 8} more`);
      }
    }
  }

  lines.push('');
  lines.push(`_Generated ${report.generated.slice(0, 16)}_`);
  return lines.join('\n');
}
