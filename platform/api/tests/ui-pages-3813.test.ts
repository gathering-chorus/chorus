// @test-type: unit — walks a tmpdir tree it creates; no repo paths, no live service.
/**
 * #3813 — the UI inventory.
 *
 * Jeff's framing (2026-08-11): "its like cleaning the attic or the basement or
 * the garage — u dont even know what u have and u may not have time or energy
 * to go thru every box, so u make piles and one of them ends up being a misc
 * pile."
 *
 * So the thing under test is NOT that the misc pile is small. It is that the
 * inventory is COMPLETE and HONEST: every page on disk lands in exactly one
 * pile, and a page nobody claims is visibly unclaimed rather than quietly
 * dropped. A pile that silently loses boxes is worse than no pile — you would
 * stop looking for what it never showed you.
 */

import fs from 'fs';
import os from 'os';
import path from 'path';
import { discoverPages, buildInventory, ownerOf, readTitle } from '../src/handlers/ui-pages';

/** The test brings its own world (#3528) — a tree it created, never the repo's. */
function makeTree(files: Record<string, string>): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ui-pages-3813-'));
  for (const [rel, body] of Object.entries(files)) {
    const abs = path.join(root, rel);
    fs.mkdirSync(path.dirname(abs), { recursive: true });
    fs.writeFileSync(abs, body);
  }
  return root;
}

const PRODUCTS = ['Athena', 'Borg', 'Loom', 'Werk'];

describe('#3813 discovery', () => {
  test('finds every html page in the tree, at any depth', () => {
    const root = makeTree({
      'index.html': '<title>Home</title>',
      'athena/model.html': '<title>Model</title>',
      'borg/hooks/index.html': '<title>Hooks</title>',
      'notes.txt': 'not a page',
    });
    const found = discoverPages(root).map((p) => p.href).sort();
    expect(found).toEqual(['/athena/model.html', '/borg/hooks/index.html', '/index.html']);
  });

  test('uses the <title>, falling back to the filename when there is none', () => {
    const root = makeTree({
      'titled.html': '<html><head><title>  Real Title  </title></head>',
      'untitled.html': '<html><body>no head at all</body></html>',
    });
    const byHref = Object.fromEntries(discoverPages(root).map((p) => [p.href, p.title]));
    expect(byHref['/titled.html']).toBe('Real Title');
    expect(byHref['/untitled.html']).toBe('untitled');
  });

  test('NEGATIVE PROOF: a page added to the tree shows up without being listed anywhere', () => {
    // The defect this card exists to end: the landing page carried a
    // hand-written map of 8 links while 77 pages sat on disk, so a new page was
    // invisible until someone remembered to add it. If this test could pass
    // against a hardcoded list, it would not be testing discovery at all.
    const root = makeTree({ 'athena/model.html': '<title>Model</title>' });
    expect(discoverPages(root)).toHaveLength(1);
    fs.writeFileSync(path.join(root, 'athena', 'brand-new.html'), '<title>Brand New</title>');
    const after = discoverPages(root);
    expect(after).toHaveLength(2);
    expect(after.map((p) => p.title)).toContain('Brand New');
  });

  test('NEGATIVE PROOF: a deleted page disappears — the walk is not additive', () => {
    // A guard whose target is deleted must fail loudly, never pass vacuously.
    // If discovery kept returning a page after its file was gone, the tile
    // would link to a 404 and we would trust the list over the disk.
    const root = makeTree({ 'a.html': '<title>A</title>', 'b.html': '<title>B</title>' });
    fs.unlinkSync(path.join(root, 'b.html'));
    expect(discoverPages(root).map((p) => p.title)).toEqual(['A']);
  });

  test('does not follow symlinks out of the tree', () => {
    // A link out of our directory would put something on the page that the walk
    // never inspected — the "make the mess visible" card must not become a way
    // to surface files nobody reviewed.
    const outside = makeTree({ 'secret.html': '<title>Not Ours</title>' });
    const root = makeTree({ 'own.html': '<title>Own</title>' });
    fs.symlinkSync(outside, path.join(root, 'linked'));
    expect(discoverPages(root).map((p) => p.title)).toEqual(['Own']);
  });
});

describe('#3813 piles', () => {
  const page = (href: string, dir: string, title = href) => ({ href, dir, title });

  test('a page in a product folder is claimed by that product', () => {
    expect(ownerOf(page('/athena/model.html', 'athena'), PRODUCTS)).toBe('Athena');
    expect(ownerOf(page('/borg/hooks/index.html', 'borg/hooks'), PRODUCTS)).toBe('Borg');
  });

  test('a page at the root belongs to nobody', () => {
    expect(ownerOf(page('/business-plan.html', ''), PRODUCTS)).toBeNull();
  });

  test('a grab-bag folder is misc, not a product', () => {
    // chorus-pages/ holds nine pages and its name says nothing about who owns
    // them. Guessing would put pages in piles nobody chose, and a wrong pile is
    // worse than misc: misc is honest about not knowing.
    expect(ownerOf(page('/chorus-pages/icd.html', 'chorus-pages'), PRODUCTS)).toBeNull();
  });

  test('every page lands in exactly one pile and none are lost', () => {
    const pages = [
      page('/athena/model.html', 'athena'),
      page('/borg/hooks.html', 'borg'),
      page('/business-plan.html', ''),
      page('/chorus-pages/icd.html', 'chorus-pages'),
    ];
    const inv = buildInventory(pages, PRODUCTS);
    expect(inv.total).toBe(pages.length);
    expect(inv.claimedCount + inv.miscCount).toBe(pages.length);
    expect(inv.miscCount).toBe(2);
  });

  test('NEGATIVE PROOF: an unclaimed page is SHOWN as misc, never dropped', () => {
    // The failure that would make this card a lie: a tidy-looking page that
    // quietly omits what it could not classify. Then the mess is not visible,
    // it is hidden behind a page claiming to show it.
    const inv = buildInventory([page('/orphan.html', '')], PRODUCTS);
    expect(inv.total).toBe(1);
    expect(inv.misc.map((p) => p.href)).toEqual(['/orphan.html']);
  });

  test('NEGATIVE PROOF: the pile shrinks only when a page is really claimed', () => {
    // Proves miscCount tracks reality rather than being decorative: the SAME
    // page leaves misc exactly when its folder becomes a known product.
    const orphan = page('/newthing/page.html', 'newthing');
    expect(buildInventory([orphan], PRODUCTS).miscCount).toBe(1);
    expect(buildInventory([orphan], [...PRODUCTS, 'NewThing']).miscCount).toBe(0);
  });
});

describe('#3813 title reading is bounded', () => {
  test('a huge file does not get read whole to find its title', () => {
    // #3819 is this same shape one size up — a handler that swallowed a 732MB
    // log to find a few lines and froze the API for seconds. A title past the
    // first 8KB does not exist in any practical sense.
    const root = makeTree({ 'big.html': `${'<!-- pad -->'.repeat(2000)}<title>Too Late</title>` });
    expect(readTitle(path.join(root, 'big.html'), 'big')).toBe('big');
  });

  test('an unreadable file falls back to its filename instead of throwing', () => {
    expect(readTitle('/definitely/not/here.html', 'here')).toBe('here');
  });
});

describe('#3813 titles are data, not markup', () => {
  test('an encoded title is decoded once, not shown as entity text', () => {
    // Real case from the tree: "Attention &amp; Intensity — Analytics". The page
    // escapes what it renders, so a title left encoded here reaches Jeff as
    // "&amp;amp;" — the ampersand escaped twice.
    const root = makeTree({ 'a.html': '<title>Attention &amp; Intensity</title>' });
    expect(readTitle(path.join(root, 'a.html'), 'a')).toBe('Attention & Intensity');
  });

  test('NEGATIVE PROOF: decoding does not run twice', () => {
    // "&amp;amp;" in the source means the author wanted a literal "&amp;".
    // Decoding repeatedly until nothing changes would silently rewrite it.
    const root = makeTree({ 'b.html': '<title>&amp;amp;</title>' });
    expect(readTitle(path.join(root, 'b.html'), 'b')).toBe('&amp;');
  });
});
