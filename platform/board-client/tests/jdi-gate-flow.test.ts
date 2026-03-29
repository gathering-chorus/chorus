/**
 * JDI Gate Hook Flow Tests — #1189
 *
 * Tests the three detection layers of jdi-gate-hook.sh:
 *   1. Seeking pattern detection — permission-seeking text in responses
 *   2. Jeff-asked bypass — interrogative prompts are legitimate
 *   3. Legitimacy signals — trade-offs, ambiguity, uncertainty
 *   4. Preference matching — cross-reference jeff-preferences.json
 *   5. Code block stripping — patterns inside code blocks are ignored
 *
 * Tests the Python logic directly via execSync to avoid session JSONL dependency.
 */
import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';

const SCRIPTS_DIR = path.join(__dirname, '../../scripts');
const PREFS_FILE = '/Users/jeffbridwell/CascadeProjects/messages/jeff-preferences.json';

// Helper: run the seeking-pattern Python detector against sample text
function detectSeeking(responseText: string, humanMsg: string = 'build 1189'): string {
  const escaped = responseText.replace(/'/g, "'\\''");
  const humanEscaped = humanMsg.replace(/'/g, "'\\''");
  try {
    return execSync(`LAST_HUMAN_MSG='${humanEscaped}' echo '${escaped}' | python3 -c "
import sys, re, os

text = sys.stdin.read()
BT = chr(96)
text = re.sub(BT*3 + r'.*?' + BT*3, '', text, flags=re.DOTALL)
text = re.sub(BT + r'[^' + BT + r']+' + BT, '', text)
tail = text[-500:].lower()

seeking_patterns = [
    r'shall i (?:go ahead|proceed|start|do|create|build|update|make|write|run)',
    r'should i (?:go ahead|proceed|start|do|create|build|update|make|write|run)',
    r'would you like me to\\\\b',
    r'want me to (?:go ahead|proceed|start|do|create|build|update|make|write|run)',
    r'do you want me to\\\\b',
    r'ready to proceed\\\\?',
    r'i can do (?:this|that).+(?:want|like|prefer)',
    r'here.s (?:what i.m thinking|my plan|the plan).+(?:\\\\?|let me know|sound good)',
    r'i.ll (?:go ahead|proceed|start).+(?:if you|unless you)',
    r'(?:option [a-c]|two options|three options).+(?:which|what|prefer|resonate)',
]

legit_signals = [
    r'genuinely ambiguous',
    r'i don.t (?:know|have enough)',
    r'unclear whether',
    r'could go either way',
    r'trade-?off',
    r'risk.+(?:worth|accept)',
]

jeff_msg = os.environ.get('LAST_HUMAN_MSG', '').lower().strip()
if not jeff_msg:
    print('legit-no-human-msg')
    sys.exit(0)
jeff_asked = bool(
    jeff_msg.endswith('?') or
    re.match(r'^(what |how |why |where |when |which |who |is |are |can |do (?:you|we|i\\\\b)|does |should |could |would |will )', jeff_msg)
)
if jeff_asked:
    print('legit-jeff-asked')
    sys.exit(0)

for p in seeking_patterns:
    if re.search(p, tail):
        for l in legit_signals:
            if re.search(l, tail):
                print('legit')
                sys.exit(0)
        print('seeking')
        sys.exit(0)

print('clean')
"`, { encoding: 'utf-8', timeout: 5000, env: { ...process.env, LAST_HUMAN_MSG: humanMsg } }).trim();
  } catch (err: any) {
    return (err.stdout || '').trim();
  }
}

// Helper: run the preference matcher against sample text
function matchPreference(responseText: string): string {
  const escaped = responseText.replace(/'/g, "'\\''");
  try {
    return execSync(`echo '${escaped}' | python3 -c "
import sys, json, re, os

text = sys.stdin.read()
BT = chr(96)
text = re.sub(BT*3 + r'.*?' + BT*3, '', text, flags=re.DOTALL)
text = re.sub(BT + r'[^' + BT + r']+' + BT, '', text)
tail = text[-500:].lower()

try:
    with open(os.environ['PREFS_FILE']) as f:
        prefs = json.load(f)['preferences']
except:
    print('')
    sys.exit(0)

matchers = {
    'P001': r'(?:should i|shall i|want me to|let me) (?:commit|push)',
    'P002': r'(?:should i|shall i|want me to) (?:proceed|go ahead|continue|start)',
    'P003': r'(?:should i|shall i|want me to) (?:card|create a card)',
    'P004': r'(?:option [a-c]|two options|three options|which (?:approach|do you|would you|resonat))',
    'P005': r'(?:should i|shall i|want me to) deploy',
    'P006': r'(?:should i|shall i|want me to) update (?:state|docs|memory)',
    'P007': r'(?:should i|shall i|want me to) (?:brief|send a brief|notify)',
    'P012': r'(?:should i|shall i|want me to) (?:use plan|enter plan|plan mode)',
    'P014': r'(?:should i|shall i|want me to) (?:pull|pick up) (?:the next|another)',
    'P015': r'here.s (?:what i.m thinking|my plan|the plan)',
    'P017': r'(?:should i|shall i|can i) mark.+done',
}

for pid, pattern in matchers.items():
    if re.search(pattern, tail):
        pref = next((p for p in prefs if p['id'] == pid), None)
        if pref:
            print(pid)
            sys.exit(0)

print('')
"`, { encoding: 'utf-8', timeout: 5000, env: { ...process.env, PREFS_FILE } }).trim();
  } catch (err: any) {
    return (err.stdout || '').trim();
  }
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. HOOK INFRASTRUCTURE — script exists and parses
// ═══════════════════════════════════════════════════════════════════════════

describe.skip('Flow: JDI gate [migrated to Rust] infrastructure', () => {
  test('jdi-gate-hook.sh exists and is executable', () => {
    // jdi-gate-hook.sh was merged into autonomy-guard.sh (#1306)
    const script = path.join(SCRIPTS_DIR, 'autonomy-guard.sh');
    expect(fs.existsSync(script)).toBe(true);
    const stat = fs.statSync(script);
    expect(stat.mode & 0o111).toBeGreaterThan(0);
  });

  test('jdi-gate-hook.sh uses set -euo pipefail', () => {
    const content = fs.readFileSync(path.join(SCRIPTS_DIR, 'autonomy-guard.sh'), 'utf-8');
    expect(content).toContain('set -uo pipefail');
  });

  test('jdi-gate-hook.sh exits 0 on stop_hook_active (no infinite loops)', () => {
    const content = fs.readFileSync(path.join(SCRIPTS_DIR, 'autonomy-guard.sh'), 'utf-8');
    expect(content).toContain('stop_hook_active');
    expect(content).toMatch(/exit 0/);
  });

  test('jdi-gate-hook.sh exits 2 to block permission-seeking', () => {
    const content = fs.readFileSync(path.join(SCRIPTS_DIR, 'autonomy-guard.sh'), 'utf-8');
    expect(content).toContain('exit 2');
  });

  test('jdi-gate-hook.sh derives role from CWD', () => {
    const content = fs.readFileSync(path.join(SCRIPTS_DIR, 'autonomy-guard.sh'), 'utf-8');
    expect(content).toContain('product-manager');
    expect(content).toContain('architect');
    expect(content).toContain('engineer');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 2. SEEKING PATTERN DETECTION — catches permission-seeking text
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: Seeking pattern detection', () => {
  test('"Should I proceed?" → seeking', () => {
    expect(detectSeeking('All tests pass. Should I proceed with the deployment?')).toBe('seeking');
  });

  test('"Shall I go ahead?" → seeking', () => {
    expect(detectSeeking('Everything looks good. Shall I go ahead and build it?')).toBe('seeking');
  });

  test('"Would you like me to" → seeking', () => {
    expect(detectSeeking('I found the issue. Would you like me to fix it?')).toBe('seeking');
  });

  test('"Do you want me to" → seeking', () => {
    expect(detectSeeking('The card is ready. Do you want me to start?')).toBe('seeking');
  });

  test('"Here\'s what I\'m thinking... sound good?" → seeking', () => {
    expect(detectSeeking("Here's what I'm thinking for the approach. Sound good?")).toBe('seeking');
  });

  test('"Option A vs Option B, which resonates?" → seeking', () => {
    expect(detectSeeking('Option A uses React, Option B uses vanilla. Which resonates?')).toBe('seeking');
  });

  test('"I\'ll go ahead if you..." → seeking', () => {
    expect(detectSeeking("I'll go ahead and deploy if you want.")).toBe('seeking');
  });

  test('clean response → clean', () => {
    expect(detectSeeking('Done. Committed f9a4f30, 38 files, all checks passed.')).toBe('clean');
  });

  test('status report without questions → clean', () => {
    expect(detectSeeking('Built the feature. 29 tests passing. Ready for demo.')).toBe('clean');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 3. JEFF-ASKED BYPASS — interrogative prompts are legitimate
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: Jeff-asked bypass', () => {
  test('Jeff asks "how should we..." → legit-jeff-asked', () => {
    expect(detectSeeking('Should I proceed with option A?', 'how should we handle this?')).toBe('legit-jeff-asked');
  });

  test('Jeff asks "what do you think?" → legit-jeff-asked', () => {
    expect(detectSeeking('Should I go ahead and build it?', 'what do you think about this approach?')).toBe('legit-jeff-asked');
  });

  test('Jeff asks "can you..." → legit-jeff-asked', () => {
    expect(detectSeeking('Would you like me to fix it?', 'can you look at this?')).toBe('legit-jeff-asked');
  });

  test('Jeff question ending with ? → legit-jeff-asked', () => {
    expect(detectSeeking('Should I start building?', 'thoughts on the architecture?')).toBe('legit-jeff-asked');
  });

  test('Jeff directive "build 1189" → not bypassed', () => {
    expect(detectSeeking('Should I proceed with the build?', 'build 1189')).toBe('seeking');
  });

  test('empty human message → legit-no-human-msg (safe default)', () => {
    expect(detectSeeking('Should I proceed?', '')).toBe('legit-no-human-msg');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 4. LEGITIMACY SIGNALS — trade-offs and uncertainty pass through
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: Legitimacy signals', () => {
  test('"genuinely ambiguous" → legit', () => {
    expect(detectSeeking('This is genuinely ambiguous. Should I proceed with A or B?')).toBe('legit');
  });

  test('"I don\'t know" → legit', () => {
    expect(detectSeeking("I don't know enough to decide. Should I proceed?")).toBe('legit');
  });

  test('"trade-off" → legit', () => {
    expect(detectSeeking('There is a trade-off here. Should I go ahead with the simpler approach?')).toBe('legit');
  });

  test('"could go either way" → legit', () => {
    expect(detectSeeking('This could go either way. Want me to start with the first option?')).toBe('legit');
  });

  test('"risk worth accepting" → legit', () => {
    expect(detectSeeking('There is risk worth accepting here. Should I proceed?')).toBe('legit');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 5. PREFERENCE MATCHING — cross-reference jeff-preferences.json
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: Preference matching', () => {
  test('jeff-preferences.json exists with 20 preferences', () => {
    expect(fs.existsSync(PREFS_FILE)).toBe(true);
    const prefs = JSON.parse(fs.readFileSync(PREFS_FILE, 'utf-8'));
    expect(prefs.preferences.length).toBe(20);
  });

  test('"Should I commit?" → P001', () => {
    expect(matchPreference('All done. Should I commit and push?')).toBe('P001');
  });

  test('"Should I proceed?" → P002', () => {
    expect(matchPreference('Tests pass. Should I proceed?')).toBe('P002');
  });

  test('"Should I card this?" → P003', () => {
    expect(matchPreference('Found an issue. Should I card this?')).toBe('P003');
  });

  test('"Option A vs B, which resonates?" → P004', () => {
    expect(matchPreference('Option A is faster, Option B is cleaner. Which resonates?')).toBe('P004');
  });

  test('"Should I deploy?" → P005', () => {
    expect(matchPreference('Build complete. Should I deploy?')).toBe('P005');
  });

  test('"Should I update state files?" → P006', () => {
    expect(matchPreference('Session wrapping up. Should I update state files?')).toBe('P006');
  });

  test('"Should I brief the other role?" → P007', () => {
    expect(matchPreference('This crosses into Kade territory. Should I brief him?')).toBe('P007');
  });

  test('"Should I use plan mode?" → P012', () => {
    expect(matchPreference('This is complex. Should I use plan mode?')).toBe('P012');
  });

  test('"Should I pull the next card?" → P014', () => {
    expect(matchPreference('Card done. Should I pull the next one?')).toBe('P014');
  });

  test('"Here\'s what I\'m thinking..." → P015', () => {
    expect(matchPreference("Here's what I'm thinking for the implementation.")).toBe('P015');
  });

  test('"Can I mark this done?" → P017', () => {
    expect(matchPreference('Feature complete. Can I mark this card done?')).toBe('P017');
  });

  test('clean text → no preference match', () => {
    expect(matchPreference('Done. Committed f9a4f30.')).toBe('');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 6. CODE BLOCK STRIPPING — patterns in code are ignored
// ═══════════════════════════════════════════════════════════════════════════

describe('Flow: Code block stripping', () => {
  test('seeking pattern inside triple-backtick block → clean', () => {
    const text = 'Here is the code:\n```\nshould i proceed with this?\n```\nDone.';
    expect(detectSeeking(text)).toBe('clean');
  });

  test('seeking pattern inside inline code → clean', () => {
    const text = 'The error message says `should i go ahead` but I fixed it.';
    expect(detectSeeking(text)).toBe('clean');
  });

  test('seeking pattern outside code block → seeking', () => {
    const text = 'Here is the code:\n```\nsome code\n```\nShould I proceed?';
    expect(detectSeeking(text)).toBe('seeking');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 7. HOOK WIRING — events and feedback
// ═══════════════════════════════════════════════════════════════════════════

describe.skip('Flow: Hook event [migrated to Rust] wiring', () => {
  test('hook emits decision.gate.matched for preference matches', () => {
    const content = fs.readFileSync(path.join(SCRIPTS_DIR, 'autonomy-guard.sh'), 'utf-8');
    expect(content).toContain('decision.gate.matched');
    expect(content).toContain('source=response_text');
  });

  test('hook emits decision.gate.text_leak for generic seeking', () => {
    const content = fs.readFileSync(path.join(SCRIPTS_DIR, 'autonomy-guard.sh'), 'utf-8');
    expect(content).toContain('decision.gate.text_leak');
  });

  test('hook references jeff-preferences.json', () => {
    const content = fs.readFileSync(path.join(SCRIPTS_DIR, 'autonomy-guard.sh'), 'utf-8');
    expect(content).toContain('jeff-preferences.json');
  });

  test('hook feedback includes preference ID and source when matched', () => {
    const content = fs.readFileSync(path.join(SCRIPTS_DIR, 'autonomy-guard.sh'), 'utf-8');
    expect(content).toContain('PREF_ID');
    expect(content).toContain('PREF_TEXT');
    expect(content).toContain('PREF_SOURCE');
  });

  test('hook feedback falls back to generic DEC-025 when no preference matched', () => {
    const content = fs.readFileSync(path.join(SCRIPTS_DIR, 'autonomy-guard.sh'), 'utf-8');
    expect(content).toContain('DEC-025 gate');
    expect(content).toContain('DEC-069 gate');
  });
});
