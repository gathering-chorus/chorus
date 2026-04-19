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

export function parseAddArgs(args: string[]): AddArgs {
  let title = '', status = 'later', owner = '', priority = '';
  let domain = '', description = '', product = '', chunk = '', sequence = '', type = '', origin = '';
  let descFile = '';
  let quick = false;

  let i = 0;
  while (i < args.length) {
    switch (args[i]) {
      case '--status': status = args[++i]; break;
      case '--owner': owner = args[++i]; break;
      case '--priority': priority = args[++i]; break;
      case '--domain': domain = args[++i]; break;
      case '--description': case '--desc': description = args[++i]; break;
      case '--desc-file': descFile = args[++i]; break;
      case '--product': case '-p': product = args[++i]; break;
      case '--chunk': chunk = args[++i]; break;
      case '--sequence': case '--seq': sequence = args[++i]; break;
      case '--type': case '-t': type = args[++i]; break;
      case '--origin': origin = args[++i]; break;
      case '--quick': case '-q': quick = true; break;
      default:
        if (!title) title = args[i];
        else throw new Error(`Unexpected argument: ${args[i]}`);
    }
    i++;
  }

  // --desc-file takes precedence; --desc=- reads stdin
  if (descFile) {
    if (!fs.existsSync(descFile)) {
      throw new Error(`--desc-file path does not exist: ${descFile}`);
    }
    description = fs.readFileSync(descFile, 'utf-8');
  } else if (description === '-') {
    description = fs.readFileSync(0, 'utf-8');  // fd 0 = stdin
  }

  if (!title) throw new Error(USAGE);
  return { title, status, owner, priority, domain, description, product, chunk, sequence, type, origin, quick };
}
