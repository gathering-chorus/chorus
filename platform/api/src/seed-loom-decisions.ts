/* eslint-disable security/detect-non-literal-fs-filename -- internal seeder: reads a decisions file from a controlled repo path, never untrusted input (#3429) */
/**
 * #2485 Move 1 — populate `urn:chorus:instances` with chorus:Decision instances.
 *
 * Pre-staged uncommitted while #2492 (DEC-093/101 collision) lands.
 * DO NOT --apply until corpus is collision-free.
 *
 * Modes:
 *   tsx seed-loom-decisions.ts                # dry-run → /tmp/loom-decisions-seed.rq
 *   tsx seed-loom-decisions.ts --apply        # POST to Fuseki update
 *   tsx seed-loom-decisions.ts --check        # SELECT count vs source, exit non-zero on drift
 */

import * as fs from 'node:fs';
import * as path from 'node:path';

const REPO_ROOT = path.resolve(__dirname, '../../..');
const DECISIONS_MD = path.join(REPO_ROOT, 'roles/wren/decisions.md');
const ADR_DIR = path.join(REPO_ROOT, 'roles/silas/adr');
const FUSEKI_UPDATE = 'http://localhost:3030/pods/update';
const FUSEKI_QUERY = 'http://localhost:3030/pods/query';
const INSTANCES_GRAPH = 'urn:chorus:instances';
const CHORUS_PREFIX = 'https://jeffbridwell.com/chorus#';

export interface DecisionRow {
  id: string;
  uri: string;
  decisionType: 'DEC' | 'ADR';
  label: string;
  body: string;
  date?: string;
  status?: string;
  card?: number;
  supersedes?: string;
  source: string;
}

export interface CollisionReport {
  uri: string;
  sources: string[];
}

function pad(n: string): string {
  return n.padStart(3, '0');
}

export function parseDecisions(md: string, sourcePath: string): DecisionRow[] {
  const rows: DecisionRow[] = [];
  const re = /^## DEC-(\d+):\s*(.+)$/gm;
  const matches = [...md.matchAll(re)];
  for (let i = 0; i < matches.length; i++) {
    // eslint-disable-next-line security/detect-object-injection -- i is the bounded loop index over matches[]
    const m = matches[i];
    const num = m[1];
    const label = m[2].trim();
    const start = m.index! + m[0].length;
    const end = i + 1 < matches.length ? matches[i + 1].index! : md.length;
    const body = md.slice(start, end).trim();
    const dateMatch = /\*\*Date\*\*:\s*([0-9]{4}-[0-9]{2}-[0-9]{2})/.exec(body);
    const cardMatch = /\*\*Card\*\*:\s*#?(\d+)/.exec(body);
    rows.push({
      id: `dec-${pad(num)}`,
      uri: `${CHORUS_PREFIX}dec-${pad(num)}`,
      decisionType: 'DEC',
      label,
      body: `## DEC-${num}: ${label}\n${body}`,
      date: dateMatch ? dateMatch[1] : undefined,
      card: cardMatch ? Number.parseInt(cardMatch[1], 10) : undefined,
      source: sourcePath,
    });
  }
  return rows;
}

export function parseAdrFromString(content: string, filePath: string): DecisionRow | null {
  const titleMatch = /^#\s+ADR-(\d+):\s*(.+)$/m.exec(content);
  if (!titleMatch) return null;
  const num = titleMatch[1];
  const label = titleMatch[2].trim();
  const statusMatch = /\*\*Status:\*\*\s*([A-Za-z]+)/.exec(content);
  const cardMatch = /\*\*Card:\*\*[^#]*#(\d+)/.exec(content);
  const supersedesMatch = /\*\*Supersedes:\*\*\s*ADR-(\d+)/i.exec(content);
  return {
    id: `adr-${pad(num)}`,
    uri: `${CHORUS_PREFIX}adr-${pad(num)}`,
    decisionType: 'ADR',
    label,
    body: content,
    status: statusMatch ? statusMatch[1] : undefined,
    card: cardMatch ? Number.parseInt(cardMatch[1], 10) : undefined,
    supersedes: supersedesMatch ? `${CHORUS_PREFIX}adr-${pad(supersedesMatch[1])}` : undefined,
    source: filePath,
  };
}

export function parseAdr(filePath: string): DecisionRow | null {
  const content = fs.readFileSync(filePath, 'utf-8');
  return parseAdrFromString(content, filePath);
}

export function detectCollisions(rows: DecisionRow[]): CollisionReport | null {
  const seen = new Map<string, string>();
  for (const r of rows) {
    const prior = seen.get(r.uri);
    if (prior !== undefined) {
      return { uri: r.uri, sources: [prior, r.source] };
    }
    seen.set(r.uri, r.source);
  }
  return null;
}

function escapeTurtleString(s: string): string {
  return s
    .replace(/\\/g, '\\\\')
    .replace(/"/g, '\\"')
    .replace(/\n/g, '\\n')
    .replace(/\r/g, '\\r')
    .replace(/\t/g, '\\t');
}

function rowToTriples(r: DecisionRow): string[] {
  const u = `<${r.uri}>`;
  const triples: string[] = [
    `${u} a chorus:Decision .`,
    `${u} chorus:id "${r.id}" .`,
    `${u} rdfs:label "${escapeTurtleString(r.label)}" .`,
    `${u} rdfs:comment "${escapeTurtleString(r.body)}" .`,
    `${u} chorus:decisionType "${r.decisionType}" .`,
  ];
  if (r.date) triples.push(`${u} dcterms:created "${r.date}"^^xsd:date .`);
  if (r.status) triples.push(`${u} chorus:status "${r.status}" .`);
  if (r.card !== undefined) triples.push(`${u} chorus:relatedCard "${r.card}"^^xsd:integer .`);
  if (r.supersedes) triples.push(`${u} chorus:supersedes <${r.supersedes}> .`);
  return triples;
}

export function buildInsert(rows: DecisionRow[]): string {
  const triples = rows.flatMap(rowToTriples);
  return [
    `PREFIX chorus: <${CHORUS_PREFIX}>`,
    'PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>',
    'PREFIX dcterms: <http://purl.org/dc/terms/>',
    'PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>',
    '',
    'INSERT DATA {',
    `  GRAPH <${INSTANCES_GRAPH}> {`,
    ...triples.map((t) => `    ${t}`),
    '  }',
    '}',
  ].join('\n');
}

export function loadAllRows(): DecisionRow[] {
  const decMd = fs.readFileSync(DECISIONS_MD, 'utf-8');
  const decRows = parseDecisions(decMd, DECISIONS_MD);
  const adrFiles = fs
    .readdirSync(ADR_DIR)
    .filter((f) => /^ADR-\d+.*\.md$/.test(f))
    .map((f) => path.join(ADR_DIR, f));
  const adrRows = adrFiles.map(parseAdr).filter((r): r is DecisionRow => r !== null);
  return [...decRows, ...adrRows];
}

async function main(): Promise<void> {
  const mode = process.argv.includes('--apply')
    ? 'apply'
    : process.argv.includes('--check')
      ? 'check'
      : 'dry-run';

  const rows = loadAllRows();
  const decCount = rows.filter((r) => r.decisionType === 'DEC').length;
  const adrCount = rows.filter((r) => r.decisionType === 'ADR').length;
  console.error(`parsed: ${decCount} DECs + ${adrCount} ADRs = ${rows.length} rows`);

  const collision = detectCollisions(rows);
  if (collision) {
    console.error(`COLLISION: ${collision.uri} from ${collision.sources.join(' AND ')}`);
    process.exit(2);
  }

  const insert = buildInsert(rows);

  if (mode === 'dry-run') {
    const out = '/tmp/loom-decisions-seed.rq';
    fs.writeFileSync(out, insert);
    console.error(`dry-run: wrote ${insert.length} bytes to ${out}`);
    return;
  }

  if (mode === 'check') {
    const query = `PREFIX chorus: <${CHORUS_PREFIX}>\nSELECT (COUNT(*) AS ?n) WHERE { GRAPH <${INSTANCES_GRAPH}> { ?d a chorus:Decision } }`;
    const res = await fetch(FUSEKI_QUERY, {
      method: 'POST',
      headers: { 'Content-Type': 'application/sparql-query', Accept: 'application/sparql-results+json' },
      body: query,
    });
    const json = (await res.json()) as { results: { bindings: Array<{ n: { value: string } }> } };
    const n = Number.parseInt(json.results.bindings[0]?.n.value ?? '0', 10);
    console.error(`graph: ${n} | source: ${rows.length} | drift: ${n - rows.length}`);
    process.exit(n === rows.length ? 0 : 1);
  }

  const res = await fetch(FUSEKI_UPDATE, {
    method: 'POST',
    headers: { 'Content-Type': 'application/sparql-update' },
    body: insert,
  });
  if (!res.ok) {
    const body = await res.text();
    throw new Error(`Fuseki update ${res.status}: ${body.slice(0, 400)}`);
  }
  console.error(`apply: inserted ${rows.length} chorus:Decision instances`);
}

if (require.main === module) {
  main().catch((err) => {
    console.error(err);
    process.exit(1);
  });
}
