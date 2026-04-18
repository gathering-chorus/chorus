/**
 * Domain section enrichment tests — #2178
 *
 * /api/chorus/domain/:name sections populated from urn:chorus:instances.
 * Pre-#2178: sections.<name> = { title, items: [label,...] }.
 * Post-#2178: same shape PLUS sections.<name>.itemDetails = [{ label, description?, reads?, writes?, consumes? }, ...].
 *
 * Backward-compat: existing consumers (envelope renderer, Clearing tiles,
 * domain-detail.html) iterate items as strings. itemDetails is additive.
 */

import { startTestApp, type TestApp } from './lib/test-app';

describe('#2178: domain section enrichment', () => {
  let harness: TestApp;

  beforeAll(async () => { harness = await startTestApp(); });
  afterAll(async () => { if (harness) await harness.close(); });

  test('sections.services.items is still a string array', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/chorus`);
    expect(res.status).toBe(200);
    const body = await res.json();
    const services = body.sections?.services;
    expect(services).toBeDefined();
    expect(Array.isArray(services.items)).toBe(true);
    expect(services.items.length).toBeGreaterThan(0);
    for (const item of services.items) expect(typeof item).toBe('string');
  }, 15_000);

  test('sections.services.itemDetails present with label per entity', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/chorus`);
    const body = await res.json();
    const services = body.sections?.services;
    expect(Array.isArray(services.itemDetails)).toBe(true);
    expect(services.itemDetails.length).toBe(services.items.length);
    for (const d of services.itemDetails) expect(typeof d.label).toBe('string');
  }, 15_000);

  test('Pulse itemDetail carries description + reads + writes', async () => {
    const res = await fetch(`${harness.baseUrl}/api/chorus/domain/chorus`);
    const body = await res.json();
    const pulse = body.sections?.services?.itemDetails?.find((d: any) => d.label === 'Pulse');
    expect(pulse).toBeDefined();
    expect(typeof pulse.description).toBe('string');
    expect(Array.isArray(pulse.reads)).toBe(true);
    expect(pulse.reads.length).toBeGreaterThan(0);
    expect(Array.isArray(pulse.writes)).toBe(true);
    expect(pulse.writes.length).toBeGreaterThan(0);
  }, 15_000);
});
