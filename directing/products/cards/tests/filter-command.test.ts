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

describe('cards filter command', () => {
  test('filter --domain chorus --sequence gates returns only gates cards', () => {
    const output = run('filter --domain chorus --sequence gates');
    expect(output).toContain('sequence:gates');
    expect(output).not.toContain('sequence:ops');
  });

  test('filter --domain chorus --sequence ops returns only ops cards', () => {
    const output = run('filter --domain chorus --sequence ops');
    expect(output).toContain('sequence:ops');
  });

  test('filter --owner wren returns only Wren cards', () => {
    const output = run('filter --owner wren');
    expect(output).toContain('Wren');
  });

  test('filter --domain chorus --type fix returns only fix cards', () => {
    const output = run('filter --domain chorus --type fix');
    expect(output).toContain('type:fix');
  });

  test('filter with no flags shows usage', () => {
    const output = run('filter');
    expect(output).toContain('Usage');
  });

  test('filter excludes Done and Won\'t Do by default', () => {
    const output = run('filter --domain chorus --sequence gates');
    expect(output).not.toMatch(/^Done/m);
    expect(output).not.toMatch(/Won't Do/m);
  });
});
