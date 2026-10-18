// @test-type: unit
// @card: #4597
// @owner: abby-normal
// @domain: messages

import { formatRoomText, renderRoomMessage } from '../src/views';

describe('#4597: Clearing v2 message formatting (line breaks, links, bullets, bold, code blocks)', () => {
  describe('SSR formatRoomText (src/views.ts)', () => {
    test('AC1: Line breaks and blank lines are preserved (#4597)', () => {
      const input = 'First line\nSecond line\n\nAfter blank line';
      const html = formatRoomText(input);
      expect(html).toContain('First line');
      expect(html).toContain('Second line');
      expect(html).toContain('After blank line');
      expect(html).toMatch(/First line.*<br>.*Second line.*<br>.*<br>.*After blank line/s);
    });

    test('AC2: http(s) links are clickable and open in a new tab (#4597)', () => {
      const input = 'Check https://example.com/test and http://foo.org/bar for info.';
      const html = formatRoomText(input);
      expect(html).toContain('<a href="https://example.com/test" target="_blank" rel="noopener noreferrer">https://example.com/test</a>');
      expect(html).toContain('<a href="http://foo.org/bar" target="_blank" rel="noopener noreferrer">http://foo.org/bar</a>');
    });

    test('AC2 trailing punctuation: URL stops before trailing period or comma (#4597)', () => {
      const input = 'Visit https://lightlifeurbangardens.com, or https://example.com.';
      const html = formatRoomText(input);
      expect(html).toContain('href="https://lightlifeurbangardens.com"');
      expect(html).toContain('href="https://example.com"');
      expect(html).not.toContain('href="https://example.com."');
      expect(html).not.toContain('href="https://lightlifeurbangardens.com,"');
    });

    test('AC3: Bullet and numbered lists render as lists, bold as strong (#4597)', () => {
      const input = '**Important:**\n- item one\n- item two\n1. first\n2. second';
      const html = formatRoomText(input);
      expect(html).toContain('<strong>Important:</strong>');
      expect(html).toContain('<ul>');
      expect(html).toContain('<li>item one</li>');
      expect(html).toContain('<li>item two</li>');
      expect(html).toContain('</ul>');
      expect(html).toContain('<ol>');
      expect(html).toContain('<li>first</li>');
      expect(html).toContain('<li>second</li>');
      expect(html).toContain('</ol>');
    });

    test('AC4: Code fences render as monospace block (<pre><code>) (#4597)', () => {
      const input = 'Here is code:\n```bash\ngit status -s\necho done\n```\nAfter code.';
      const html = formatRoomText(input);
      expect(html).toContain('<pre><code class="language-bash">git status -s\necho done</code></pre>');
    });

    test('AC5 Negative proof: script tag shows as text, never executes (#3734 negative proof)', () => {
      const input = '<script>alert("pwned")</script> <img src=x onerror=alert(1)>';
      const html = formatRoomText(input);
      expect(html).not.toContain('<script>');
      expect(html).not.toContain('<img');
      expect(html).toContain('&lt;script&gt;alert(&quot;pwned&quot;)&lt;/script&gt;');
      expect(html).toContain('&lt;img src=x onerror=alert(1)&gt;');
    });
  });

  describe('CSR renderRoomMessage integration with app.js parity', () => {
    test('AC1 & AC2: renderRoomMessage preserves line breaks and clickable URLs (#4597)', () => {
      const rendered = renderRoomMessage({
        id: 'msg-links-1',
        from: 'wren',
        text: 'Hello world\nCheck https://github.com/test for updates\n\nBye',
        time: '12:00',
        tick: true,
      });
      expect(rendered).toContain('id="msg-msg-links-1"');
      expect(rendered).toContain('href="https://github.com/test"');
      expect(rendered).toContain('target="_blank"');
      expect(rendered).toContain('<br>');
    });

    test('AC3: renderRoomMessage handles bullet lists and bold (#4597)', () => {
      const rendered = renderRoomMessage({
        id: 'msg-list-1',
        from: 'silas',
        text: '**Title**\n- first item\n- second item',
        time: '12:01',
        tick: true,
      });
      expect(rendered).toContain('<strong>Title</strong>');
      expect(rendered).toContain('<ul>');
      expect(rendered).toContain('<li>first item</li>');
      expect(rendered).toContain('<li>second item</li>');
    });

    test('AC4: renderRoomMessage handles code fences (<pre><code>) (#4597)', () => {
      const rendered = renderRoomMessage({
        id: 'msg-code-1',
        from: 'kade',
        text: '```bash\ncargo test\n```',
        time: '12:02',
        tick: true,
      });
      expect(rendered).toContain('<pre>');
      expect(rendered).toContain('<code class="language-bash">cargo test</code>');
    });

    test('AC5 Negative proof: renderRoomMessage escapes script tags (#3734 negative proof)', () => {
      const rendered = renderRoomMessage({
        id: 'msg-xss-1',
        from: 'jeff',
        text: '<script>alert("attack")</script>',
        time: '12:03',
        tick: true,
      });
      expect(rendered).not.toContain('<script>alert');
      expect(rendered).toContain('&lt;script&gt;alert(&quot;attack&quot;)&lt;/script&gt;');
    });
  });
});
