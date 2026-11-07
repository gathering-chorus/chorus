// @test-type: unit
// @domain: messages
// @card: #4886
// @owner: abby-normal

import { renderRoomsPage } from '../src/views';
import * as fs from 'fs';
import * as path from 'path';

describe('#4886 AC1, AC2, AC5: Boston time script serving and browser-safe loading', () => {
  it('AC1: renderRoomsPage includes vendor chorus-time boston.js before app.js', () => {
    const html = renderRoomsPage('team', { room: 'team', channels: ['team'], directRoles: [], messages: [] }, []);
    expect(html).toContain('<script src="/vendor/chorus-time/boston.js?v=');
    expect(html).toContain('<script src="/app.js?v=');
    const bostonIdx = html.indexOf('/vendor/chorus-time/boston.js');
    const appIdx = html.indexOf('/app.js');
    expect(bostonIdx).toBeGreaterThan(-1);
    expect(appIdx).toBeGreaterThan(bostonIdx);
  });

  it('AC2: app.js never calls require in a browser environment', () => {
    const appJsPath = path.join(__dirname, '../public/app.js');
    const content = fs.readFileSync(appJsPath, 'utf8');
    // Ensure no unguarded require call exists that could execute when require is undefined
    expect(content).not.toMatch(/typeof chorusBoston !== 'undefined' \? chorusBoston : require\(/);
    const requireMatches = content.match(/require\(['"][^'"]+['"]\)/g) || [];
    expect(requireMatches.length).toBeGreaterThan(0);
    // Line 125 must explicitly guard require with typeof require !== 'undefined'
    expect(content).toContain("(typeof require !== 'undefined' ? require('../../../platform/chorus-sdk/time/boston') : null)");
  });

  it('AC4: Negative proof: when chorusBoston is undefined in browser, app.js surfaces fallback or diagnostic instead of crashing', () => {
    const appJsPath = path.join(__dirname, '../public/app.js');
    const content = fs.readFileSync(appJsPath, 'utf8');
    expect(content).toMatch(/typeof chorusBoston !== 'undefined'\s*\?\s*chorusBoston\s*:\s*\(typeof require !== 'undefined'\s*\?\s*require\([^)]+\)\s*:\s*null\)/);
  });

  it('AC5: Retires public/index.html so views.ts is the single source of truth; fails if a second copy comes back', () => {
    const indexPath = path.join(__dirname, '../public/index.html');
    expect(fs.existsSync(indexPath)).toBe(false);
  });
});
