// @test-type: unit — pure text→HTML rendering; brings its own world.
/**
 * #4220 — Jeff, 2026-09-19, reading Clearing on his phone:
 *
 *   "u may bullet things out but it comes to me as a big paragraph generally
 *    which makes it much harder for me to process"
 *
 * Two defects made the paragraph:
 *   1. `renderMarkdown` had no case for a ``` fence, so the fence markers and
 *      their contents fell through to the prose branch and merged into the
 *      surrounding sentence.
 *   2. Prose lines were joined with a literal "\n", which HTML collapses to a
 *      space. Only a BLANK line survived, as a single <br>.
 *
 * NEGATIVE PROOFS (#3734): both tests below are written against the state the
 * check exists to catch, and each was watched go RED against the old renderer
 * before this file landed.
 *
 * The functions live in the inline script of public/index.html, which is the
 * shipped artefact — so they are extracted from it rather than copied, or the
 * test would stop describing what Jeff actually sees.
 */
import { readFileSync } from 'fs';
import { join } from 'path';

function loadRenderer(): (t: string) => string {
  const html = readFileSync(join(__dirname, '..', 'public', 'index.html'), 'utf8');
  const grab = (name: string) => {
    const start = html.indexOf(`function ${name}(`);
    if (start < 0) throw new Error(`function ${name} not found in public/index.html`);
    let depth = 0;
    let i = html.indexOf('{', start);
    for (; i < html.length; i++) {
      if (html[i] === '{') depth++;
      else if (html[i] === '}' && --depth === 0) break;
    }
    return html.slice(start, i + 1);
  };
  const src = [grab('escapeHtml'), grab('linkify'), grab('inlineMd'), grab('renderMarkdown')].join('\n');
  const document = {
    createElement: () => {
      let value = '';
      return {
        set textContent(t: string) {
          value = t.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
        },
        get innerHTML() {
          return value;
        },
      };
    },
  };
  // eslint-disable-next-line no-new-func
  return new Function('document', `${src}; return renderMarkdown;`)(document);
}

const renderMarkdown = loadRenderer();

describe('#4220 Clearing renders like Claude Code, not like one paragraph', () => {
  it('NEGATIVE: a fenced block becomes a <pre>, not prose merged into the sentence', () => {
    const out = renderMarkdown('before\n```\nacl:agent  crawler 3\n```\nafter');
    expect(out).toContain('<pre');
    expect(out).toContain('acl:agent  crawler 3');
    expect(out).not.toContain('```');
  });

  it('NEGATIVE: two prose lines stay two lines', () => {
    const out = renderMarkdown('first line\nsecond line');
    expect(out).toContain('<br>');
    expect(out.indexOf('first line')).toBeLessThan(out.indexOf('<br>'));
  });

  it('bullets still render as a list, and get no stray <br>', () => {
    const out = renderMarkdown('- one\n- two');
    expect(out).toContain('<ul');
    expect(out).toContain('<li>one</li>');
    expect(out).toContain('<li>two</li>');
    expect(out).not.toContain('</li><br>');
    expect(out).not.toContain('<br><li>');
  });

  it('code inside a fence is not treated as markdown', () => {
    const out = renderMarkdown('```\n- not a bullet\n**not bold**\n```');
    expect(out).not.toContain('<li>');
    expect(out).not.toContain('<strong>not bold</strong>');
    expect(out).toContain('**not bold**');
  });

  it('an unterminated fence still renders as a block instead of swallowing the tail', () => {
    const out = renderMarkdown('intro\n```\ndangling line');
    expect(out).toContain('<pre');
    expect(out).toContain('dangling line');
  });

  it('a header remains a block and does not gain a <br> of its own', () => {
    const out = renderMarkdown('# Title\nbody line');
    expect(out).toContain('display:block');
    expect(out).not.toContain('</strong><br>');
  });
});
