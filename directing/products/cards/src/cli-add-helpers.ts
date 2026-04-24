/**
 * Helpers for `cards add` parsing (#2223 ergonomics).
 *
 * parseAddArgs: pure parser, throws on error (testable). Handles
 * --desc-file <path> and --desc - (stdin). --desc-file takes precedence
 * over inline --desc when both present.
 */
import * as fs from 'fs';

export interface AddArgs {
  title: string;
  status: string;
  owner: string;
  priority: string;
  domain: string;
  description: string;
  product: string;
  chunk: string;
  sequence: string;
  type: string;
  origin: string;
  quick: boolean;
}

const USAGE =
  'Usage: cards add "title" [--status S] [--owner O] [--priority P] [--domain D] ' +
  '[--product P] [--chunk C] [--sequence S] [--type T] [--origin O] ' +
  '[--desc D | --desc-file PATH | --desc -] [--quick]';

/** String-valued field names only — `quick` (boolean) is not here. */
type StringField = Exclude<keyof AddArgs, 'quick'>;

const STRING_FLAGS: Record<string, StringField> = {
  '--status': 'status',
  '--owner': 'owner',
  '--priority': 'priority',
  '--domain': 'domain',
  '--description': 'description',
  '--desc': 'description',
  '--product': 'product',
  '-p': 'product',
  '--chunk': 'chunk',
  '--sequence': 'sequence',
  '--seq': 'sequence',
  '--type': 'type',
  '-t': 'type',
  '--origin': 'origin',
};

function resolveDescription(description: string, descFile: string): string {
  if (descFile) {
    if (!fs.existsSync(descFile)) throw new Error(`--desc-file path does not exist: ${descFile}`);
    return fs.readFileSync(descFile, 'utf-8');
  }
  if (description === '-') return fs.readFileSync(0, 'utf-8');
  return description;
}

export function parseAddArgs(args: string[]): AddArgs {
  const out: AddArgs = {
    title: '', status: 'later', owner: '', priority: '',
    domain: '', description: '', product: '', chunk: '', sequence: '',
    type: '', origin: '', quick: false,
  };
  let descFile = '';

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--desc-file') { descFile = args[++i]; continue; }
    if (arg === '--quick' || arg === '-q') { out.quick = true; continue; }
    const field = STRING_FLAGS[arg];
    if (field) {
      out[field] = args[++i];
      continue;
    }
    if (!out.title) out.title = arg;
    else throw new Error(`Unexpected argument: ${arg}`);
  }

  out.description = resolveDescription(out.description, descFile);
  if (!out.title) throw new Error(USAGE);
  return out;
}
