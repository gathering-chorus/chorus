/**
 * sdk.ts pure-function tests (#2241 wave 2).
 *
 * Covers the validation / classification helpers that don't need a
 * BoardClient. These gate behaviors are what users hit when running
 * `cards move`, `cards done`, etc. — describe Jeff-visible decisions, not
 * internal paths.
 */

import {
  isCodeCard,
  warnShortTitle,
  enforceNowDescriptionGate,
  enforceExperienceGate,
  enforceACGate,
  enforceTaxonomyGate,
} from '../src/sdk';

function silenceConsole() {
  const origLog = console.log;
  const origErr = console.error;
  console.log = () => {};
  console.error = () => {};
  return () => { console.log = origLog; console.error = origErr; };
}

describe('isCodeCard', () => {
  it('returns true for handler / refactor / sparql / endpoint phrasing', () => {
    expect(isCodeCard('Extract handler for subdomain completeness')).toBe(true);
    expect(isCodeCard('Refactor the SPARQL query loader')).toBe(true);
    expect(isCodeCard('Fix stale-timestamp test bug')).toBe(true);
  });

  it('returns false when explicit non-code tags are present', () => {
    expect(isCodeCard('[process] retrospective planning')).toBe(false);
    expect(isCodeCard('[docs] write a decision brief')).toBe(false);
    expect(isCodeCard('meeting to discuss spike research')).toBe(false);
  });

  it('non-code beats code when both words appear', () => {
    expect(isCodeCard('[process] decision to refactor handler')).toBe(false);
  });

  it('returns false for cards with no code-like words', () => {
    expect(isCodeCard('Photograph the library shelf')).toBe(false);
    expect(isCodeCard('Label seeds by tag')).toBe(false);
  });
});

describe('warnShortTitle', () => {
  it('emits warning line when title shorter than 10 chars', () => {
    const logs: string[] = [];
    const origLog = console.log;
    console.log = (...a) => logs.push(a.join(' '));
    try {
      warnShortTitle('short', 'gathering');
    } finally {
      console.log = origLog;
    }
    expect(logs.some((l) => l.toLowerCase().includes('very short'))).toBe(true);
  });

  it('silent when title meets the length floor', () => {
    const logs: string[] = [];
    const origLog = console.log;
    console.log = (...a) => logs.push(a.join(' '));
    try {
      warnShortTitle('A title that has enough words', 'gathering');
    } finally {
      console.log = origLog;
    }
    expect(logs.length).toBe(0);
  });
});

describe('enforceNowDescriptionGate', () => {
  it('swat cards bypass the gate', () => {
    const restore = silenceConsole();
    try {
      expect(enforceNowDescriptionGate(1, '[swat] urgent fix', '', 'gathering')).toBe(true);
    } finally { restore(); }
  });

  it('empty description → blocks with error', () => {
    const restore = silenceConsole();
    try {
      expect(enforceNowDescriptionGate(1, 'build feature', '', 'gathering')).toBe(false);
      expect(enforceNowDescriptionGate(1, 'build feature', undefined, 'gathering')).toBe(false);
    } finally { restore(); }
  });

  it('description with Experience + AC passes', () => {
    const desc = '## Experience\nJeff sees X.\n## AC\n- [ ] verify Y';
    const restore = silenceConsole();
    try {
      expect(enforceNowDescriptionGate(1, 'build', desc, 'gathering')).toBe(true);
    } finally { restore(); }
  });

  it('missing Experience or AC → blocks', () => {
    const experienceOnly = '## Experience\njust this';
    const acOnly = '## AC\n- [ ] item';
    const restore = silenceConsole();
    try {
      expect(enforceNowDescriptionGate(1, 'build', experienceOnly, 'gathering')).toBe(false);
      expect(enforceNowDescriptionGate(1, 'build', acOnly, 'gathering')).toBe(false);
    } finally { restore(); }
  });
});

describe('enforceExperienceGate', () => {
  it('swat cards bypass', () => {
    const restore = silenceConsole();
    try {
      expect(enforceExperienceGate(1, '[swat] fix', '', 'gathering')).toBe(true);
    } finally { restore(); }
  });

  it('parent/umbrella cards bypass', () => {
    const restore = silenceConsole();
    try {
      expect(enforceExperienceGate(1, 'parent', 'children: #1 #2 #3', 'gathering')).toBe(true);
      expect(enforceExperienceGate(1, 'parent', 'Parent card — tracks children', 'gathering')).toBe(true);
    } finally { restore(); }
  });

  it('missing Experience → blocks', () => {
    const restore = silenceConsole();
    try {
      expect(enforceExperienceGate(1, 'build', '## AC\n- [ ] item', 'gathering')).toBe(false);
    } finally { restore(); }
  });

  it('Experience present → passes', () => {
    const restore = silenceConsole();
    try {
      expect(enforceExperienceGate(1, 'build', '## Experience\nJeff sees X', 'gathering')).toBe(true);
    } finally { restore(); }
  });
});

describe('enforceACGate', () => {
  it('swat cards bypass', () => {
    const restore = silenceConsole();
    try {
      expect(enforceACGate(1, '[swat] fix', '', 'gathering')).toBe(true);
    } finally { restore(); }
  });

  it('parent/umbrella cards bypass', () => {
    const restore = silenceConsole();
    try {
      expect(enforceACGate(1, 'parent', 'children: #1', 'gathering')).toBe(true);
    } finally { restore(); }
  });

  it('missing AC (no heading, no checkboxes, no numbered list) → blocks', () => {
    const restore = silenceConsole();
    try {
      expect(enforceACGate(1, 'build', 'just a prose description', 'gathering')).toBe(false);
    } finally { restore(); }
  });

  it('heading "## AC" → passes', () => {
    const restore = silenceConsole();
    try {
      expect(enforceACGate(1, 'build', '## AC\n- something', 'gathering')).toBe(true);
    } finally { restore(); }
  });

  it('markdown checkbox list → passes', () => {
    const restore = silenceConsole();
    try {
      expect(enforceACGate(1, 'build', '- [ ] first item', 'gathering')).toBe(true);
      expect(enforceACGate(1, 'build', '- [x] done item', 'gathering')).toBe(true);
    } finally { restore(); }
  });

  it('numbered-list acceptance items → passes', () => {
    const restore = silenceConsole();
    try {
      expect(enforceACGate(1, 'build', '1. do a thing', 'gathering')).toBe(true);
    } finally { restore(); }
  });

  it('phrase "acceptance criteria" in prose → passes', () => {
    const restore = silenceConsole();
    try {
      expect(enforceACGate(1, 'build', 'the acceptance criteria are X Y Z', 'gathering')).toBe(true);
    } finally { restore(); }
  });
});

describe('enforceTaxonomyGate', () => {
  it('swat cards bypass', () => {
    const restore = silenceConsole();
    try {
      expect(enforceTaxonomyGate(1, '[swat] fix', [], 'gathering')).toBe(true);
    } finally { restore(); }
  });

  it('always returns true (warn-only gate) regardless of sequence label', () => {
    const restore = silenceConsole();
    try {
      expect(enforceTaxonomyGate(1, 'build', [], 'gathering')).toBe(true);
      expect(enforceTaxonomyGate(1, 'build', ['sequence:quality'], 'gathering')).toBe(true);
    } finally { restore(); }
  });

  it('warns when no sequence label present', () => {
    const logs: string[] = [];
    const origLog = console.log;
    console.log = (...a) => logs.push(a.join(' '));
    try {
      enforceTaxonomyGate(42, 'build', ['chunk:ops'], 'gathering');
    } finally {
      console.log = origLog;
    }
    expect(logs.some((l) => l.includes('No sequence label'))).toBe(true);
  });

  it('does not warn when sequence label present', () => {
    const logs: string[] = [];
    const origLog = console.log;
    console.log = (...a) => logs.push(a.join(' '));
    try {
      enforceTaxonomyGate(42, 'build', ['sequence:quality'], 'gathering');
    } finally {
      console.log = origLog;
    }
    expect(logs.length).toBe(0);
  });
});
