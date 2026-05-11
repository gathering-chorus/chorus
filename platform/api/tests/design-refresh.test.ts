/**
 * #2900 — chorus_design_refresh unit tests.
 *
 * Tests cover the pure functions in design-refresh.ts: validateTemplate,
 * extractCardRefs, regenerateReferences, regeneratePathToClose,
 * regenerateGaps, plus the full executeDesignRefresh integration with
 * mocked deps. Per DEC-1674 (TDD): tests describe Jeff's experience —
 * he says /design-refresh <name> and the cite-density layers update
 * with current card statuses without touching human-authored sections.
 */
import {
  validateTemplate,
  extractCardRefs,
  regenerateReferences,
  regeneratePathToClose,
  regenerateGaps,
  executeDesignRefresh,
  autoConform,
  DesignRefreshError,
  type CardStatus,
} from '../src/mcp/design-refresh';

const TEMPLATE_OK = `
<!DOCTYPE html>
<html><body>
<h1>Test Design</h1>
<div class="summary-block">
  <h2>At a Glance</h2>
  <pre class="mermaid">A --> B</pre>
  <table class="gap-and-next"><tr><th>Status</th></tr></table>
  <p class="invariants">One sentence.</p>
</div>
<h2>Overview</h2>
<p>Refers to #2891 and #2898.</p>
<h2 data-section="references">References</h2>
<ul>
<li>#2891 — observer.error</li>
<li>#2898 — design refresh</li>
<li>Some other ref</li>
</ul>
</body></html>
`;

const TEMPLATE_NO_SUMMARY = `<html><body><h2>Overview</h2></body></html>`;

const TEMPLATE_NO_MERMAID = `
<html><body>
<div class="summary-block">
  <table class="gap-and-next"></table>
  <p class="invariants">x</p>
</div>
<h2 data-section="references">R</h2>
</body></html>
`;

const TEMPLATE_NO_DATA_SECTIONS = `
<html><body>
<div class="summary-block">
  <pre class="mermaid">A</pre>
  <table class="gap-and-next"></table>
  <p class="invariants">x</p>
</div>
<h2>Overview</h2>
</body></html>
`;

describe('#2900 validateTemplate', () => {
  test('passes on canonical structure', () => {
    expect(() => validateTemplate(TEMPLATE_OK)).not.toThrow();
  });

  test('refuses summary-missing when no summary-block div present', () => {
    expect(() => validateTemplate(TEMPLATE_NO_SUMMARY)).toThrow(DesignRefreshError);
    try {
      validateTemplate(TEMPLATE_NO_SUMMARY);
    } catch (e) {
      expect((e as DesignRefreshError).reason).toBe('summary-missing');
      expect((e as DesignRefreshError).detail).toMatch(/summary-block/);
    }
  });

  test('refuses summary-missing when mermaid diagram absent in summary block', () => {
    expect(() => validateTemplate(TEMPLATE_NO_MERMAID)).toThrow(DesignRefreshError);
    try {
      validateTemplate(TEMPLATE_NO_MERMAID);
    } catch (e) {
      expect((e as DesignRefreshError).reason).toBe('summary-missing');
      expect((e as DesignRefreshError).detail).toMatch(/mermaid/);
    }
  });

  test('refuses template-violation when no data-section h2 present', () => {
    expect(() => validateTemplate(TEMPLATE_NO_DATA_SECTIONS)).toThrow(DesignRefreshError);
    try {
      validateTemplate(TEMPLATE_NO_DATA_SECTIONS);
    } catch (e) {
      expect((e as DesignRefreshError).reason).toBe('template-violation');
    }
  });
});

describe('#2900 extractCardRefs', () => {
  test('extracts deduplicated card refs in numeric order', () => {
    const html = '<p>See #2898 and #2891 and #2898 again. Also #1234.</p>';
    expect(extractCardRefs(html)).toEqual([1234, 2891, 2898]);
  });

  test('ignores #N where N is single digit (not a card)', () => {
    const html = '<p>Section #1 and section #2 vs card #2891.</p>';
    // Min length is 2 digits per CARD_REF_RE.
    expect(extractCardRefs(html)).toEqual([2891]);
  });

  test('returns empty array when no refs', () => {
    expect(extractCardRefs('<p>nothing here</p>')).toEqual([]);
  });
});

describe('#2900 regenerateReferences', () => {
  const statuses = new Map<number, CardStatus>([
    [2891, { card_id: 2891, status: 'Done' }],
    [2898, { card_id: 2898, status: 'Done' }],
  ]);

  test('appends (Status) to each card ref in references section', () => {
    const { html, changed } = regenerateReferences(TEMPLATE_OK, statuses);
    expect(changed).toBe(true);
    expect(html).toMatch(/#2891 \(Done\)/);
    expect(html).toMatch(/#2898 \(Done\)/);
  });

  test('idempotent: re-running does not stack tags', () => {
    const first = regenerateReferences(TEMPLATE_OK, statuses);
    const second = regenerateReferences(first.html, statuses);
    expect(second.html).toBe(first.html);
    expect(second.changed).toBe(false);
    // No "(Done) (Done)" stacking.
    expect(first.html).not.toMatch(/\(Done\)\s*\(Done\)/);
  });

  test('does not touch refs outside the references section', () => {
    const { html } = regenerateReferences(TEMPLATE_OK, statuses);
    // The Overview section says "Refers to #2891 and #2898"; those should stay clean.
    const overview = html.match(/<h2>Overview<\/h2>([\s\S]*?)<h2/i)?.[1] ?? '';
    expect(overview).toMatch(/#2891 and #2898/);
    expect(overview).not.toMatch(/#2891 \(Done\)/);
  });

  test('skips refs whose status is Unknown', () => {
    const partial = new Map<number, CardStatus>([
      [2891, { card_id: 2891, status: 'Done' }],
      [2898, { card_id: 2898, status: 'Unknown' }],
    ]);
    const { html } = regenerateReferences(TEMPLATE_OK, partial);
    expect(html).toMatch(/#2891 \(Done\)/);
    expect(html).not.toMatch(/#2898 \(Unknown\)/);
  });

  test('updates stale status tag in place', () => {
    const stale = TEMPLATE_OK.replace('#2891 — observer.error', '#2891 (Next) — observer.error');
    const { html, changed } = regenerateReferences(stale, statuses);
    expect(changed).toBe(true);
    expect(html).toMatch(/#2891 \(Done\)/);
    expect(html).not.toMatch(/#2891 \(Next\)/);
  });
});

describe('#2900 regeneratePathToClose', () => {
  const sectionHtml = `
<h2 data-section="path-to-close">Path to close</h2>
<ol>
<li>Manifest schema + writer (#2791).</li>
<li><strong>SHIPPED 2026-05-07.</strong> Working-tree recovery (#2779).</li>
<li>Plain item with no card ref.</li>
<li>#2899 future work.</li>
</ol>
<h2>Next section</h2>
`;
  const statuses = new Map<number, CardStatus>([
    [2791, { card_id: 2791, status: 'Done' }],
    [2779, { card_id: 2779, status: 'Done' }],
    [2899, { card_id: 2899, status: 'Later' }],
  ]);

  test('prepends SHIPPED tag to items whose card is Done', () => {
    const { html, changed } = regeneratePathToClose(sectionHtml, statuses);
    expect(changed).toBe(true);
    expect(html).toMatch(/<strong>SHIPPED\.<\/strong> Manifest schema/);
  });

  test('prepends LATER tag to items whose card is Later', () => {
    const { html } = regeneratePathToClose(sectionHtml, statuses);
    expect(html).toMatch(/<strong>LATER\.<\/strong> #2899/);
  });

  test('leaves items without card refs untouched', () => {
    const { html } = regeneratePathToClose(sectionHtml, statuses);
    expect(html).toMatch(/<li>Plain item with no card ref\.<\/li>/);
  });

  test('leaves items with hand-authored status indicators untouched', () => {
    // The second item already has "<strong>SHIPPED 2026-05-07.</strong>" — that's
    // richer than our generic SHIPPED tag, so don't duplicate or overwrite. This
    // is the "card or delete" doc discipline: hand-authored richer status wins.
    const { html } = regeneratePathToClose(sectionHtml, statuses);
    expect(html).toMatch(/<strong>SHIPPED 2026-05-07\.<\/strong> Working-tree recovery/);
    // No double tag.
    expect(html).not.toMatch(/<strong>SHIPPED\.<\/strong>\s*<strong>SHIPPED/);
  });
});

describe('#2900 regenerateGaps', () => {
  const sectionHtml = `
<h2 data-section="gaps">Gaps</h2>
<h3>Closed</h3>
<div class="gap">Manifest schema (#2791).</div>
<h3>Open</h3>
<div class="gap">cdhash unknown — see also #2734 and #2888.</div>
<h2>Next section</h2>
`;
  const statuses = new Map<number, CardStatus>([
    [2791, { card_id: 2791, status: 'Done' }],
    [2734, { card_id: 2734, status: 'Done' }],
    [2888, { card_id: 2888, status: 'Later' }],
  ]);

  test('appends current status to each card ref in gaps section', () => {
    const { html, changed } = regenerateGaps(sectionHtml, statuses);
    expect(changed).toBe(true);
    expect(html).toMatch(/#2791 \(Done\)/);
    expect(html).toMatch(/#2734 \(Done\)/);
    expect(html).toMatch(/#2888 \(Later\)/);
  });

  test('idempotent: re-running does not stack', () => {
    const first = regenerateGaps(sectionHtml, statuses);
    const second = regenerateGaps(first.html, statuses);
    expect(second.html).toBe(first.html);
    expect(first.html).not.toMatch(/\(Done\)\s*\(Done\)/);
  });
});

describe('#2900 autoConform', () => {
  const docWithoutSummary = `
<html><body>
<h1>Some Service Design</h1>
<p class="date">2026-05-11</p>
<h2>Overview</h2>
<p>Refs #2891.</p>
<h2>Path to close</h2>
<ol><li>Item #2891.</li></ol>
<h2>Gaps to close</h2>
<div class="gap">#2891 gap.</div>
<h2>References</h2>
<ul><li>#2891 — example.</li></ul>
</body></html>
`;

  const docWithSummary = `
<html><body>
<h1>Compliant</h1>
<p class="date">2026-05-11</p>
<div class="summary-block">
  <h2>At a Glance</h2>
  <pre class="mermaid">A</pre>
  <table class="gap-and-next"></table>
  <p class="invariants">x</p>
</div>
<h2 data-section="references">References</h2>
<ul><li>#2891</li></ul>
</body></html>
`;

  test('scaffolds summary-block when missing', () => {
    const { html, scaffolded } = autoConform(docWithoutSummary);
    expect(scaffolded).toContain('summary-block');
    expect(html).toMatch(/<div\s+class="summary-block"/);
    expect(html).toMatch(/<pre\s+class="mermaid"/);
    expect(html).toMatch(/class="gap-and-next"/);
    expect(html).toMatch(/class="invariants"/);
    // Summary block sits right after the date line (not at top of body).
    const idxDate = html.indexOf('<p class="date"');
    const idxSummary = html.indexOf('<div class="summary-block"');
    expect(idxSummary).toBeGreaterThan(idxDate);
  });

  test('adds data-section attribute to standard H2 names when missing', () => {
    const { html, scaffolded } = autoConform(docWithoutSummary);
    expect(scaffolded).toContain('path-to-close.data-section');
    expect(scaffolded).toContain('gaps.data-section');
    expect(scaffolded).toContain('references.data-section');
    expect(html).toMatch(/<h2\s+data-section="path-to-close">Path to close<\/h2>/);
    expect(html).toMatch(/<h2\s+data-section="gaps">Gaps to close<\/h2>/);
    expect(html).toMatch(/<h2\s+data-section="references">References<\/h2>/);
  });

  test('no-op on doc that is already template-compliant', () => {
    const { html, scaffolded } = autoConform(docWithSummary);
    expect(scaffolded).toEqual([]);
    expect(html).toBe(docWithSummary);
  });

  test('inserts placeholder tokens in non-mermaid sections (mermaid uses plain labels)', () => {
    const { html } = autoConform(docWithoutSummary);
    // Table + invariants get {{TOKEN}} placeholders.
    expect(html).toMatch(/\{\{INVARIANT_PARAGRAPH\}\}/);
    expect(html).toMatch(/\{\{STATUS_1\}\}/);
    // Mermaid uses plain-text labels — mermaid 10.x parses `{` inside node
    // brackets as a different node shape, so {{TOKEN}} breaks rendering.
    expect(html).toMatch(/A\[Actor or input\] --> B\[Domain surface\]/);
    expect(html).not.toMatch(/A\[\{\{/);
  });

  test('idempotent: re-running autoConform does not double-insert', () => {
    const once = autoConform(docWithoutSummary);
    const twice = autoConform(once.html);
    expect(twice.scaffolded).toEqual([]);
    expect(twice.html).toBe(once.html);
  });
});

describe('#2900 executeDesignRefresh integration', () => {
  function makeDeps(overrides: Partial<{
    fileMap: Record<string, string>;
    cardStatusMap: Record<number, { status: string; title: string }>;
    written: Map<string, string>;
    events: Array<{ event: string; fields: Record<string, unknown> }>;
  }> = {}) {
    const fileMap = overrides.fileMap ?? {
      '/designs/test-design.html': TEMPLATE_OK,
    };
    const cardStatusMap = overrides.cardStatusMap ?? {
      2891: { status: 'Done', title: 'observer.error' },
      2898: { status: 'Done', title: 'design refresh' },
    };
    const written = overrides.written ?? new Map<string, string>();
    const events = overrides.events ?? [];
    return {
      readFile: (p: string) => {
        if (!(p in fileMap)) throw new Error(`ENOENT: ${p}`);
        return fileMap[p];
      },
      writeFile: (p: string, content: string) => {
        written.set(p, content);
      },
      cardsPath: '/fake/cards',
      designsDir: '/designs',
      emit: (event: string, fields: Record<string, unknown>) => {
        events.push({ event, fields });
      },
      execFile: (async (_path: string, args: readonly string[]) => {
        if (args[0] === 'view' && args[2] === '--json') {
          const id = Number(args[1]);
          const c = cardStatusMap[id];
          if (!c) throw new Error('not found');
          return { stdout: JSON.stringify({ status: c.status, title: c.title }), stderr: '' };
        }
        return { stdout: '', stderr: '' };
      }) as never,
      _written: written,
      _events: events,
    };
  }

  test('full happy-path: validates, fetches, regenerates references, writes, emits', async () => {
    const deps = makeDeps();
    const result = await executeDesignRefresh(
      { role: 'silas', design_name: 'test-design' },
      deps,
    );
    expect(result.design_name).toBe('test-design');
    expect(result.cards_referenced).toEqual([2891, 2898]);
    expect(result.sections_regenerated).toContain('references');
    const written = deps._written.get('/designs/test-design.html');
    expect(written).toBeDefined();
    expect(written).toMatch(/#2891 \(Done\)/);
    expect(written).toMatch(/#2898 \(Done\)/);

    const started = deps._events.find((e) => e.event === 'design.refresh.started');
    expect(started).toBeDefined();
    const completed = deps._events.find((e) => e.event === 'design.refreshed');
    expect(completed).toBeDefined();
    expect(completed?.fields.cards_referenced).toContain('2891');
  });

  test('refuses with design-not-found when file missing', async () => {
    const deps = makeDeps({ fileMap: {} });
    await expect(
      executeDesignRefresh({ role: 'silas', design_name: 'missing' }, deps),
    ).rejects.toThrow(/design-not-found/);
    const failed = deps._events.find((e) => e.event === 'design.refresh.failed');
    expect(failed?.fields.reason).toBe('design-not-found');
  });

  test('auto-conforms when summary block absent (#2900 scope expansion)', async () => {
    // Doc has only Overview + a Path-to-close H2; no summary block, no data-section attrs.
    // With auto-conform, the MCP should scaffold the missing structure and proceed.
    const minimalDoc = `<html><body><h1>X</h1><p class="date">2026</p><h2>Overview</h2><p>Refs #2891.</p><h2>Path to close</h2><ol><li>Item.</li></ol></body></html>`;
    const written = new Map<string, string>();
    const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
    const deps = makeDeps({
      fileMap: { '/designs/minimal.html': minimalDoc },
      written,
      events,
    });
    const result = await executeDesignRefresh(
      { role: 'silas', design_name: 'minimal' },
      deps,
    );
    expect(result.design_name).toBe('minimal');
    const scaffolded = deps._events.find((e) => e.event === 'design.scaffold.inserted');
    expect(scaffolded).toBeDefined();
    expect(scaffolded?.fields.scaffolded).toMatch(/summary-block/);
    expect(scaffolded?.fields.scaffolded).toMatch(/path-to-close\.data-section/);
    // Doc was written with the scaffold.
    const final = deps._written.get('/designs/minimal.html')!;
    expect(final).toMatch(/<div\s+class="summary-block"/);
  });

  test('idempotent: running twice produces identical output', async () => {
    const deps1 = makeDeps();
    await executeDesignRefresh({ role: 'silas', design_name: 'test-design' }, deps1);
    const firstOutput = deps1._written.get('/designs/test-design.html')!;

    const deps2 = makeDeps({
      fileMap: { '/designs/test-design.html': firstOutput },
    });
    await executeDesignRefresh({ role: 'silas', design_name: 'test-design' }, deps2);
    // Second run finds nothing to change (status already current).
    // Either no write happens, or the written content equals the input.
    const secondOutput = deps2._written.get('/designs/test-design.html');
    if (secondOutput !== undefined) {
      expect(secondOutput).toBe(firstOutput);
    }
  });
});
