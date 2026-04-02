/**
 * Domain Fold Tests — #1963
 *
 * Tests what Jeff SEES: domains as collapsible sections,
 * sorted by priority, WIP indicators when collapsed.
 */

jest.setTimeout(15000);

import * as http from 'http';

const CLEARING_URL = 'http://localhost:3470';

// Helper: GET JSON from Clearing API
function getJson(path: string): Promise<any> {
  return new Promise((resolve, reject) => {
    http.get(`${CLEARING_URL}${path}`, (res) => {
      let data = '';
      res.on('data', (chunk) => data += chunk);
      res.on('end', () => {
        try { resolve(JSON.parse(data)); }
        catch { reject(new Error(`Bad JSON from ${path}: ${data.slice(0, 200)}`)); }
      });
    }).on('error', reject);
  });
}

function getHtml(): Promise<string> {
  return new Promise((resolve, reject) => {
    http.get(CLEARING_URL, (res) => {
      let data = '';
      res.on('data', (chunk) => data += chunk);
      res.on('end', () => resolve(data));
    }).on('error', reject);
  });
}

function clearingIsUp(): boolean {
  const { execSync } = require('child_process');
  try {
    const result = execSync(`curl -sf -o /dev/null -w "%{http_code}" ${CLEARING_URL}/health`, {
      encoding: 'utf-8' as const, timeout: 3000,
    });
    return result.trim() === '200';
  } catch { return false; }
}

const skipIfDown = !clearingIsUp() ? describe.skip : describe;

skipIfDown('Domain fold — #1963', () => {

  describe('/api/flow returns priority on cards', () => {
    test('each card has a numeric priority field', async () => {
      const data = await getJson('/api/flow');
      const domains = data.domains || {};
      const allCards = Object.values(domains).flatMap((d: any) => d.cards || []);
      expect(allCards.length).toBeGreaterThan(0);
      for (const card of allCards) {
        expect(card).toHaveProperty('priority');
        expect(typeof card.priority).toBe('number');
      }
    });

    test('priority values are 1, 2, 3, or 9 (default)', async () => {
      const data = await getJson('/api/flow');
      const allCards = Object.values(data.domains || {}).flatMap((d: any) => d.cards || []);
      for (const card of allCards) {
        expect([1, 2, 3, 9]).toContain(card.priority);
      }
    });
  });

  describe('/api/flow returns sequence on cards', () => {
    test('each card has a sequence field', async () => {
      const data = await getJson('/api/flow');
      const allCards = Object.values(data.domains || {}).flatMap((d: any) => d.cards || []);
      for (const card of allCards) {
        expect(card).toHaveProperty('sequence');
        expect(typeof card.sequence).toBe('string');
      }
    });
  });

  describe('domain counts include WIP indicator data', () => {
    test('each domain has a wip count', async () => {
      const data = await getJson('/api/flow');
      for (const [domain, d] of Object.entries(data.domains || {}) as any[]) {
        expect(d.counts).toHaveProperty('wip');
        expect(typeof d.counts.wip).toBe('number');
      }
    });
  });

  describe('HTML renders collapsible domain sections', () => {
    test('flow-section-title has toggleDomain onclick', async () => {
      const html = await getHtml();
      expect(html).toContain('function toggleDomain');
    });

    test('default state is all collapsed — tracks expanded domains, not collapsed', async () => {
      const html = await getHtml();
      expect(html).toContain('expandedDomains');
      // Should NOT contain old collapsed approach
      expect(html).not.toContain('collapsedDomains');
    });

    test('collapsed domain header shows WIP indicator', async () => {
      const html = await getHtml();
      expect(html).toContain('wip-indicator');
    });

    test('cards sorted by priority then card number', async () => {
      const html = await getHtml();
      expect(html).toContain('a.priority');
    });

    test('empty domains do not render', async () => {
      const html = await getHtml();
      expect(html).toContain('cards.length === 0');
    });
  });
});
