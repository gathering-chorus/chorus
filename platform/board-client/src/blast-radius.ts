/**
 * Automated blast radius — deterministic mapping from card content to codebase graph.
 *
 * Design constraint (Jeff, #1098): "100% reliable and automated. No drift, no fuzzy matching."
 * Approach: extract explicit file paths + domain keywords → query graph for exact nodes → walk edges.
 * The graph IS the source of truth. No guessing.
 */

const APP_BASE = process.env.GATHERING_APP_URL || 'http://localhost:3000';

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
    const re = new RegExp(`\\b${d}\\b`, 'i');
    return re.test(lower);
  });
}

async function fetchJson(url: string): Promise<any> {
  const res = await fetch(url, { signal: AbortSignal.timeout(5000) });
  if (!res.ok) return null;
  return res.json();
}

/**
 * Generate blast radius report for a card.
 * Pure function — reads card data, queries graph, returns report.
 */
export async function generateBlastRadius(
  cardId: number,
  title: string,
  description: string,
): Promise<BlastReport | null> {
  const fullText = `${title}\n${description}`;

  // Step 1: Get known domains from graph topology
  const topology = await fetchJson(`${APP_BASE}/api/codebase/topology`);
  if (!topology) return null;
  const knownDomains = Object.keys(topology.domains || {});

  // Step 2: Extract file paths and domains from card text
  const explicitFiles = extractFilePaths(fullText);
  const mentionedDomains = extractDomains(fullText, knownDomains);

  // Step 3: Resolve file paths to graph nodes and walk connections
  const touchedFiles = new Set<string>();
  const touchedDomains = new Set<string>();

  for (const filePath of explicitFiles) {
    // Try exact node lookup first
    let nodeData: GraphNode | null = await fetchJson(
      `${APP_BASE}/api/codebase/node/${encodeURIComponent(filePath)}`
    );

    // If bare filename (no directory prefix), search graph for it
    if (!nodeData && !filePath.includes('/')) {
      const search = await fetchJson(
        `${APP_BASE}/api/codebase/search?q=${encodeURIComponent(filePath)}&limit=1`
      );
      if (search?.nodes?.[0]) {
        const resolved = search.nodes[0].path;
        nodeData = await fetchJson(
          `${APP_BASE}/api/codebase/node/${encodeURIComponent(resolved)}`
        );
      }
    }

    if (nodeData) {
      touchedFiles.add(nodeData.path); // use canonical graph path
      touchedDomains.add(nodeData.domain);
      for (const conn of (nodeData.connected || [])) {
        touchedFiles.add(conn.path);
        if (conn.domain) touchedDomains.add(conn.domain);
      }
    } else {
      touchedFiles.add(filePath); // keep as-is if not in graph
    }
  }

  // Step 4: For mentioned domains without explicit files, get top files from search
  for (const domain of mentionedDomains) {
    touchedDomains.add(domain);
    if (!explicitFiles.some(f => f.includes(domain))) {
      const search = await fetchJson(`${APP_BASE}/api/codebase/search?domain=${domain}&limit=10`);
      if (search?.nodes) {
        for (const node of search.nodes) {
          touchedFiles.add(node.path);
        }
      }
    }
  }

  if (touchedFiles.size === 0) {
    // No files found — card may be non-code (process, docs, etc.)
    return {
      cardId,
      title,
      dimensions: [{ label: 'No codebase impact detected', files: [] }],
      totalFiles: 0,
      crossDomain: [],
      generated: new Date().toISOString(),
    };
  }

  // Step 5: Classify files into six dimensions
  const dimMap = new Map<string, string[]>();
  for (const file of touchedFiles) {
    const dim = classifyFile(file);
    if (!dimMap.has(dim)) dimMap.set(dim, []);
    dimMap.get(dim)!.push(file);
  }

  // Sort dimensions in canonical order
  const dimOrder = ['UI/UX', 'API', 'OWL/RDF', 'Page pattern', 'Tests', 'Harvests'];
  const dimensions: BlastDimension[] = dimOrder
    .filter(d => dimMap.has(d))
    .map(d => ({ label: d, files: dimMap.get(d)!.sort() }));

  // Cross-domain connections
  const crossDomain = Array.from(touchedDomains).sort();

  return {
    cardId,
    title,
    dimensions,
    totalFiles: touchedFiles.size,
    crossDomain,
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
