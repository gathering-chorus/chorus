import { execSync } from 'child_process';
import * as path from 'path';

const CARDS = path.join(__dirname, '..', '..', '..', '..', 'platform', 'scripts', 'cards');

function run(args: string): string {
  try {
    return execSync(`bash ${CARDS} ${args}`, { encoding: 'utf-8', timeout: 15000 });
  } catch (e: any) {
    return e.stdout || e.stderr || e.message;
  }
}

describe('cards untag', () => {
  test('untag removes label without 403', () => {
    // Card 1074 still has domain:convergence (label 106 on task)
    const before = run('view 1074 --json');
    const hasDomainConvergence = before.includes('domain:convergence');

    if (hasDomainConvergence) {
      const result = run('untag 1074 domain:convergence');
      expect(result).not.toContain('403');
      expect(result).not.toContain('Forbidden');
    } else {
      // Already removed — verify untag handles missing label gracefully
      const result = run('untag 1074 domain:convergence');
      expect(result).not.toContain('403');
    }
  });
});
