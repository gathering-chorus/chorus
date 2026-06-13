/**
 * #3361 AC5 — CI grep-gate: chorus pages served from chorus-api must not link
 * back to gathering's :3000 via href/src/action/fetch. RDF graph-URIs shown as
 * display text and documented CORS config are NOT page links and are allowed.
 * Fails loud if a moved view (re)introduces a cross-origin :3000 page/api ref.
 */
import * as fs from 'fs';
import * as path from 'path';

const VIEWS = path.join(__dirname, '..', 'views');
const FORBIDDEN = /(href|src|action)=["']https?:\/\/localhost:3000|fetch\(["']https?:\/\/localhost:3000/;

function walk(dir: string): string[] {
  const out: string[] = [];
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) out.push(...walk(p));
    else if (e.name.endsWith('.ejs')) out.push(p);
  }
  return out;
}

describe('#3361 AC5 — no cross-origin :3000 page/api refs in chorus views', () => {
  test('no moved view links to localhost:3000 via href/src/action/fetch', () => {
    const offenders: string[] = [];
    for (const f of walk(VIEWS)) {
      const lines = fs.readFileSync(f, 'utf8').split('\n');
      lines.forEach((ln, i) => {
        if (FORBIDDEN.test(ln)) offenders.push(`${path.relative(VIEWS, f)}:${i + 1}`);
      });
    }
    expect(offenders).toEqual([]);
  });
});
