/**
 * ONE QUERY, ONE TRUTH — the page inventory, asked exactly once (#4001).
 *
 * The hub and the Archive both need to know which pages on disk no product
 * claims. When each built that question for itself they disagreed the same day:
 * the Archive omitted `?products=`, so nothing was claimed and it listed 77
 * where the hub counted 47. Same endpoint, different question, two numbers for
 * one fact — the shape Silas named on 2026-08-25 as the one his hollow coverage
 * check had too.
 *
 * A shared SOURCE is not enough; what has to be shared is the CALL. Both pages
 * import this and neither re-derives the product list, so a change to the
 * question lands on both surfaces at once or on neither.
 */
(function (global) {
  const bp = (p) => (global.basePath ? global.basePath(p) : p);

  /**
   * Fetch products and the page inventory as one operation.
   * Returns { products, labels, inventory } — inventory shaped
   * { claimed, misc, total, miscCount } exactly as the API answers it.
   * Throws on a bad response: a caller that renders "nothing unclaimed" from a
   * failed fetch is reporting the opposite of what happened.
   */
  async function loadPageInventory() {
    const pRes = await fetch(bp('/owl/products'));
    if (!pRes.ok) throw new Error('products answered ' + pRes.status);
    const products = (await pRes.json()).data || [];
    // The hub card for the parent product is not a product tile; excluded here
    // so both surfaces exclude it identically.
    const labels = products.map((p) => p.label).filter((l) => l !== 'Chorus (Product)');

    const iRes = await fetch(bp('/api/chorus/ui-pages') + '?products=' + encodeURIComponent(labels.join(',')));
    if (!iRes.ok) throw new Error('inventory answered ' + iRes.status);
    return { products, labels, inventory: await iRes.json() };
  }

  global.loadPageInventory = loadPageInventory;
})(window);
