import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { ContextAssembler } from '../src/context-assembler';
import { RoleConfig } from '../src/config';

describe('ContextAssembler', () => {
  let tmpDir: string;
  let assembler: ContextAssembler;

  const makeRole = (overrides?: Partial<RoleConfig>): RoleConfig => ({
    name: 'silas',
    channel: 'silas',
    claudeMdPath: path.join(tmpDir, 'CLAUDE.md'),
    memoryPath: path.join(tmpDir, 'MEMORY.md'),
    briefsPath: path.join(tmpDir, 'briefs'),
    maxCallsPerHour: 15,
    ...overrides,
  });

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'bridge-test-'));
    fs.mkdirSync(path.join(tmpDir, 'briefs'), { recursive: true });
    assembler = new ContextAssembler(path.join(tmpDir, 'activity.md'));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('includes bridge preamble', () => {
    const result = assembler.assemble(makeRole(), []);
    expect(result).toContain('You are responding via Slack');
    expect(result).toContain('You CANNOT: write files');
  });

  it('includes CLAUDE.md content', () => {
    fs.writeFileSync(path.join(tmpDir, 'CLAUDE.md'), '# Architect Role\nYou are Silas.');
    const result = assembler.assemble(makeRole(), []);
    expect(result).toContain('ROLE IDENTITY');
    expect(result).toContain('You are Silas.');
  });

  it('includes memory content', () => {
    fs.writeFileSync(path.join(tmpDir, 'MEMORY.md'), '## Key Learnings\nImportant thing.');
    const result = assembler.assemble(makeRole(), []);
    expect(result).toContain('MEMORY');
    expect(result).toContain('Important thing.');
  });

  it('lists briefs by filename', () => {
    fs.writeFileSync(path.join(tmpDir, 'briefs', '2026-02-15-bridge.md'), 'content');
    fs.writeFileSync(path.join(tmpDir, 'briefs', '2026-02-14-old.md'), 'old content');
    const result = assembler.assemble(makeRole(), []);
    expect(result).toContain('PENDING BRIEFS');
    expect(result).toContain('2026-02-15-bridge.md');
    expect(result).toContain('2026-02-14-old.md');
  });

  it('includes recent activity tail', () => {
    const lines = Array.from({ length: 50 }, (_, i) => `Line ${i + 1}`);
    fs.writeFileSync(path.join(tmpDir, 'activity.md'), lines.join('\n'));
    const result = assembler.assemble(makeRole(), []);
    expect(result).toContain('RECENT ACTIVITY');
    expect(result).toContain('Line 50');
    expect(result).toContain('Line 21');
    expect(result).not.toContain('Line 20\n');
  });

  it('includes channel history', () => {
    const history = ['Hey silas', 'Quick question about the bridge'];
    const result = assembler.assemble(makeRole(), history);
    expect(result).toContain('RECENT CHANNEL MESSAGES');
    expect(result).toContain('Quick question about the bridge');
  });

  it('handles missing files gracefully', () => {
    const role = makeRole({
      claudeMdPath: '/nonexistent/CLAUDE.md',
      memoryPath: '/nonexistent/MEMORY.md',
      briefsPath: '/nonexistent/briefs',
    });
    // Should not throw
    const result = assembler.assemble(role, []);
    expect(result).toContain('You are responding via Slack');
  });
});
