// @test-type: unit — Clearing v1 retirement send guard (#4877 AC8)
// @domain: messages
import fs from 'fs';
import path from 'path';

describe('Clearing v1 retirement guard (#4877 AC8)', () => {
  const v1Html = fs.readFileSync(path.join(__dirname, '../public/index.html'), 'utf8');

  it('refuses sending text in v1 composer and points user to exact v2 absolute URL', () => {
    expect(v1Html).toContain('Clearing v1 sending is retired');
    expect(v1Html).toContain('https://lightlifeurbangardens.com/');
    expect(v1Html).toMatch(/Use Clearing v2/);
  });

  it('refuses voice auto-send in v1 composer and points user to exact v2 absolute URL', () => {
    expect(v1Html).toContain('Clearing v1 voice sending is retired');
    expect(v1Html).toContain('https://lightlifeurbangardens.com/');
  });
});
