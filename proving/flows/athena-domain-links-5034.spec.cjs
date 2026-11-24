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

  test('the model view serves every class the domain declares, with a card per instance', async ({ page }) => {
    await openModelView(page);
    const unserved = await page.evaluate(() => [...document.querySelectorAll('[data-mf="instances"] .mfnote, [data-mf="instances"] .mferr, .mfold > .mferr')]
      .map((n) => `${n.previousElementSibling?.tagName === 'H3' ? n.previousElementSibling.textContent + ': ' : ''}${n.textContent.trim().slice(0, 80)}`));
    expect(unserved, `the model view could not list instances:\n${unserved.join('\n')}`).toEqual([]);
    expect(await page.locator('a.mfcard').count(), 'no instance card was drawn').toBeGreaterThan(0);
  });

  test('the model view has no dead link, instance cards included', async ({ page }) => {
    await openModelView(page);
    const links = await sameOriginLinks(page);
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
