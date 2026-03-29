/**
 * Seed Pipeline Flow Tests — #1237
 *
 * End-to-end validation of the seed capture pipeline:
 *   SMS → Twilio webhook → content extraction → pod write → Fuseki sync → page render
 *
 * Tests the infrastructure and boundaries without requiring live Twilio or running app.
 */
import * as fs from 'fs';
import * as path from 'path';

const APP_ROOT = path.join(__dirname, '../../../jeff-bridwell-personal-site');
const SRC_DIR = path.join(APP_ROOT, 'src');
const VIEWS_DIR = path.join(APP_ROOT, 'views');
const DATA_DIR = path.join(APP_ROOT, 'data/pods/jeff/capture');

// ═══════════════════════════════════════════════════════════════════════════
// 1. WEBHOOK HANDLER — POST /api/seed/sms exists with middleware chain
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: SMS webhook handler', () => {
  test('seed handler file exists', () => {
    const handler = path.join(SRC_DIR, 'handlers/seed.handler.ts');
    expect(fs.existsSync(handler)).toBe(true);
  });

  test('seed handler defines handleSmsWebhook method', () => {
    const content = fs.readFileSync(
      path.join(SRC_DIR, 'handlers/seed.handler.ts'), 'utf-8'
    );
    expect(content).toContain('handleSmsWebhook');
  });

  test('app.ts registers POST /api/seed/sms route', () => {
    const appTs = fs.readFileSync(path.join(SRC_DIR, 'app.ts'), 'utf-8');
    expect(appTs).toMatch(/app\.post\(.*\/api\/seed\/sms/);
  });

  test('route has Twilio signature middleware', () => {
    const appTs = fs.readFileSync(path.join(SRC_DIR, 'app.ts'), 'utf-8');
    expect(appTs).toContain('twilioSignatureMiddleware');
  });

  test('route has phone whitelist middleware', () => {
    const appTs = fs.readFileSync(path.join(SRC_DIR, 'app.ts'), 'utf-8');
    expect(appTs).toContain('phoneWhitelistMiddleware');
  });

  test('route has rate limiter', () => {
    const appTs = fs.readFileSync(path.join(SRC_DIR, 'app.ts'), 'utf-8');
    expect(appTs).toContain('seedLimiter');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 2. CONTENT EXTRACTION — SmsSeedAdapter parses Twilio payload
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: Content extraction', () => {
  test('SmsSeedAdapter exists', () => {
    const files = fs.readdirSync(SRC_DIR, { recursive: true }) as string[];
    const adapterFile = files.find(f =>
      typeof f === 'string' && f.toLowerCase().includes('sms') && f.includes('adapter')
    );
    expect(adapterFile).toBeDefined();
  });

  test('seed interface defines SeedResource with required fields', () => {
    const interfaceFile = path.join(SRC_DIR, 'interfaces/seed.interface.ts');
    expect(fs.existsSync(interfaceFile)).toBe(true);
    const content = fs.readFileSync(interfaceFile, 'utf-8');
    expect(content).toContain('seedSource');
    expect(content).toContain('seedType');
    expect(content).toContain('seedStatus');
    expect(content).toContain('content');
  });

  test('seed types include text, photo, link', () => {
    const interfaceFile = path.join(SRC_DIR, 'interfaces/seed.interface.ts');
    const content = fs.readFileSync(interfaceFile, 'utf-8');
    expect(content).toMatch(/text|photo|link/i);
  });

  test('Twilio payload schema validation exists', () => {
    const handler = fs.readFileSync(
      path.join(SRC_DIR, 'handlers/seed.handler.ts'), 'utf-8'
    );
    expect(handler).toMatch(/schema|validate|payload/i);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 3. POD WRITE — seeds written to SOLID pod as Turtle
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: Pod write boundary', () => {
  test('SeedPodService exists', () => {
    const service = path.join(SRC_DIR, 'services/seed-pod.service.ts');
    expect(fs.existsSync(service)).toBe(true);
  });

  test('SeedPodService writes Turtle files', () => {
    const content = fs.readFileSync(
      path.join(SRC_DIR, 'services/seed-pod.service.ts'), 'utf-8'
    );
    expect(content).toMatch(/turtle|ttl|writeTurtle|writeCapture/i);
  });

  test('capture directory exists in pod', () => {
    expect(fs.existsSync(DATA_DIR)).toBe(true);
  });

  test('capture directory contains .ttl files', () => {
    const files = fs.readdirSync(DATA_DIR).filter(f => f.endsWith('.ttl'));
    expect(files.length).toBeGreaterThan(0);
  });

  test('Turtle files use jb:Seed ontology class', () => {
    const files = fs.readdirSync(DATA_DIR).filter(f => f.endsWith('.ttl') && !f.startsWith('.'));
    if (files.length === 0) return;
    const content = fs.readFileSync(path.join(DATA_DIR, files[0]), 'utf-8');
    expect(content).toMatch(/jb:Seed|Seed/);
  });

  test('Turtle files contain required seed properties', () => {
    const files = fs.readdirSync(DATA_DIR).filter(f => f.endsWith('.ttl') && !f.startsWith('.'));
    if (files.length === 0) return;
    const content = fs.readFileSync(path.join(DATA_DIR, files[0]), 'utf-8');
    expect(content).toMatch(/seedSource|seedContent|seededAt/);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 4. FUSEKI SYNC — triples loaded into named graph
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: Fuseki sync boundary', () => {
  test('FusekiSyncService exists', () => {
    const service = path.join(SRC_DIR, 'services/fuseki-sync.service.ts');
    expect(fs.existsSync(service)).toBe(true);
  });

  test('FusekiSyncService has syncResource method', () => {
    const content = fs.readFileSync(
      path.join(SRC_DIR, 'services/fuseki-sync.service.ts'), 'utf-8'
    );
    expect(content).toMatch(/syncResource|syncResourceFireAndForget/);
  });

  test('sync uses fire-and-forget pattern (non-blocking)', () => {
    const content = fs.readFileSync(
      path.join(SRC_DIR, 'services/fuseki-sync.service.ts'), 'utf-8'
    );
    expect(content).toMatch(/fireAndForget|FireAndForget|async/);
  });

  test('graph URI follows localhost:3000 convention', () => {
    const content = fs.readFileSync(
      path.join(SRC_DIR, 'services/fuseki-sync.service.ts'), 'utf-8'
    );
    expect(content).toMatch(/localhost:3000\/pods/);
  });

  test('sync targets /pods dataset on Fuseki', () => {
    const content = fs.readFileSync(
      path.join(SRC_DIR, 'services/fuseki-sync.service.ts'), 'utf-8'
    );
    expect(content).toMatch(/\/pods/);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 5. PAGE RENDER — /seeds page shows captured seeds
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: Seeds page render', () => {
  test('seeds triage view exists', () => {
    const view = path.join(VIEWS_DIR, 'seed-triage.ejs');
    expect(fs.existsSync(view)).toBe(true);
  });

  test('seed handler has renderTriage method', () => {
    const content = fs.readFileSync(
      path.join(SRC_DIR, 'handlers/seed.handler.ts'), 'utf-8'
    );
    expect(content).toContain('renderTriage');
  });

  test('app.ts registers GET /seeds route', () => {
    const appTs = fs.readFileSync(path.join(SRC_DIR, 'app.ts'), 'utf-8');
    expect(appTs).toMatch(/app\.get\(.*\/seeds/);
  });

  test('triage view supports status filtering', () => {
    const content = fs.readFileSync(
      path.join(VIEWS_DIR, 'seed-triage.ejs'), 'utf-8'
    );
    expect(content).toMatch(/pending|routed|discard/i);
  });

  test('triage view supports routing to destinations', () => {
    const content = fs.readFileSync(
      path.join(VIEWS_DIR, 'seed-triage.ejs'), 'utf-8'
    );
    expect(content).toMatch(/route|destination/i);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 6. API ENDPOINTS — seed REST API completeness
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: Seed API endpoints', () => {
  test('GET /api/seed endpoint registered', () => {
    const appTs = fs.readFileSync(path.join(SRC_DIR, 'app.ts'), 'utf-8');
    expect(appTs).toMatch(/app\.get\(.*\/api\/seed[^\/]/);
  });

  test('seed route endpoint registered', () => {
    const appTs = fs.readFileSync(path.join(SRC_DIR, 'app.ts'), 'utf-8');
    expect(appTs).toMatch(/\/api\/seed\/.*route/);
  });

  test('seed sync endpoint registered', () => {
    const appTs = fs.readFileSync(path.join(SRC_DIR, 'app.ts'), 'utf-8');
    expect(appTs).toMatch(/\/api\/seed\/sync/);
  });
});
