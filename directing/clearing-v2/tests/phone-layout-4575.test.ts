// @test-type: unit — reads the page's CSS and renders the Roles view; no live services
// @domain: messages
// @card: #4575
// @owner: wren
// #4575 — Jeff's phone (10-17 06:09 photo, 06:23 "the send and url bar are challenging",
// 06:25 "the screen text floats off edges and resizes"). Each rule below is one of those.
import fs from 'fs';
import path from 'path';
import { renderRolesPage } from '../src/views';

const css = fs.readFileSync(path.join(__dirname, '..', 'public', 'style.css'), 'utf8');
const phone = css.slice(css.indexOf('@media (max-width: 480px)'));

function phoneRules(styles: string, phoneBlock: string): string[] {
  const missing: string[] = [];
  if (!/text-size-adjust:\s*100%/.test(styles)) missing.push('text-size-adjust');
  if (!/100dvh/.test(styles)) missing.push('dvh');
  if (!/env\(safe-area-inset-bottom/.test(styles)) missing.push('safe-area');
  if (!/#composer-input\s*\{[^}]*font-size:\s*16px/.test(phoneBlock)) missing.push('input-16px');
  if (!/overflow-wrap:\s*anywhere/.test(phoneBlock)) missing.push('wrap');
  return missing;
}

test('the phone CSS keeps Send visible, text unscaled, no zoom on focus, text inside the edges', () => {
  expect(phoneRules(css, phone)).toEqual([]);
});

test('negative proof: the old CSS (13.5px input, 100vh only) is caught', () => {
  const old = 'body{min-height:100vh}#composer-input{font-size:13.5px}';
  expect(phoneRules(old, '@media (max-width: 480px){#composer-input{font-size:13.5px}}')).toEqual(
    ['text-size-adjust', 'dvh', 'safe-area', 'input-16px', 'wrap']);
});

test('the page lets content reach the safe area (viewport-fit=cover)', () => {
  const html = renderRolesPage({ actors: [] } as never, []);
  expect(html).toContain('viewport-fit=cover');
});

test("a row with many sessions shows one plus a count, so it stays one line tall", () => {
  const actors = [{ id: 'jeff', name: 'Jeff', sessions: ['browser 09:50', 'browser 08:10', 'pane 06:00', 'pane 05:00'], stateValue: 'available', activity: 'acting' }];
  const html = renderRolesPage({ actors } as never, []);
  const cell = html.slice(html.indexOf('role-sessions'), html.indexOf('</td>', html.indexOf('role-sessions')));
  expect(cell).toContain('browser 09:50');
  expect(cell).toContain('+3');
  expect(cell).not.toContain('<br>');
});
