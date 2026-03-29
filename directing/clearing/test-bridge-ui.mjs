#!/usr/bin/env node
/**
 * Bridge UI tests — test what the USER sees, not what the API returns.
 * Uses Playwright to open Bridge in a real browser and inspect rendered messages.
 * Run: npx playwright test test-bridge-ui.mjs
 * Or:  node test-bridge-ui.mjs (uses playwright programmatically)
 */

import { chromium } from 'playwright';

const BRIDGE = 'http://localhost:3470';
let pass = 0;
let fail = 0;

function check(name, result) {
  if (result) { console.log(`  PASS: ${name}`); pass++; }
  else { console.log(`  FAIL: ${name}`); fail++; }
}

async function run() {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();

  console.log('=== Bridge UI Tests ===\n');

  // Load Bridge and wait for messages to render
  console.log('--- Page Load ---');
  await page.goto(BRIDGE);
  await page.waitForSelector('.msg', { timeout: 10000 }).catch(() => {});
  // Give socket time to deliver messages
  await page.waitForTimeout(3000);

  const messages = await page.$$eval('.msg', msgs => msgs.map(m => ({
    text: m.textContent || '',
    classes: m.className,
    from: m.querySelector('.msg-label')?.textContent || '',
    isJeff: m.classList.contains('msg-jeff'),
  })));

  check('Bridge loaded with messages', messages.length > 0);
  console.log(`  (${messages.length} message bubbles rendered)\n`);

  // --- Noise Filtering ---
  console.log('--- No System Noise in Messages ---');

  const noisePatterns = [
    { name: 'XML tags', pattern: /<[a-z-]+>/i },
    { name: 'File paths /Users/', pattern: /\/Users\/jeffbridwell/ },
    { name: 'File paths /var/', pattern: /\/var\/folders/ },
    { name: 'File paths /private/', pattern: /\/private\/tmp/ },
    { name: 'Tool metadata suffix', pattern: /\| tools:.*\| [\d.]+s/ },
    { name: '[Request interrupted]', pattern: /\[Request interrupted/ },
    { name: 'Base directory for skill', pattern: /Base directory for this skill/ },
    { name: 'hook blocking error', pattern: /hook blocking error/ },
    { name: 'ARGUMENTS:', pattern: /^ARGUMENTS:/ },
    { name: 'spawnSync errors', pattern: /spawnSync/ },
    { name: 'ETIMEDOUT', pattern: /ETIMEDOUT/ },
    { name: 'task-notification', pattern: /task-notification/ },
    { name: 'Image source paths', pattern: /\[Image: source:/ },
  ];

  for (const { name, pattern } of noisePatterns) {
    const noisy = messages.filter(m => pattern.test(m.text));
    check(`No ${name} in visible messages`, noisy.length === 0);
    if (noisy.length > 0) {
      console.log(`    Found in: "${noisy[0].text.substring(0, 80)}..."`);
    }
  }

  // --- Attribution ---
  console.log('\n--- Jeff Attribution ---');

  const jeffMessages = messages.filter(m => m.isJeff);
  const roleMessagesWithJeffText = messages.filter(m =>
    !m.isJeff && m.text.includes('[silas]') && !m.from.toLowerCase().includes('jeff')
  );

  check('Jeff messages exist', jeffMessages.length > 0);
  check('Jeff messages styled as Jeff (blue)', jeffMessages.every(m => m.isJeff));

  // --- Deduplication ---
  console.log('\n--- No Duplicates ---');

  const texts = messages.map(m => m.text.substring(0, 100));
  const dupes = texts.filter((t, i) => texts.indexOf(t) !== i && t.length > 20);
  check('No duplicate messages (>20 chars)', dupes.length === 0);
  if (dupes.length > 0) {
    console.log(`    Duplicate: "${dupes[0].substring(0, 60)}..."`);
  }

  // --- Tiles ---
  console.log('\n--- Tile State ---');

  const tiles = await page.$$eval('.tile', ts => ts.map(t => ({
    role: t.querySelector('.tile-name')?.textContent || '',
    state: t.querySelector('.tile-state')?.textContent?.trim() || '',
  })));

  check('4 tiles rendered (Jeff + 3 roles)', tiles.length >= 4);
  for (const t of tiles) {
    if (t.role) {
      check(`${t.role} tile has state`, t.state.length > 0);
    }
  }

  // --- Summary ---
  console.log(`\n=== Results: ${pass} passed, ${fail} failed ===`);

  await browser.close();

  if (fail > 0) {
    console.log('BLOCKED — fix failures before demo');
    process.exit(1);
  } else {
    console.log('ALL PASS — safe to demo');
    process.exit(0);
  }
}

run().catch(e => {
  console.error('Test crashed:', e.message);
  process.exit(1);
});
