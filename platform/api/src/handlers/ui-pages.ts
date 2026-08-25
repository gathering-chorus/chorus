/* eslint-disable security/detect-non-literal-fs-filename, security/detect-object-injection --
 * Paths here are built by walking a directory we own (platform/api/public) with
 * fs.readdirSync results — never from a request. The only request-derived input
 * is the product-label list, which is compared, never used as a path or a key.
 * Object indexing is on a fixed entity table and on labels drawn from that same
 * compared list.
 */
/**
 * #3813 — the UI inventory. Every page that exists on disk, grouped by the
 * product that owns it, with everything else in one honest misc pile.
 *
 * Jeff's framing (2026-08-11): "its like cleaning the attic or the basement or
 * the garage — u dont even know what u have and u may not have time or energy
 * to go thru every box, so u make piles and one of them ends up being a misc
 * pile." The misc pile is not a failure state and this code does not try to
 * empty it. It makes the pile visible and countable so its size is a choice
 * rather than a surprise.
 *
 * DISCOVERED, never hand-listed. The landing page previously carried a
 * `pageLinks` object naming 8 links by hand out of 77 pages on disk — which is
 * why 69 pages were invisible from the front door. A hand-list cannot tell you
 * about the page you forgot; a walk of the directory can only tell you the
 * truth.
 */

import fs from 'fs';
import path from 'path';

export interface UiPage {
  /** Href relative to the site root, e.g. "/athena/model.html". */
  href: string;
  /** <title> if the file has one, else the filename stem. */
  title: string;
  /** Directory the page lives in, "" for the site root. */
  dir: string;
}

export interface UiInventory {
  /** Pages whose directory names a known product, keyed by product label. */
  claimed: Record<string, UiPage[]>;
  /** Everything else — the misc pile. Deliberately allowed to be large. */
  misc: UiPage[];
  total: number;
  claimedCount: number;
  miscCount: number;
}

/**
 * Which product owns a page, or null for the misc pile.
 *
 * The rule is deliberately dumb: the page's top-level directory must match a
 * known product name, case-insensitively. Anything else is misc — including
 * shared grab-bag folders like `chorus-pages/`, whose name tells you nothing
 * about ownership. A cleverer rule (keyword matching on filenames, say) would
 * put pages in piles nobody chose, and a wrong pile is worse than misc: misc
 * is honest about not knowing.
 */
export function ownerOf(page: UiPage, productLabels: string[]): string | null {
  const match = (candidate: string): string | undefined =>
    productLabels.find((label) => label.toLowerCase() === candidate);

  // Rule 1 — the folder. /athena/domains.html is Athena's, by where it lives.
  if (page.dir) {
    const byDir = match(page.dir.split('/')[0].toLowerCase());
    if (byDir) return byDir;
  }

  // Rule 2 — the NAME, when the folder said nothing (#4001, Jeff 2026-08-25:
  // "where its clear lets map them"). /chorus-pages/loom.html is Loom's page by
  // any reading; before this it sat in the unclaimed pile purely because it is
  // not under a /loom/ folder — a fact about the tree, not about the page. So a
  // file whose name STARTS with a product's name is that product's.
  //
  // Deliberately narrow: only the first hyphen-segment of the stem, and only an
  // exact label match. "borg-assessment" is Borg's; "doc-catalog" is nobody's
  // by name, and guessing it into Athena would be an opinion wearing a rule.
  // An ambiguous page stays unclaimed, which is a visible decision, not a loss.
  const stem = page.href.split('/').pop()?.replace(/\.html$/, '').toLowerCase() ?? '';
  if (!stem || stem === 'index') return null;
  return match(stem) ?? match(stem.split('-')[0]) ?? null;
}

/** Group discovered pages into the claimed piles and the misc pile. */
export function buildInventory(pages: UiPage[], productLabels: string[]): UiInventory {
  const claimed: Record<string, UiPage[]> = {};
  const misc: UiPage[] = [];

  for (const page of pages) {
    const owner = ownerOf(page, productLabels);
    if (owner === null) {
      misc.push(page);
      continue;
    }
    (claimed[owner] ??= []).push(page);
  }

  const byTitle = (a: UiPage, b: UiPage): number => a.title.localeCompare(b.title);
  for (const list of Object.values(claimed)) list.sort(byTitle);
  misc.sort(byTitle);

  const claimedCount = Object.values(claimed).reduce((n, list) => n + list.length, 0);
  return {
    claimed,
    misc,
    total: claimedCount + misc.length,
    claimedCount,
    miscCount: misc.length,
  };
}

/**
 * The page's <title>, or the filename stem when it has none.
 *
 * Reads a bounded prefix rather than the whole file — #3819 is the same shape
 * one size up (a handler that swallowed a 732MB log to find a few lines and
 * froze the event loop for seconds). A <title> that is not in the first 8KB of
 * an HTML file does not exist in any practical sense.
 */
export function readTitle(file: string, stem: string): string {
  let head: string;
  try {
    const fd = fs.openSync(file, 'r');
    try {
      const buf = Buffer.alloc(8192);
      const read = fs.readSync(fd, buf, 0, buf.length, 0);
      head = buf.subarray(0, read).toString('utf8');
    } finally {
      fs.closeSync(fd);
    }
  } catch {
    return stem;
  }
  const match = head.match(/<title[^>]*>([^<]*)<\/title>/i);
  const title = match?.[1].trim();
  // Titles come out of the file still encoded ("Attention &amp; Intensity").
  // The page escapes whatever it renders, so leaving them encoded would
  // double-escape and show Jeff the entity text. Decode at the boundary where
  // the value stops being HTML and becomes data.
  return title ? decodeEntities(title) : stem;
}

const ENTITIES: Record<string, string> = {
  '&amp;': '&', '&lt;': '<', '&gt;': '>', '&quot;': '"', '&#39;': "'", '&apos;': "'", '&nbsp;': ' ',
};

function decodeEntities(text: string): string {
  return text.replace(/&(?:amp|lt|gt|quot|#39|apos|nbsp);/g, (m) => ENTITIES[m] ?? m);
}

/**
 * Walk a directory tree for .html files. Returns pages with root-relative
 * hrefs. Symlinks are not followed — the tree is ours, and a link out of it
 * would surface something the walk never inspected.
 */
export function discoverPages(root: string): UiPage[] {
  const out: UiPage[] = [];

  const walk = (abs: string, rel: string): void => {
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(abs, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      if (entry.name.startsWith('.')) continue;
      if (entry.isSymbolicLink()) continue;
      const childAbs = path.join(abs, entry.name);
      const childRel = rel ? `${rel}/${entry.name}` : entry.name;
      if (entry.isDirectory()) {
        walk(childAbs, childRel);
        continue;
      }
      if (!entry.name.toLowerCase().endsWith('.html')) continue;
      const stem = entry.name.replace(/\.html$/i, '');
      out.push({
        href: `/${childRel}`,
        title: readTitle(childAbs, stem),
        dir: rel,
      });
    }
  };

  walk(root, '');
  return out;
}
