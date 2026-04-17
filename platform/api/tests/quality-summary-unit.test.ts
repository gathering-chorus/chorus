/**
 * quality-summary — unit tests (#2167).
 *
 * Target: 80%+ on src/quality-summary.ts. Builds fixture test trees under
 * tempdirs pointed at via GATHERING_ROOT, CHORUS_ROOT, QUALITY_CACHE_PATH.
 */

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'quality-test-'));
const GATH = path.join(TMP, 'gathering');
const CHOR = path.join(TMP, 'chorus');
const CACHE = path.join(TMP, 'cache.json');

process.env.GATHERING_ROOT = GATH;
process.env.CHORUS_ROOT = CHOR;
process.env.QUALITY_CACHE_PATH = CACHE;

function load() {
  return require('../src/quality-summary');
}

afterAll(() => {
  try { fs.rmSync(TMP, { recursive: true, force: true }); } catch {}
});

function clear() {
  try { fs.rmSync(GATH, { recursive: true, force: true }); } catch {}
  try { fs.rmSync(CHOR, { recursive: true, force: true }); } catch {}
  try { fs.unlinkSync(CACHE); } catch {}
  fs.mkdirSync(GATH, { recursive: true });
  fs.mkdirSync(CHOR, { recursive: true });
}

function write(p: string, content: string) {
  fs.mkdirSync(path.dirname(p), { recursive: true });
  fs.writeFileSync(p, content);
}

describe('quality-summary — runQualityScan shape', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('empty roots → total 0, pyramid of 10 layers', () => {
    const r = load().runQualityScan();
    expect(r.total).toBe(0);
    expect(r.pyramid).toHaveLength(10);
    expect(r.repos).toHaveLength(2);
    expect(r.repos.map((x: any) => x.name)).toEqual(['Gathering App', 'Chorus Platform']);
  });

  test('writes cache on scan', () => {
    load().runQualityScan();
    expect(fs.existsSync(CACHE)).toBe(true);
    const cached = JSON.parse(fs.readFileSync(CACHE, 'utf-8'));
    expect(cached).toHaveProperty('data');
    expect(cached).toHaveProperty('timestamp');
  });

  test('pyramid sorts E2E/security/perf first, then integration, then unit', () => {
    const r = load().runQualityScan();
    const tiers = r.pyramid.map((l: any) => l.key);
    // E2E, security, performance should come first (tier 1)
    const tier1Keys = ['e2e', 'security', 'performance'];
    for (const k of tier1Keys) {
      const idx = tiers.indexOf(k);
      expect(idx).toBeGreaterThanOrEqual(0);
      expect(idx).toBeLessThan(6);  // within top half
    }
  });
});

describe('quality-summary — countTestCases per file type', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('TS jest it() + test() counted', () => {
    write(path.join(GATH, 'tests/unit/a.test.ts'),
      `it('one', ()=>{}); test('two', ()=>{}); describe('x', ()=>{ it('three', ()=>{}); });`);
    const r = load().runQualityScan();
    const layer = r.pyramid.find((l: any) => l.key === 'ts-unit');
    expect(layer.count).toBe(3);
  });

  test('.bats @test counted', () => {
    write(path.join(CHOR, 'platform/tests/a.bats'),
      `@test "one" { true; }\n@test "two" { true; }\n`);
    const r = load().runQualityScan();
    const layer = r.pyramid.find((l: any) => l.key === 'bats');
    expect(layer.count).toBe(2);
  });

  test('.feature Scenario counted', () => {
    write(path.join(CHOR, 'platform/tests/a.feature'),
      `Feature: x\n  Scenario: one\n    Given y\n  Scenario: two\n    Given z\n`);
    const r = load().runQualityScan();
    const layer = r.pyramid.find((l: any) => l.key === 'bdd');
    expect(layer.count).toBe(2);
  });

  test('.rs #[test] counted', () => {
    write(path.join(CHOR, 'platform/services/chorus-inject/src/lib.rs'),
      `#[test] fn a() {}\n#[test] fn b() {}\nfn not_test() {}\n`);
    const r = load().runQualityScan();
    const layer = r.pyramid.find((l: any) => l.key === 'rust-unit');
    expect(layer.count).toBe(2);
  });

  test('unreadable file returns 0 (try/catch)', () => {
    // Create a dir with the expected file name — readFileSync on a directory fails.
    fs.mkdirSync(path.join(GATH, 'tests/unit'), { recursive: true });
    fs.mkdirSync(path.join(GATH, 'tests/unit/fake.test.ts'), { recursive: true });
    const r = load().runQualityScan();
    // Doesn't crash; count is 0 for this "file"
    expect(r.pyramid.find((l: any) => l.key === 'ts-unit').count).toBe(0);
  });
});

describe('quality-summary — classifyTestKind', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('playwright file → ui', () => {
    write(path.join(GATH, 'e2e/tests/login.spec.ts'),
      `import { test } from '@playwright/test';\ntest('login', async ({ page }) => { await page.goto('/'); });`);
    const r = load().runQualityScan();
    const layer = r.pyramid.find((l: any) => l.key === 'e2e');
    expect(layer.uiCount).toBe(1);
  });

  test('supertest file → api', () => {
    write(path.join(GATH, 'tests/integration/api.test.ts'),
      `import request from 'supertest';\ntest('api', async () => { await request(app).get('/x'); });`);
    const r = load().runQualityScan();
    const layer = r.pyramid.find((l: any) => l.key === 'integration');
    expect(layer.apiCount).toBe(1);
  });

  test('plain function test → other', () => {
    write(path.join(GATH, 'tests/unit/util.test.ts'),
      `function util() { return 1; }\ntest('util', () => { expect(util()).toBe(1); });`);
    const r = load().runQualityScan();
    const layer = r.pyramid.find((l: any) => l.key === 'ts-unit');
    const file = layer.files[0];
    expect(file.kind).toBe('other');
  });

  test('.bats with curl → api, without → other', () => {
    write(path.join(CHOR, 'platform/tests/api.bats'),
      `@test "curl test" { curl -s http://localhost/x; }\n`);
    write(path.join(CHOR, 'platform/tests/other.bats'),
      `@test "plain" { true; }\n`);
    const r = load().runQualityScan();
    const bats = r.pyramid.find((l: any) => l.key === 'bats');
    expect(bats.apiCount).toBe(1);
    expect(bats.files.some((f: any) => f.kind === 'api')).toBe(true);
    expect(bats.files.some((f: any) => f.kind === 'other')).toBe(true);
  });

  test('.feature with HTTP → api', () => {
    write(path.join(CHOR, 'platform/tests/api.feature'),
      `Feature: x\n  Scenario: s\n    Given HTTP request\n`);
    const r = load().runQualityScan();
    const bdd = r.pyramid.find((l: any) => l.key === 'bdd');
    expect(bdd.apiCount).toBeGreaterThan(0);
  });

  test('.rs test → other (no UI)', () => {
    write(path.join(CHOR, 'platform/services/chorus-inject/src/x.rs'),
      `#[test] fn t() {}`);
    const r = load().runQualityScan();
    const rust = r.pyramid.find((l: any) => l.key === 'rust-unit');
    expect(rust.files.every((f: any) => f.kind === 'other')).toBe(true);
  });

  test('UI render patterns → ui', () => {
    write(path.join(GATH, 'tests/unit/page.test.ts'),
      `test('render', () => { const s = renderPage(); expect(s).toContain('.render('); });`);
    const r = load().runQualityScan();
    const unit = r.pyramid.find((l: any) => l.key === 'ts-unit');
    expect(unit.uiCount).toBeGreaterThan(0);
  });
});

describe('quality-summary — inferDomain', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('file named photos.test.ts → photos', () => {
    write(path.join(GATH, 'tests/unit/photos.test.ts'), `test('a', ()=>{});`);
    const r = load().runQualityScan();
    const layer = r.pyramid.find((l: any) => l.key === 'ts-unit');
    expect(layer.files[0].domain).toBe('photos');
  });

  test('file stem split by hyphen resolves to domain', () => {
    write(path.join(GATH, 'tests/unit/photo-upload.test.ts'), `test('a', ()=>{});`);
    const r = load().runQualityScan();
    const layer = r.pyramid.find((l: any) => l.key === 'ts-unit');
    expect(layer.files[0].domain).toBe('photos');
  });

  test('unknown name in /chorus/ path → chorus', () => {
    write(path.join(CHOR, 'platform/board-client/tests/random.test.ts'),
      `test('a', ()=>{});`);
    const r = load().runQualityScan();
    const layer = r.pyramid.find((l: any) => l.key === 'chorus-ts');
    expect(layer.files[0].domain).toBe('chorus');
  });

  test('unknown name outside chorus → other', () => {
    write(path.join(GATH, 'tests/unit/totally-obscure.test.ts'),
      `test('a', ()=>{});`);
    const r = load().runQualityScan();
    const layer = r.pyramid.find((l: any) => l.key === 'ts-unit');
    expect(layer.files[0].domain).toBe('other');
  });

  test('explicit DOMAIN_MAP entry (e.g. cards → chorus)', () => {
    write(path.join(GATH, 'tests/unit/cards.test.ts'), `test('a', ()=>{});`);
    const r = load().runQualityScan();
    const layer = r.pyramid.find((l: any) => l.key === 'ts-unit');
    expect(layer.files[0].domain).toBe('chorus');
  });
});

describe('quality-summary — cache behavior', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('getQualityScan returns cached result when fresh', () => {
    const mod = load();
    const first = mod.runQualityScan();
    const second = mod.getQualityScan();
    // Second call hits cache — same scannedAt
    expect(second.scannedAt).toBe(first.scannedAt);
  });

  test('getQualityScan bypasses expired cache (>1h old)', () => {
    const mod = load();
    // Write a stale cache
    fs.mkdirSync(path.dirname(CACHE), { recursive: true });
    fs.writeFileSync(CACHE, JSON.stringify({
      data: { total: 9999, pyramid: [], repos: [], scannedAt: '2020-01-01' },
      timestamp: Date.now() - (2 * 60 * 60 * 1000),  // 2h old
    }));
    const fresh = mod.getQualityScan();
    expect(fresh.total).not.toBe(9999);
  });

  test('malformed cache is treated as missing', () => {
    fs.mkdirSync(path.dirname(CACHE), { recursive: true });
    fs.writeFileSync(CACHE, 'not json');
    const mod = load();
    const r = mod.getQualityScan();
    expect(r.total).toBe(0);
  });

  test('missing cache triggers fresh scan', () => {
    const r = load().getQualityScan();
    expect(r).toHaveProperty('scannedAt');
    expect(fs.existsSync(CACHE)).toBe(true);
  });
});

describe('quality-summary — getQualityByDomain', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('aggregates files across layers for one domain', () => {
    write(path.join(GATH, 'tests/unit/photos.test.ts'), `test('a', ()=>{}); test('b', ()=>{});`);
    write(path.join(GATH, 'tests/integration/photos.test.ts'), `test('c', ()=>{});`);
    const r = load().getQualityByDomain('photos');
    expect(r.domain).toBe('photos');
    expect(r.total).toBe(3);
    expect(r.files).toHaveLength(2);
    expect(r.layers.length).toBeGreaterThan(0);
  });

  test('unknown domain → empty response', () => {
    const r = load().getQualityByDomain('no-such-domain');
    expect(r.total).toBe(0);
    expect(r.files).toEqual([]);
    expect(r.layers).toEqual([]);
  });

  test('omits layers with zero matching files', () => {
    write(path.join(GATH, 'tests/unit/photos.test.ts'), `test('a', ()=>{});`);
    const r = load().getQualityByDomain('photos');
    for (const layer of r.layers) {
      expect(layer.files.length).toBeGreaterThan(0);
    }
  });
});

describe('quality-summary — findFiles robustness', () => {
  beforeEach(() => { clear(); jest.resetModules(); });

  test('node_modules, .git, target dirs are skipped', () => {
    write(path.join(GATH, 'tests/unit/node_modules/pkg/a.test.ts'), `test('a', ()=>{});`);
    write(path.join(GATH, 'tests/unit/real.test.ts'), `test('b', ()=>{});`);
    const r = load().runQualityScan();
    const layer = r.pyramid.find((l: any) => l.key === 'ts-unit');
    expect(layer.files).toHaveLength(1);
    expect(layer.files[0].name).toContain('real.test.ts');
  });

  test('.spec suffix also recognized', () => {
    write(path.join(GATH, 'tests/unit/a.spec.ts'), `test('a', ()=>{});`);
    const r = load().runQualityScan();
    const layer = r.pyramid.find((l: any) => l.key === 'ts-unit');
    expect(layer.fileCount).toBe(1);
  });

  test('.js test files also recognized', () => {
    write(path.join(GATH, 'tests/unit/a.test.js'), `test('a', () => {});`);
    const r = load().runQualityScan();
    const layer = r.pyramid.find((l: any) => l.key === 'ts-unit');
    expect(layer.fileCount).toBe(1);
  });
});
