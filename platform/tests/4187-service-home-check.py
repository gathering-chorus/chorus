#!/usr/bin/env python3
"""#4187 — is Service's instance home the services domain graph?

Split out of 4187-service-home.bats so the bats file holds no nested heredoc.
Two modes, and the second is the negative proof: `homeless-fixture` strips BOTH
halves of the answer from a copy of the model text and requires the same reader
to report no home. Without it the check could not tell "home is derived" from
"nobody ever said".
"""
import sys

def claimed_anywhere(root):
    """Does ANY model file claim Service? The claim need not sit in the same
    file as the shape — services-domain's own claim lives in kade's
    domains-builds-decisions-rcas-4022.ttl, and the first version of this check
    read chorus.ttl alone and therefore called a claimed class homeless. A check
    narrower than the thing it describes reports the wrong state confidently."""
    import glob, os
    for d in ('roles', 'designing'):
        for f in glob.glob(os.path.join(root, d, '**', '*.ttl'), recursive=True):
            try:
                t = open(f).read()
            except OSError:
                continue
            for line in t.splitlines():
                if 'definesVocabulary' in line and 'chorus:Service' in line:
                    tail = line.split('definesVocabulary', 1)[1]
                    names = [n.strip().rstrip(';.').strip() for n in tail.split(',')]
                    names = [n.split('#')[0].strip() for n in names]
                    if 'chorus:Service' in names:
                        return True
    return False


def home(text, root=None):
    i = text.index('chorus:ServiceShape a sh:NodeShape ;')
    blk = text[i:text.index(' .\n', i)]
    if 'chorus:instancesGraph "urn:chorus:instances"' in blk:
        return 'catch-all'
    if 'chorus:instancesGraph "urn:chorus:domains:services"' in blk:
        return 'pinned'
    if root is not None and claimed_anywhere(root):
        return 'derived'
    return 'homeless'

def main():
    import os
    path = sys.argv[1]
    text = open(path).read()
    mode = sys.argv[2]
    root = sys.argv[3] if len(sys.argv) > 3 else None
    if mode == 'home':
        got = home(text, root)
        if got not in ('pinned', 'derived'):
            print(f'Service home is {got}, expected pinned or derived', file=sys.stderr)
            return 1
        print(got)
        return 0
    if mode == 'homeless-fixture':
        stripped = text.replace('chorus:instancesGraph "urn:chorus:domains:services"', 'chorus:comment "stripped"')
        got = home(stripped, None)
        if got != 'homeless':
            print(f'the homeless fixture reads as {got} — the check cannot separate the two states', file=sys.stderr)
            return 1
        print(got)
        return 0
    print(f'unknown mode {mode}', file=sys.stderr)
    return 2

sys.exit(main())
