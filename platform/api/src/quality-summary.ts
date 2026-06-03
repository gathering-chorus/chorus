/* eslint-disable security/detect-non-literal-fs-filename, security/detect-object-injection --
 * fs paths from server-controlled env constants (GATHERING_ROOT, CHORUS_ROOT,
 * QUALITY_CACHE_PATH) joined with discovered filenames; object indexing on
 * internally-derived TestKind/layer/domain enum keys.
 */
/**
 * Quality Service — #2099 per-page migration from Gathering.
 *
 * Scans Gathering + Chorus repos for test files, classifies each by layer,
 * kind (api/ui/other), and domain. Returns a pyramid-ordered breakdown plus
 * per-repo aggregates. Semantics match
 * jeff-bridwell-personal-site/src/services/quality-scanner.service.ts —
 * same DOMAIN_MAP, same kind classifiers, same layer color codes. Cache
 * lives at ~/.chorus/quality-cache.json (chorus-owned, not Gathering's).
 */

import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { CHORUS_ROOT } from './lib/chorus-paths'; // #3197 — single root source

const GATHERING_ROOT = process.env.GATHERING_ROOT || path.join(os.homedir(), 'CascadeProjects', 'jeff-bridwell-personal-site');
// #2167 test seam — overridable so tests can use a tempdir.
const CACHE_PATH = process.env.QUALITY_CACHE_PATH || path.join(os.homedir(), '.chorus', 'quality-cache.json');
const CACHE_TTL_MS = 60 * 60 * 1000;

export type TestKind = 'api' | 'ui' | 'other';

export interface TestFile {
  name: string;
  count: number;
  kind: TestKind;
  domain: string;
}

export interface TestLayer {
  name: string;
  key: string;
  count: number;
  fileCount: number;
  color: string;
  detail: string;
  files: TestFile[];
  apiCount: number;
  uiCount: number;
}

export interface RepoBreakdown {
  name: string;
  total: number;
  layers: TestLayer[];
}

export interface QualityScan {
  total: number;
  pyramid: TestLayer[];
  repos: RepoBreakdown[];
  scannedAt: string;
}

interface CachedScan {
  data: QualityScan;
  timestamp: number;
}

function countTestCases(filePath: string): number {
  try {
    const content = fs.readFileSync(filePath, 'utf-8');
    const ext = path.extname(filePath);
    if (ext === '.bats') return (content.match(/^@test\s/gm) || []).length;
    if (ext === '.feature') return (content.match(/^\s*Scenario:/gm) || []).length;
    if (ext === '.rs') return (content.match(/#\[test\]/g) || []).length;
    const itMatches = (content.match(/\bit\s*\(/g) || []).length;
    const testMatches = (content.match(/\btest\s*\(/g) || []).length;
    return itMatches + testMatches;
  } catch {
    return 0;
  }
}

function findFiles(dir: string, predicate: (f: string) => boolean): string[] {
  const results: string[] = [];
  if (!fs.existsSync(dir)) return results;
  try {
    const entries = fs.readdirSync(dir, { withFileTypes: true });
    for (const entry of entries) {
      const full = path.join(dir, entry.name);
      if (entry.name === 'node_modules' || entry.name === '.git' || entry.name === 'target') continue;
      if (entry.isDirectory()) results.push(...findFiles(full, predicate));
      else if (predicate(entry.name)) results.push(full);
    }
  } catch { /* permission */ }
  return results;
}

function isTestFile(name: string): boolean {
  return /\.(test|spec)\.(ts|js)$/.test(name) || name.endsWith('.bats') || name.endsWith('.feature');
}

function classifyTestKind(filePath: string): TestKind {
  try {
    const content = fs.readFileSync(filePath, 'utf-8');
    const ext = path.extname(filePath);
    if (ext === '.bats') return /curl\s|http|HTTP|localhost/.test(content) ? 'api' : 'other';
    if (ext === '.feature') return /HTTP|endpoint|API|response/.test(content) ? 'api' : 'other';
    if (ext === '.rs') return 'other';
    if (content.includes('@playwright/test') || content.includes('page.goto')) return 'ui';
    if (/\.render\(|res\.render|\.toHaveTitle|\.locator\(|\.toBeVisible|\.getByText/.test(content)) return 'ui';
    if (/request\(app\)|\.json\(\)|response\.body|fetch\(['"]http|supertest|mockRes\.json|\.status\(/.test(content)) return 'api';
    return 'other';
  } catch {
    return 'other';
  }
}

const DOMAIN_MAP: Record<string, string> = {
  photos: 'photos', photo: 'photos', 'photo-enrichment': 'photos', 'photo-upload': 'photos',
  music: 'music', album: 'music',
  people: 'people', person: 'people',
  cooking: 'cooking', recipe: 'cooking',
  reading: 'reading', book: 'books', 'book-upload': 'books',
  watching: 'watching',
  notes: 'notes', note: 'notes',
  seeds: 'seeds', seed: 'seeds', 'seed-pipeline': 'seeds', 'seed-routing': 'seeds',
  stories: 'stories', story: 'stories',
  documents: 'documents', document: 'documents',
  social: 'social', gallery: 'gallery', blog: 'blog',
  property: 'property', garden: 'property',
  ideas: 'ideas', idea: 'ideas', glimmers: 'glimmers', search: 'search',
  cards: 'chorus', decisions: 'chorus', briefs: 'chorus',
  analytics: 'chorus', cost: 'chorus', dashboard: 'chorus', quality: 'chorus',
  'code-inventory': 'code', 'tests-domain-code': 'code', 'codebase-graph': 'code', 'discover-code': 'code',
  'discover-endpoints': 'code', 'discover-pages': 'code',
  'framework-lint': 'code', 'graph-separation': 'code',
  'gate-code': 'gates', 'tdd': 'gates', 'demo-gate': 'gates', 'pair-gate': 'gates',
  'done-gate': 'gates', 'skill-gate': 'gates', 'chrome-tab-gate': 'gates',
  'pre-commit': 'gates', 'pre-push': 'gates',
  'domain-api-consolidated': 'chorus', 'domain-borg-services': 'chorus',
  'embed-sync': 'chorus', 'shacl-validation': 'chorus',
  'ollama-resilience': 'chorus', 'timestamp': 'chorus', 'logs-facet': 'chorus',
  athena: 'chorus', rca: 'chorus', crawl: 'chorus', 'crawl-shape': 'chorus', 'crawl-validation': 'chorus',
  'perf-budget': 'chorus', 'domain-api': 'chorus', 'domain-radius': 'chorus',
  'domain-releases': 'chorus', 'domain-pipeline': 'chorus', 'domain-docs': 'chorus',
  'domain-borg': 'chorus', 'domain-dependencies': 'chorus',
  hooks: 'chorus', fitness: 'chorus', 'borg-landing': 'chorus', 'chorus-landing': 'chorus',
};

function inferDomain(filePath: string): string {
  const base = path.basename(filePath).replace(/\.(test|spec|handler)\.(ts|js|rs|bats|feature)$/g, '').replace(/\.handler$/, '');
  if (DOMAIN_MAP[base]) return DOMAIN_MAP[base];
  const stem = base.split(/[.-]/)[0];
  if (DOMAIN_MAP[stem]) return DOMAIN_MAP[stem];
  if (filePath.includes('/chorus/')) return 'chorus';
  return 'other';
}

function buildLayer(name: string, key: string, color: string, detail: string, filePaths: string[], root: string): TestLayer {
  const files: TestFile[] = filePaths.map((f) => ({
    name: path.relative(root, f),
    count: countTestCases(f),
    kind: classifyTestKind(f),
    domain: inferDomain(f),
  })).sort((a, b) => b.count - a.count);
  const count = files.reduce((s, f) => s + f.count, 0);
  const apiCount = files.filter((f) => f.kind === 'api').reduce((s, f) => s + f.count, 0);
  const uiCount = files.filter((f) => f.kind === 'ui').reduce((s, f) => s + f.count, 0);
  return { name, key, color, detail, count, fileCount: filePaths.length, files, apiCount, uiCount };
}

function classifyGatheringFiles(root: string): TestLayer[] {
  const dirs: Record<string, string[]> = {
    'ts-unit': findFiles(path.join(root, 'tests/unit'), isTestFile),
    'integration': findFiles(path.join(root, 'tests/integration'), isTestFile),
    'e2e': findFiles(path.join(root, 'e2e/tests'), isTestFile),
    'security': findFiles(path.join(root, 'tests/security'), isTestFile),
    'performance': findFiles(path.join(root, 'tests/performance'), isTestFile),
  };
  return [
    buildLayer('TS Unit', 'ts-unit', '#1d4ed8', `${dirs['ts-unit'].length} files`, dirs['ts-unit'], root),
    buildLayer('Integration', 'integration', '#0891b2', `${dirs['integration'].length} files`, dirs['integration'], root),
    buildLayer('E2E', 'e2e', '#dc2626', `Playwright, ${dirs['e2e'].length} specs`, dirs['e2e'], root),
    buildLayer('Security', 'security', '#ea580c', `${dirs['security'].length} dedicated files`, dirs['security'], root),
    buildLayer('Performance', 'performance', '#9ca3af', `${dirs['performance'].length} files`, dirs['performance'], root),
  ];
}

function classifyChorusFiles(root: string): TestLayer[] {
  const tsTestDirs = [
    'platform/board-client/tests',
    'directing/clearing/tests',
    'platform/workflow-engine/tests',
    'platform/chorus-sdk/tests',
  ];
  const tsFiles: string[] = [];
  for (const d of tsTestDirs) tsFiles.push(...findFiles(path.join(root, d), isTestFile));

  const rustDirs = [
    'platform/services/chorus-hooks/src',
    'platform/services/chorus-hooks/tests',
    'platform/services/chorus-inject/src',
    'platform/services/chorus-inject/tests',
  ];
  const rustFiles: string[] = [];
  for (const d of rustDirs) {
    const found = findFiles(path.join(root, d), (n) => n.endsWith('.rs'));
    rustFiles.push(...found.filter((f) => {
      try { return fs.readFileSync(f, 'utf-8').includes('#[test]'); } catch { return false; }
    }));
  }

  const apiIntegrationFiles = findFiles(path.join(root, 'platform/api/tests'), isTestFile);
  const batsFiles = findFiles(path.join(root, 'platform/tests'), (n) => n.endsWith('.bats'));
  const gherkinFiles = findFiles(path.join(root, 'platform/tests'), (n) => n.endsWith('.feature'));

  return [
    buildLayer('Rust Unit', 'rust-unit', '#7c3aed', `${rustFiles.length} source files`, rustFiles, root),
    buildLayer('Board/Clearing TS', 'chorus-ts', '#2563eb', `${tsFiles.length} files`, tsFiles, root),
    buildLayer('API Integration', 'api-integration', '#059669', `${apiIntegrationFiles.length} files`, apiIntegrationFiles, root),
    buildLayer('BDD', 'bdd', '#d97706', `${gherkinFiles.length} feature files`, gherkinFiles, root),
    buildLayer('BATS Ops', 'bats', '#0e7490', `${batsFiles.length} files`, batsFiles, root),
  ];
}

function readCache(): QualityScan | null {
  try {
    if (!fs.existsSync(CACHE_PATH)) return null;
    const cached: CachedScan = JSON.parse(fs.readFileSync(CACHE_PATH, 'utf-8'));
    if (Date.now() - cached.timestamp > CACHE_TTL_MS) return null;
    return cached.data;
  } catch {
    return null;
  }
}

function writeCache(data: QualityScan): void {
  try {
    fs.mkdirSync(path.dirname(CACHE_PATH), { recursive: true });
    fs.writeFileSync(CACHE_PATH, JSON.stringify({ data, timestamp: Date.now() }, null, 2));
  } catch { /* best effort */ }
}

export function runQualityScan(): QualityScan {
  const gatheringLayers = classifyGatheringFiles(GATHERING_ROOT);
  const chorusLayers = classifyChorusFiles(CHORUS_ROOT);
  const gatheringTotal = gatheringLayers.reduce((s, l) => s + l.count, 0);
  const chorusTotal = chorusLayers.reduce((s, l) => s + l.count, 0);

  const tierOrder: Record<string, number> = {
    'e2e': 1, 'security': 1, 'performance': 1,
    'integration': 2, 'api-integration': 2, 'bdd': 2, 'bats': 2,
    'ts-unit': 3, 'rust-unit': 3, 'chorus-ts': 3,
  };
  const pyramid = [...gatheringLayers, ...chorusLayers].sort((a, b) => {
    const tA = tierOrder[a.key] || 2;
    const tB = tierOrder[b.key] || 2;
    if (tA !== tB) return tA - tB;
    return a.count - b.count;
  });

  const result: QualityScan = {
    total: gatheringTotal + chorusTotal,
    pyramid,
    repos: [
      { name: 'Gathering App', total: gatheringTotal, layers: gatheringLayers },
      { name: 'Chorus Platform', total: chorusTotal, layers: chorusLayers },
    ],
    scannedAt: new Date().toISOString(),
  };
  writeCache(result);
  return result;
}

export function getQualityScan(): QualityScan {
  return readCache() || runQualityScan();
}

export function getQualityByDomain(domain: string): {
  domain: string;
  total: number;
  files: TestFile[];
  layers: { name: string; key: string; count: number; files: TestFile[] }[];
} {
  const scan = getQualityScan();
  const layers = scan.pyramid.map((l) => {
    const domainFiles = l.files.filter((f) => f.domain === domain);
    return {
      name: l.name,
      key: l.key,
      count: domainFiles.reduce((s, f) => s + f.count, 0),
      files: domainFiles,
    };
  }).filter((l) => l.files.length > 0);
  const allFiles = layers.flatMap((l) => l.files);
  return {
    domain,
    total: allFiles.reduce((s, f) => s + f.count, 0),
    files: allFiles,
    layers,
  };
}
