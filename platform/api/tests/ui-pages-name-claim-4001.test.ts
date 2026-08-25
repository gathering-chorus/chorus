// @test-type: unit
/**
 * #4001 — a page named after a product belongs to that product.
 *
 * Jeff, 2026-08-25, looking at the 48 unclaimed pages: "where its clear lets
 * map them". /chorus-pages/loom.html was unclaimed for one reason — it is not
 * under a /loom/ folder. That is a fact about the directory tree, not about the
 * page, and it left real product pages unreachable from their own tile.
 *
 * The rule added is deliberately narrow, and these tests grade the narrowness
 * as hard as they grade the mapping: a page the rule CANNOT read stays
 * unclaimed. An unclaimed page is a visible decision waiting to be made; a page
 * guessed onto the wrong tile is a wrong answer presented as a right one.
 */
import { ownerOf } from '../src/handlers/ui-pages';

const PRODUCTS = ['Loom', 'Athena', 'Convergence', 'Werk', 'The Clearing', 'Borg', 'Gathering'];
const page = (href: string, dir = ''): { href: string; dir: string; title: string } =>
  ({ href, dir, title: href });

describe('#4001 name-based claiming', () => {
  test('a page named for a product is claimed by it, folder notwithstanding', () => {
    expect(ownerOf(page('/chorus-pages/loom.html', 'chorus-pages'), PRODUCTS)).toBe('Loom');
    expect(ownerOf(page('/chorus-pages/werk.html', 'chorus-pages'), PRODUCTS)).toBe('Werk');
  });

  test('the first hyphen-segment counts, so a qualified name still lands', () => {
    expect(ownerOf(page('/chorus-pages/borg-assessment.html', 'chorus-pages'), PRODUCTS)).toBe('Borg');
  });

  test('the folder still wins when it says something', () => {
    // A page under /athena/ named for another product stays with its folder —
    // the stronger signal, and the one that was already load-bearing.
    expect(ownerOf(page('/athena/werk-notes.html', 'athena'), PRODUCTS)).toBe('Athena');
  });

  // NEGATIVE PROOFS — the rule must REFUSE what it cannot read. Each of these
  // would be claimed by a looser rule (substring match, fuzzy match, "contains
  // a product word"), and each would put a page on a tile it does not belong to.
  test('a name the rule cannot read stays unclaimed', () => {
    expect(ownerOf(page('/doc-catalog.html'), PRODUCTS)).toBeNull();
    expect(ownerOf(page('/business-plan.html'), PRODUCTS)).toBeNull();
    expect(ownerOf(page('/model-data-hub.html'), PRODUCTS)).toBeNull();
  });

  test('a product name buried mid-name does not claim the page', () => {
    // "gathering-chorus" is about the seam, not owned by Gathering by naming.
    // A substring rule would claim it; this one must not.
    expect(ownerOf(page('/gathering-chorus.html'), PRODUCTS)).toBe('Gathering');
    expect(ownerOf(page('/chorus-gathering.html'), PRODUCTS)).toBeNull();
  });

  test('index pages never claim by name', () => {
    // Every folder has one; claiming by "index" would sweep the whole tree.
    expect(ownerOf(page('/index.html'), PRODUCTS)).toBeNull();
    expect(ownerOf(page('/book/index.html', 'book'), PRODUCTS)).toBeNull();
  });
});
