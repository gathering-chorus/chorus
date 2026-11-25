// @test-type: unit
// @domain: tests · @card: 4235 · owner: kade
// #4235 AC4 — the domain page shows the files the crawl tagged with the domain,
// read from the graph (CodeFile hasDomain), and never says "0 files" when the
// read failed. Static: the page script is read as text.
import fs from 'fs';
import path from 'path';

const page = fs.readFileSync(path.join(__dirname, '../public/athena/domain.html'), 'utf8');
const fn = page.slice(page.indexOf('async function renderFiles'), page.indexOf('// ---- the materialized view'));

describe('#4235 domain page Files fold', () => {
  it('is mounted and called for the domain', () => {
    expect(page).toContain('<div id="files-slot"></div>');
    expect(page).toContain('renderFiles(d);');
  });

  // #5040 — the test "reads the crawl's tags from the graph" is retired with the code it read.
  // It held the fold to a query POSTed to /sparql-read; the public sign-in door refuses a POST
  // (405 on Jeff's phone, 2026-11-25), so the fold now reads the model door's CodeFile
  // collection filtered on hasDomain. What that test covered is held, as drawn in a browser
  // through the real door, by proving/flows/athena-domain-folds-5034.spec.cjs ("the Services
  // and Files folds read through the door with no POST, and show what the store holds").

  it('NEGATIVE PROOF — a failed read shows an unknown count, never zero files', () => {
    const failure = fn.slice(fn.indexOf('} catch (e)'));
    expect(failure).toContain("fold('?'");
    expect(failure).not.toContain('fold(0');
  });
});
