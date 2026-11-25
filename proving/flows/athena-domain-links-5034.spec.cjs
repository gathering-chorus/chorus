// @test-type: e2e:ui — playwright: every link on the Athena domain page and its model view leads somewhere
// @domain: tests
/**
 * #5034 AC2 — Jeff's phone, 2026-11-24: on the principles pages the vocabulary link answered
 * 404, the design doc link answered 404, and the model view said "Principle — Not served"
 * while the door held 14 principles. A reader who taps a link must land on something.
 *
 * Every same-origin link on both pages is followed:
 *   - the ops view   /athena/domain.html?d=<domain>
 *   - the model view /domains/<domain>, including one card per instance
 * The model view is one template that answers 200 for any name, so those links are opened
 * and read, never trusted on status (lib/athena-page-5034.cjs deadLinkReason).
 *
 * Page code this suite opens and reads, named so a change to any of it runs this suite:
 *   platform/api/public/athena/domain.html, platform/api/public/athena/domains-view.html,
 *   platform/api/public/athena/model-fold.js (the instance cards and their links).
 *
 * RUN: ATHENA_URL=http://localhost:3340 npx playwright test proving/flows/athena-domain-links-5034.spec.cjs
 */
const { test, expect } = require('@playwright/test');
const { BASE, DOMAIN, openDomainPage, openModelView, sameOriginLinks, deadLinks } = require('./lib/athena-page-5034.cjs');

test.describe(`#5034 every link on the ${DOMAIN} domain pages leads somewhere`, () => {
  test.setTimeout(240_000);

  test('the ops view has no dead link', async ({ page }) => {
    await openDomainPage(page);
    const links = await sameOriginLinks(page);
    expect(links.length, 'the page drew no links at all').toBeGreaterThan(5);
    const dead = await deadLinks(page, links);
    expect(dead, `dead links on the ops view:\n${dead.join('\n')}`).toEqual([]);
  });

  // Whether the model view says the right thing about each class (served or not, a card per
  // row) is the true-data suite's question (athena-domain-true-5034). Here: every link drawn.
  test('the model view has no dead link, instance cards included', async ({ page }) => {
    await openModelView(page);
    const links = await sameOriginLinks(page);
    expect(links.length, 'the model view drew no links at all').toBeGreaterThan(5);
    const dead = await deadLinks(page, links);
    expect(dead, `dead links on the model view:\n${dead.join('\n')}`).toEqual([]);
  });

  test('NEGATIVE: a dead link and a link to a name the model does not hold are both named', async ({ page }) => {
    await openDomainPage(page);
    const planted = [`${BASE}/owl/no-such-collection-5034`, `${BASE}/domains/${DOMAIN}/no-such-instance-5034`, `${BASE}/athena/no-such-page-5034.html`];
    const dead = await deadLinks(page, planted);
    expect(dead.length, `all three planted links must be reported, got:\n${dead.join('\n')}`).toBe(3);
    // and a live one is not: the check can tell the two apart
    expect(await deadLinks(page, [`${BASE}/domains/${DOMAIN}`, `${BASE}/athena/domains.html`])).toEqual([]);
  });
});
