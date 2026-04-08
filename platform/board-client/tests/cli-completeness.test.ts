/**
 * #2024 AC #7 — CLI completeness tests
 * Integration tests for tag error, untag, bulk-move, add --sequence warn.
 */
import { execSync } from 'child_process';

const CARDS = 'bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards';

function run(cmd: string): { stdout: string; stderr: string; exitCode: number } {
  try {
    // Merge stderr into stdout so we capture warnings from successful commands
    const stdout = execSync(`${CARDS} ${cmd} 2>&1`, { encoding: 'utf-8' });
    return { stdout, stderr: '', exitCode: 0 };
  } catch (err: any) {
    return {
      stdout: err.stdout || '',
      stderr: err.stderr || '',
      exitCode: err.status || 1,
    };
  }
}

describe('CLI completeness (#2024)', () => {

  // AC #2: tag error message references correct commands
  test('tag command shows error pointing to cards set and sequence-tag', () => {
    const result = run('tag 1866 sequence infrastructure');
    expect(result.exitCode).not.toBe(0);
    const output = result.stdout + result.stderr;
    expect(output).toContain('cards set');
    expect(output).toContain('cards sequence-tag');
    expect(output).not.toContain('board set');
  });

  // AC #3: untag removes a label
  test('untag adds then removes a label', async () => {
    // Add a sequence label
    run('set 1866 sequence=content');
    const before = run('view 1866');
    expect(before.stdout).toContain('sequence:content');

    // Remove it
    const result = run('untag 1866 sequence:content');
    expect(result.stdout).toContain('Untagged');

    // Restore original
    run('set 1866 sequence=infrastructure');
    const after = run('view 1866');
    expect(after.stdout).toContain('sequence:infrastructure');
    expect(after.stdout).not.toContain('sequence:content');
  });

  test('untag rejects bare values without category prefix', () => {
    const result = run('untag 1866 infrastructure');
    expect(result.exitCode).not.toBe(0);
    const output = result.stdout + result.stderr;
    expect(output).toContain('category:value');
  });

  // AC #4: bulk-move moves multiple cards
  test('bulk-move moves cards and moves them back', () => {
    // Move 3 Later cards to Next
    const moveResult = run('bulk-move 1761,1762,1763 Next');
    expect(moveResult.stdout).toContain('#1761');
    expect(moveResult.stdout).toContain('#1762');
    expect(moveResult.stdout).toContain('#1763');
    expect(moveResult.stdout).toContain('Moved 3 card(s)');

    // Move them back
    const backResult = run('bulk-move 1761,1762,1763 Later');
    expect(backResult.stdout).toContain('Moved 3 card(s)');
  });

  // AC #5: add without --sequence warns on stderr
  test('add without --sequence warns on stderr', () => {
    const result = run('add "CLI test card" --owner kade --priority P3 --domain chorus --type chore --chunk ops --quick');
    const combined = result.stdout + result.stderr;
    expect(combined).toContain('WARN: No --sequence');

    // Clean up — extract card ID and move to Won't Do
    const match = result.stdout.match(/#(\d+)/);
    if (match) {
      run(`move ${match[1]} "Won't Do"`);
    }
  });

  test('add with --sequence does not warn', () => {
    const result = run('add "CLI test card seq" --owner kade --priority P3 --domain chorus --type chore --chunk ops --sequence hardening --quick');
    const combined = result.stdout + result.stderr;
    expect(combined).not.toContain('WARN: No --sequence');

    // Clean up
    const match = result.stdout.match(/#(\d+)/);
    if (match) {
      run(`move ${match[1]} "Won't Do"`);
    }
  });
});
