// @test-type: unit — pure parsing functions, no fs, no services.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { parseSetFolder, getTagFromParent } from '../lib/parser.js';

describe('parseSetFolder', () => {
  it('parses solo-f with site and numeric id', () => {
    const r = parseSetFolder('sabrina-banks-solo-f-nubiles-porn-1_cutie-pie_1200');
    assert.equal(r.name, 'sabrina-banks');
    assert.equal(r.category, 'solo-f');
    assert.equal(r.site, 'nubiles-porn');
  });

  it('parses fm with site', () => {
    const r = parseSetFolder('kimberly-brix-michael-williams-fm-blacked-100171-KIMBERLY-BRIX');
    assert.equal(r.name, 'kimberly-brix-michael-williams');
    assert.equal(r.category, 'fm');
    assert.equal(r.site, 'blacked');
  });

  it('parses ffm with site', () => {
    const r = parseSetFolder('kimmy-kimm-lulu-chu-jax-slayher-ffm-deeper-103858');
    assert.equal(r.name, 'kimmy-kimm-lulu-chu-jax-slayher');
    assert.equal(r.category, 'ffm');
    assert.equal(r.site, 'deeper');
  });

  it('parses mm with site', () => {
    const r = parseSetFolder('kenzo-alvarez-upton-sterling-mm-falcon-147671');
    assert.equal(r.name, 'kenzo-alvarez-upton-sterling');
    assert.equal(r.category, 'mm');
    assert.equal(r.site, 'falcon');
  });

  it('parses solo-m', () => {
    const r = parseSetFolder('kyle-quinn-solo-m-falcon-18219');
    assert.equal(r.name, 'kyle-quinn');
    assert.equal(r.category, 'solo-m');
    assert.equal(r.site, 'falcon');
  });

  it('parses pov-blowjob-fm', () => {
    const r = parseSetFolder('jane-doe-pov-blowjob-fm-bangbros-12345');
    assert.equal(r.name, 'jane-doe');
    assert.equal(r.category, 'pov-blowjob-fm');
    assert.equal(r.site, 'bangbros');
  });

  it('parses multi-word site names', () => {
    const r = parseSetFolder('karen-brennan-solo-f-penthouse-1990-08');
    assert.equal(r.name, 'karen-brennan');
    assert.equal(r.category, 'solo-f');
    assert.equal(r.site, 'penthouse');
  });

  it('parses site with hyphen like digital-desire', () => {
    const r = parseSetFolder('kaiya-solo-f-digital-desire-115046');
    assert.equal(r.name, 'kaiya');
    assert.equal(r.category, 'solo-f');
    assert.equal(r.site, 'digital-desire');
  });

  it('parses watch4beauty (starts with letter)', () => {
    const r = parseSetFolder('why-effy-solo-f-watch4beauty-20260321-max');
    assert.equal(r.name, 'why-effy');
    assert.equal(r.category, 'solo-f');
    assert.equal(r.site, 'watch4beauty');
  });

  it('parses fmm with multi-part names', () => {
    const r = parseSetFolder('kimberly-brix-jessy-jones-markus-dupree-fmm-tushy-100220-KIMBERLY-BRIX-THREESOME');
    assert.equal(r.name, 'kimberly-brix-jessy-jones-markus-dupree');
    assert.equal(r.category, 'fmm');
    assert.equal(r.site, 'tushy');
  });

  it('returns unknown for unparseable folder names', () => {
    const r = parseSetFolder('random-folder-name');
    assert.equal(r.category, 'unknown');
    assert.equal(r.site, 'unknown');
  });

  it('handles folder with parentheses in name', () => {
    const r = parseSetFolder('kiki (brunette)-solo-f-penthouse-1978-04');
    assert.equal(r.name, 'kiki (brunette)');
    assert.equal(r.category, 'solo-f');
    assert.equal(r.site, 'penthouse');
  });

  it('parses fff category', () => {
    const r = parseSetFolder('kiki-caprice-silvie-fff-hegre-10000px');
    assert.equal(r.category, 'fff');
    assert.equal(r.site, 'hegre');
  });

  it('parses tf category', () => {
    const r = parseSetFolder('model-a-model-b-tf-trans-angels-12345');
    assert.equal(r.category, 'tf');
    assert.equal(r.site, 'trans-angels');
  });
});

describe('getTagFromParent', () => {
  it('returns 💄 for lipstick path', () => {
    assert.equal(getTagFromParent('/Volumes/VideosNew/photo sets - 💄/A/model'), '💄');
  });

  it('returns 🟠 for orange path', () => {
    assert.equal(getTagFromParent('/Volumes/VideosNew/photo sets - 🟠/B/model'), '🟠');
  });

  it('returns 🟢 for green path', () => {
    assert.equal(getTagFromParent('/Volumes/VideosNew/photo sets - 🟢/C/model'), '🟢');
  });

  it('returns 🟣 for purple path', () => {
    assert.equal(getTagFromParent('/Volumes/VideosNew/photo sets - 🟣/D/model'), '🟣');
  });

  it('returns unknown for unrecognized path', () => {
    assert.equal(getTagFromParent('/Volumes/VideosNew/video/something'), 'unknown');
  });
});
