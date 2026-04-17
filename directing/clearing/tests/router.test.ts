/**
 * MessageRouter — unit tests (#2167 phase 2).
 *
 * Target: 80%+ on src/router.ts. Covers classify() dispatch across all
 * type paths, dedup, trim-to-max, and the four private helpers
 * (stripSpineMetadata, isSystemNoise, isToolCall, isSkillOutput,
 * isRoleToRole) via routed inputs.
 */

import { MessageRouter } from '../src/router';

function mk(from: string, text: string, type?: string, level?: string) {
  return { from, text, ts: '2026-04-17T20:00:00Z', ...(type ? { type } : {}), ...(level ? { level } : {}) };
}

describe('MessageRouter — ingest basics', () => {
  test('ingest stores message and emits', () => {
    const r = new MessageRouter();
    const heard: any[] = [];
    r.on('message', (m) => heard.push(m));
    r.ingest(mk('jeff', 'hello'));
    expect(heard).toHaveLength(1);
    expect(r.getRecent(10)).toHaveLength(1);
  });

  test('level is preserved if supplied', () => {
    const r = new MessageRouter();
    r.ingest(mk('silas', '[nudge from silas] x', undefined, 'critical'));
    // Goes to role-to-role (hidden). includeHidden to inspect.
    const m = r.getRecent(10, true)[0];
    expect(m.level).toBe('critical');
  });

  test('getRecent default filters hidden', () => {
    const r = new MessageRouter();
    r.ingest(mk('jeff', 'visible'));
    r.ingest(mk('kade', '[nudge from kade] hidden'));
    expect(r.getRecent(10)).toHaveLength(1);
    expect(r.getRecent(10, true)).toHaveLength(2);
  });

  test('trim to 200 max — excess drops from front', () => {
    const r = new MessageRouter();
    for (let i = 0; i < 205; i++) {
      r.ingest(mk('jeff', `m${i}`));
    }
    const all = r.getRecent(999);
    expect(all.length).toBeLessThanOrEqual(200);
    // Latest is retained
    expect(all[all.length - 1].text).toBe('m204');
  });
});

describe('MessageRouter — dedup', () => {
  test('exact duplicate from same sender is dropped', () => {
    const r = new MessageRouter();
    r.ingest(mk('jeff', 'same'));
    r.ingest(mk('jeff', 'same'));
    expect(r.getRecent(10)).toHaveLength(1);
  });

  test('same text from different sender is kept', () => {
    const r = new MessageRouter();
    r.ingest(mk('jeff', 'shared line'));
    r.ingest(mk('wren', 'shared line'));
    expect(r.getRecent(10, true)).toHaveLength(2);
  });

  test('@mention-stripped exact duplicate is dropped', () => {
    const r = new MessageRouter();
    r.ingest(mk('jeff', 'deploy the fix'));
    r.ingest(mk('jeff', '@kade deploy the fix'));
    expect(r.getRecent(10)).toHaveLength(1);
  });

  test('different text is not deduped', () => {
    const r = new MessageRouter();
    r.ingest(mk('jeff', 'a'));
    r.ingest(mk('jeff', 'b'));
    expect(r.getRecent(10)).toHaveLength(2);
  });
});

describe('MessageRouter — classify: hidden paths', () => {
  test('probe type is hidden', () => {
    const r = new MessageRouter();
    r.ingest(mk('probe', 'health ping', 'probe'));
    expect(r.getRecent(10)).toHaveLength(0);
    expect(r.getRecent(10, true)[0].visible).toBe(false);
  });

  test('probe sender without type is hidden', () => {
    const r = new MessageRouter();
    r.ingest(mk('probe', 'raw ping'));
    expect(r.getRecent(10)).toHaveLength(0);
  });

  test('[progress] batch text is hidden', () => {
    const r = new MessageRouter();
    r.ingest(mk('kade', '[progress] 50/100'));
    r.ingest(mk('kade', '[batch] start'));
    r.ingest(mk('kade', '[batch-complete] done'));
    expect(r.getRecent(10)).toHaveLength(0);
  });

  test('[bridge] echo is hidden', () => {
    const r = new MessageRouter();
    r.ingest(mk('silas', '[bridge] echo of event'));
    expect(r.getRecent(10)).toHaveLength(0);
  });

  test('XML-tag system noise is hidden', () => {
    const r = new MessageRouter();
    r.ingest(mk('silas', '<system>raw</system>'));
    expect(r.getRecent(10)).toHaveLength(0);
  });

  test('file-path noise is hidden (Users, var, private, tmp)', () => {
    const r = new MessageRouter();
    r.ingest(mk('kade', '/Users/jeff/x'));
    r.ingest(mk('kade', '/var/log/y'));
    r.ingest(mk('kade', '/private/tmp/z'));
    r.ingest(mk('kade', '/tmp/w'));
    expect(r.getRecent(10)).toHaveLength(0);
  });

  test('specific noise prefixes are hidden', () => {
    const r = new MessageRouter();
    ['hook fire', 'Base directory: x', 'ARGUMENTS: x', 'Stop hook', '→ delivered', '[Request interrupted]', '[Image: source: x]', 'chorus-query result', '[search] 3 results']
      .forEach((t) => r.ingest(mk('kade', t)));
    expect(r.getRecent(10)).toHaveLength(0);
  });
});

describe('MessageRouter — classify: pm-thinking', () => {
  test('pm-thinking with tool call is hidden', () => {
    const r = new MessageRouter();
    r.ingest(mk('wren', 'bash ls', 'pm-thinking'));
    expect(r.getRecent(10)).toHaveLength(0);
  });

  test('pm-thinking with skill output is hidden', () => {
    const r = new MessageRouter();
    r.ingest(mk('wren', 'Done: #1234', 'pm-thinking'));
    r.ingest(mk('wren', 'Moved #5 to WIP', 'pm-thinking'));
    r.ingest(mk('wren', 'Pulled #7', 'pm-thinking'));
    r.ingest(mk('wren', 'Gate chain passed', 'pm-thinking'));
    expect(r.getRecent(10)).toHaveLength(0);
  });

  test('pm-thinking with plain commentary is visible', () => {
    const r = new MessageRouter();
    r.ingest(mk('wren', 'I think we should prioritize the tests.', 'pm-thinking'));
    expect(r.getRecent(10)).toHaveLength(1);
    expect(r.getRecent(10)[0].type).toBe('pm-thinking');
  });
});

describe('MessageRouter — classify: accept-request', () => {
  test('explicit accept-request type is visible', () => {
    const r = new MessageRouter();
    r.ingest(mk('jeff', 'Accepted #5', 'accept-request'));
    expect(r.getRecent(10)[0].type).toBe('accept-request');
  });

  test('jeff saying /acp is accept-request', () => {
    const r = new MessageRouter();
    r.ingest(mk('jeff', '/acp 42'));
    expect(r.getRecent(10)[0].type).toBe('accept-request');
  });

  test('role saying "Accepted #X" is NOT accept-request (skill output)', () => {
    const r = new MessageRouter();
    r.ingest(mk('kade', 'Accepted #42 — ship it'));
    // Not jeff, no explicit type — falls through to default hidden.
    expect(r.getRecent(10)).toHaveLength(0);
  });
});

describe('MessageRouter — classify: jeff input and visibility', () => {
  test('jeff-input type strips spine metadata', () => {
    const r = new MessageRouter();
    r.ingest(mk('jeff', 'check status | tools: none | 0.3s', 'jeff-input'));
    expect(r.getRecent(10)[0].text).toBe('check status');
  });

  test('from=jeff always visible, strips spine metadata', () => {
    const r = new MessageRouter();
    r.ingest(mk('jeff', 'raw input | tools: Read | 0.1s'));
    expect(r.getRecent(10)[0].type).toBe('jeff-input');
    expect(r.getRecent(10)[0].text).toBe('raw input');
  });

  test('from=jeff-guest (startsWith jeff) is jeff-input', () => {
    const r = new MessageRouter();
    r.ingest(mk('jeff-guest', 'hi from lan'));
    expect(r.getRecent(10)[0].type).toBe('jeff-input');
  });
});

describe('MessageRouter — classify: visible markers', () => {
  test('system-error type is visible', () => {
    const r = new MessageRouter();
    r.ingest(mk('system', 'Fuseki down', 'system-error'));
    expect(r.getRecent(10)[0].type).toBe('system-error');
  });

  test('[demo] tag becomes demo-ready', () => {
    const r = new MessageRouter();
    r.ingest(mk('kade', '[demo] #2167 shipped'));
    expect(r.getRecent(10)[0].type).toBe('demo-ready');
  });

  test('"demo ready" phrase becomes demo-ready', () => {
    const r = new MessageRouter();
    r.ingest(mk('kade', 'Demo ready: #2166'));
    expect(r.getRecent(10)[0].type).toBe('demo-ready');
  });

  test('BLOCKED keyword becomes blocked', () => {
    const r = new MessageRouter();
    r.ingest(mk('silas', 'BLOCKED: Fuseki 503'));
    expect(r.getRecent(10)[0].type).toBe('blocked');
  });

  test('"blocked" lowercase keyword also becomes blocked', () => {
    const r = new MessageRouter();
    r.ingest(mk('silas', 'task blocked by perms'));
    expect(r.getRecent(10)[0].type).toBe('blocked');
  });

  test('[decision] becomes visible role-response', () => {
    const r = new MessageRouter();
    r.ingest(mk('wren', '[decision] ship it'));
    expect(r.getRecent(10)[0].type).toBe('role-response');
  });

  test('"decision needed" phrase becomes role-response', () => {
    const r = new MessageRouter();
    r.ingest(mk('wren', 'decision needed: lunch?'));
    expect(r.getRecent(10)[0].type).toBe('role-response');
  });

  test('[gemba] prefix rendered with eye icon', () => {
    const r = new MessageRouter();
    r.ingest(mk('wren', '[gemba] observing silas'));
    const m = r.getRecent(10)[0];
    expect(m.text).toBe('👁 observing silas');
    expect(m.type).toBe('role-response');
  });
});

describe('MessageRouter — classify: role-to-role (hidden)', () => {
  test('[nudge from X] is hidden', () => {
    const r = new MessageRouter();
    r.ingest(mk('silas', '[nudge from silas] internal'));
    expect(r.getRecent(10)).toHaveLength(0);
  });

  test('coordination prefixes hidden ([reply], [ack], [feedback], [direction], [correction], [chat])', () => {
    const r = new MessageRouter();
    ['[reply] ok', '[ack] got it', '[feedback] nit', '[direction] do X', '[correction] fix Y', '[chat] hi']
      .forEach((t) => r.ingest(mk('kade', t)));
    expect(r.getRecent(10)).toHaveLength(0);
  });

  test('plain acks hidden (ack, acknowledged, got it, will do, on it)', () => {
    const r = new MessageRouter();
    ['ack', 'acknowledged', 'got it', 'will do', 'on it']
      .forEach((t) => r.ingest(mk('kade', t)));
    expect(r.getRecent(10)).toHaveLength(0);
  });

  test('DELIVERED to role is hidden', () => {
    const r = new MessageRouter();
    r.ingest(mk('kade', 'DELIVERED to silas at 10:00'));
    expect(r.getRecent(10)).toHaveLength(0);
  });

  test('role-response explicit tag (not caught by role-to-role) is visible', () => {
    const r = new MessageRouter();
    r.ingest(mk('kade', 'Here is my status update.', 'role-response'));
    expect(r.getRecent(10)[0].type).toBe('role-response');
  });

  test('role message with no signal defaults to hidden role-to-role', () => {
    const r = new MessageRouter();
    r.ingest(mk('silas', 'just some random internal note'));
    expect(r.getRecent(10)).toHaveLength(0);
  });
});

describe('MessageRouter — getHiddenCount', () => {
  test('counts hidden since last visible, resets on visible', () => {
    const r = new MessageRouter();
    r.ingest(mk('jeff', 'visible one'));           // visible
    r.ingest(mk('kade', '[nudge from kade] h1'));   // hidden
    r.ingest(mk('kade', '[nudge from kade] h2'));   // hidden
    expect(r.getHiddenCount()).toBe(2);
    r.ingest(mk('jeff', 'visible two'));
    expect(r.getHiddenCount()).toBe(0);
  });

  test('all hidden = count equals total', () => {
    const r = new MessageRouter();
    r.ingest(mk('kade', '[bridge] echo'));
    r.ingest(mk('silas', '[nudge from silas] x'));
    expect(r.getHiddenCount()).toBe(2);
  });

  test('empty router returns 0', () => {
    expect(new MessageRouter().getHiddenCount()).toBe(0);
  });
});

describe('MessageRouter — isToolCall / isSkillOutput corner cases', () => {
  test('pm-thinking with bash command hidden', () => {
    const r = new MessageRouter();
    r.ingest(mk('wren', 'cd /tmp && ls', 'pm-thinking'));
    expect(r.getRecent(10)).toHaveLength(0);
  });

  test('pm-thinking with git bracket output hidden', () => {
    const r = new MessageRouter();
    r.ingest(mk('wren', '[main abc1234] committed changes', 'pm-thinking'));
    expect(r.getRecent(10)).toHaveLength(0);
  });

  test('pm-thinking with JSON block hidden', () => {
    const r = new MessageRouter();
    r.ingest(mk('wren', '{"status": "ok"}', 'pm-thinking'));
    expect(r.getRecent(10)).toHaveLength(0);
  });

  test('pm-thinking with HTTP response hidden', () => {
    const r = new MessageRouter();
    r.ingest(mk('wren', 'HTTP/1.1 200 OK', 'pm-thinking'));
    expect(r.getRecent(10)).toHaveLength(0);
  });

  test('pm-thinking with exit code hidden', () => {
    const r = new MessageRouter();
    r.ingest(mk('wren', 'Exit code 1', 'pm-thinking'));
    expect(r.getRecent(10)).toHaveLength(0);
  });

  test('pm-thinking with skill output variants all hidden', () => {
    const r = new MessageRouter();
    ['Auto-checked 5 AC items', 'INJECT_FAILED', 'Updated #1', 'Rejected: #2 — reason',
     'Blocked: #3', 'Unblocked: #4', 'gate:product-pass', 'Nudge delivered', 'pre-commit: done']
      .forEach((t) => r.ingest(mk('wren', t, 'pm-thinking')));
    expect(r.getRecent(10)).toHaveLength(0);
  });
});
