/**
 * Quality Summary — #3657: a projection of the TESTS DOMAIN.
 *
 * Reads the owl-api V2 /tests collection (generated from chorus:TestShape)
 * and aggregates it into the pyramid the /werk/quality/ page renders. The
 * former filesystem scanner (#2099 lineage: fs-walk + DOMAIN_MAP + api/ui
 * content heuristics) is retired — tests are model data; this module holds
 * no classification opinions of its own. One truth: page = API = graph.
 *
 * Scope is what the domain declares. Today that is the chorus platform only
 * (no gathering tests are registered in the tests domain), so repos carries a
 * single honest entry rather than a fabricated Gathering rollup.
 *
 * Seams (tests bring their own world):
 *   OWL_API_BASE        — owl-api origin (default http://localhost:3360)
 *   QUALITY_CACHE_PATH  — cache file (default ~/.chorus/quality-cache.json)
 *
 * owl-api read-surface quirk: collection links.next says /v1/tests but only
 * /tests is routed — followNext() rewrites the prefix or page 2 404s.
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

const CACHE_TTL_MS = 60 * 60 * 1000;
const PAGE_LIMIT = 200; // owl-api 404s above this

function owlApiBase(): string {
  return process.env.OWL_API_BASE || 'http://localhost:3360';
}

function cachePath(): string {
  return process.env.QUALITY_CACHE_PATH || path.join(os.homedir(), '.chorus', 'quality-cache.json');
}

export interface TestFile {
  name: string;
  count: number;
  domain: string;
  hermeticCount: number;
  needsStackCount: number;
}

export interface TestLayer {
  name: string;
  key: string;
  count: number;
  fileCount: number;
  color: string;
  detail: string;
  files: TestFile[];
  hermeticCount: number;
  needsStackCount: number;
}

export interface RepoBreakdown {
  name: string;
  total: number;
  layers: TestLayer[];
}

export interface GeneratedFrom {
  graph?: string;
  shape?: string;
  shapeVersion?: string;
  commit?: string;
}

export interface QualityScan {
  total: number;
  pyramid: TestLayer[];
  repos: RepoBreakdown[];
  scannedAt: string;
  source: {
    kind: 'tests-domain';
    api: string;
    generatedFrom: GeneratedFrom | null;
  };
}

interface DomainTestRow {
  filePath?: string;
  pyramidLayer?: string;
  hermeticity?: string;
  covers?: string;
}

interface CollectionPage {
  data?: DomainTestRow[];
  generatedFrom?: GeneratedFrom;
  links?: { next?: string | null };
  count?: number;
}

interface CachedScan {
  data: QualityScan;
  timestamp: number;
}

// Pyramid display order (top → base). Colors are the validated categorical
// slots 1–4 (dataviz palette; the old red/amber set failed the normal-vision
// adjacency floor, ΔE 14.4). Fixed order — identity follows the layer.
const LAYER_ORDER: Array<{ key: string; name: string; color: string }> = [
  { key: 'e2e', name: 'E2E', color: '#2a78d6' },
  { key: 'bdd', name: 'BDD', color: '#008300' },
  { key: 'integration', name: 'Integration', color: '#e87ba4' },
  { key: 'unit', name: 'Unit', color: '#eda100' },
];

async function fetchAllTests(): Promise<{ rows: DomainTestRow[]; generatedFrom: GeneratedFrom | null }> {
  const base = owlApiBase();
  const rows: DomainTestRow[] = [];
  let generatedFrom: GeneratedFrom | null = null;
  let url: string | null = `${base}/tests?limit=${PAGE_LIMIT}`;
  while (url) {
    const res = await fetch(url);
    if (!res.ok) throw new Error(`tests-domain fetch failed: ${res.status} ${url}`);
    const page = (await res.json()) as CollectionPage;
    rows.push(...(page.data || []));
    if (!generatedFrom && page.generatedFrom) generatedFrom = page.generatedFrom;
    const next = page.links?.next;
    // links.next carries the unrouted /v1 prefix — rewrite to the served path.
    url = next ? base + next.replace(/^\/v1\/tests/, '/tests') : null;
  }
  return { rows, generatedFrom };
}

function buildLayer(meta: { key: string; name: string; color: string }, layerRows: DomainTestRow[]): TestLayer {
  const byFile = new Map<string, { count: number; domain: string; hermetic: number; needsStack: number }>();
  for (const row of layerRows) {
    const file = row.filePath || '(unregistered path)';
    const entry = byFile.get(file) || { count: 0, domain: row.covers || 'unassigned', hermetic: 0, needsStack: 0 };
    entry.count += 1;
    if (row.hermeticity === 'needs-stack') entry.needsStack += 1;
    else entry.hermetic += 1;
    byFile.set(file, entry);
  }
  const files: TestFile[] = [...byFile.entries()]
    .map(([name, e]) => ({ name, count: e.count, domain: e.domain, hermeticCount: e.hermetic, needsStackCount: e.needsStack }))
    .sort((a, b) => b.count - a.count);
  const hermeticCount = files.reduce((s, f) => s + f.hermeticCount, 0);
  const needsStackCount = files.reduce((s, f) => s + f.needsStackCount, 0);
  return {
    name: meta.name,
    key: meta.key,
    color: meta.color,
    count: layerRows.length,
    fileCount: files.length,
    detail: `${files.length} files · ${hermeticCount} hermetic / ${needsStackCount} needs-stack`,
    files,
    hermeticCount,
    needsStackCount,
  };
}

function buildScan(rows: DomainTestRow[], generatedFrom: GeneratedFrom | null): QualityScan {
  const byLayer = new Map<string, DomainTestRow[]>();
  for (const row of rows) {
    const key = row.pyramidLayer || 'unclassified';
    const list = byLayer.get(key) || [];
    list.push(row);
    byLayer.set(key, list);
  }
  const orderedKeys = [
    ...LAYER_ORDER.filter((l) => byLayer.has(l.key)),
    // A pyramidLayer value the model grows that this list doesn't know yet
    // still renders (grey) instead of being silently dropped.
    ...[...byLayer.keys()]
      .filter((k) => !LAYER_ORDER.some((l) => l.key === k))
      .map((k) => ({ key: k, name: k, color: '#9ca3af' })),
  ];
  const pyramid = orderedKeys.map((meta) => buildLayer(meta, byLayer.get(meta.key) || []));
  const total = rows.length;
  return {
    total,
    pyramid,
    repos: [{ name: 'Chorus Platform', total, layers: pyramid }],
    scannedAt: new Date().toISOString(),
    source: {
      kind: 'tests-domain',
      api: `${owlApiBase()}/tests`,
      generatedFrom,
    },
  };
}

function readCache(): QualityScan | null {
  try {
    if (!fs.existsSync(cachePath())) return null;
    const cached: CachedScan = JSON.parse(fs.readFileSync(cachePath(), 'utf-8')) as CachedScan;
    if (Date.now() - cached.timestamp > CACHE_TTL_MS) return null;
    // A pre-#3657 scanner cache has no source block — treat as expired.
    if (!cached.data?.source) return null;
    return cached.data;
  } catch {
    return null;
  }
}

function writeCache(data: QualityScan): void {
  try {
    fs.mkdirSync(path.dirname(cachePath()), { recursive: true });
    fs.writeFileSync(cachePath(), JSON.stringify({ data, timestamp: Date.now() }, null, 2));
  } catch { /* best effort */ }
}

export async function runQualityScan(): Promise<QualityScan> {
  const { rows, generatedFrom } = await fetchAllTests();
  const result = buildScan(rows, generatedFrom);
  writeCache(result);
  return result;
}

export async function getQualityScan(): Promise<QualityScan> {
  return readCache() || runQualityScan();
}

export async function getQualityByDomain(domain: string): Promise<{
  domain: string;
  total: number;
  files: TestFile[];
  layers: { name: string; key: string; count: number; files: TestFile[] }[];
}> {
  const scan = await getQualityScan();
  const layers = scan.pyramid
    .map((l) => {
      const domainFiles = l.files.filter((f) => f.domain === domain);
      return {
        name: l.name,
        key: l.key,
        count: domainFiles.reduce((s, f) => s + f.count, 0),
        files: domainFiles,
      };
    })
    .filter((l) => l.files.length > 0);
  const allFiles = layers.flatMap((l) => l.files);
  return {
    domain,
    total: allFiles.reduce((s, f) => s + f.count, 0),
    files: allFiles,
    layers,
  };
}
