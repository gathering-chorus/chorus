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

/**
 * #4230 — Jeff, 2026-09-20: "right side of readouts in courier is cropped".
 *
 * #4223 made fenced blocks render as <pre>. They were styled `white-space: pre`
 * with `overflow-x: auto`, so on a phone a line wider than the screen ran off
 * the right edge into a scrollbar inside an already-scrolling page. Every count
 * and every path we send him rides in a fence, so the half of a message that
 * carries the evidence was the half he could not read.
 *
 * NEGATIVE PROOF (#3734): the assertion is written against the state it exists
 * to catch — `white-space:pre` on the block — and was watched go RED against the
 * shipped file before the style changed.
 */
describe('#4234 — fenced blocks scroll sideways instead of wrapping', () => {
  const render = loadRenderer();
  const longLine = 'row-missing-required-field 16161 v1-row 15862 owner-not-principal 6783';
  const html = render('```\n' + longLine + '\n```');
  const preTag = html.slice(html.indexOf('<pre'), html.indexOf('>', html.indexOf('<pre')) + 1);

  it('renders the fence as a <pre> block', () => {
    expect(html).toContain('<pre');
    expect(html).toContain(longLine);
  });

  /**
   * Jeff, 2026-09-20: "the wrap on the others is not legible i dont know where
   * one row ends and the next begins" then "a horizontal scroll is better".
   * A row stays on ONE line; the block scrolls.
   */
  it('a row stays on one line — white-space is pre, never pre-wrap', () => {
    expect(/white-space:pre(?!-wrap)/.test(preTag)).toBe(true);
    expect(preTag).not.toContain('pre-wrap');
    expect(preTag).not.toContain('overflow-wrap:anywhere');
  });

  it('the block owns a sideways scroll that a thumb can reach', () => {
    expect(preTag).toContain('overflow-x:auto');
    expect(preTag).toContain('-webkit-overflow-scrolling:touch');
  });

  /**
   * NEGATIVE PROOF (#3734): the phone rule `.msg { overflow-x: hidden }` is what
   * made the old scroll unreachable — the block was scrollable in principle and
   * clipped in practice. Assert against the shipped stylesheet, not the markup.
   */
  it('the message row does not clip the block', () => {
    const { readFileSync } = require('fs');
    const { join } = require('path');
    const page = readFileSync(join(__dirname, '..', 'public', 'index.html'), 'utf8');
    expect(/\.msg \{[^}]*overflow-x:\s*hidden/.test(page)).toBe(false);
  });
});
