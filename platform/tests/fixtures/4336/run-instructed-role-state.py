#!/usr/bin/env python3
"""#4336 — RUN every role-state command an instruction file tells a role to run.

Skill markdown and generated CLAUDE.md are instructions: the "code" is the
command a role will type. This pulls each `role-state <role> <state> ...`
command out of inline backticks and fenced code blocks, fills the placeholders
(<you>/<role>/<your-role> -> silas, any other <x> -> x), and executes it through
the real role-state CLI (argv[1] = chorus-hook-shim). A command the CLI refuses
(card= / type=, #2467/#2629) is reported; so is a file set with nothing to run.

  run-instructed-role-state.py <shim> <file|dir> ...
Prints one line per command (RAN / REFUSED / SKIPPED) and a summary
`ran=N refused=M`. Exit 1 when anything was refused, 3 when nothing ran.
`query` is skipped: it reads chorus-api over the network and takes no args the
refusal applies to. The caller sets HOME / CHORUS_LOG_FILE / CHORUS_CONTEXT.
"""
import os
import re
import shlex
import subprocess
import sys

CMD = re.compile(r"(?:^|[\s/])role-state((?:[ \t]+[^\s`]+)+)")
ROLE_PH = re.compile(r"<(?:you|role|your-role|your_role|me)>")
OTHER_PH = re.compile(r"<([^<>\s]+)>")


def files(paths):
    for p in paths:
        if os.path.isfile(p):
            yield p
            continue
        for root, _, names in os.walk(p):
            for n in sorted(names):
                if n.endswith(".md"):
                    yield os.path.join(root, n)


def commands(path):
    """(lineno, argstring) for each role-state command in backticks or fences."""
    fenced = False
    for i, line in enumerate(open(path, encoding="utf-8", errors="replace"), 1):
        if line.lstrip().startswith("```"):
            fenced = not fenced
            continue
        spans = [line] if fenced else re.findall(r"`([^`]+)`", line)
        for span in spans:
            for m in CMD.finditer(span):
                yield i, m.group(1).strip()


def main():
    shim, paths = sys.argv[1], sys.argv[2:]
    ran = refused = 0
    for f in files(paths):
        for lineno, args in commands(f):
            filled = OTHER_PH.sub(r"\1", ROLE_PH.sub("silas", args))
            try:
                argv = shlex.split(filled)
            except ValueError:
                argv = filled.split()
            if len(argv) < 2 or argv[0] in ("query", "cleanup"):
                print(f"SKIPPED {f}:{lineno}: role-state {filled}")
                continue
            p = subprocess.run([shim, "role-state", *argv], capture_output=True, text=True, timeout=20)
            out = (p.stdout + p.stderr).strip().replace("\n", " ")
            if "REFUSED" in out:
                refused += 1
                print(f"REFUSED {f}:{lineno}: role-state {filled} -> rc={p.returncode} {out}")
            else:
                ran += 1
                print(f"RAN {f}:{lineno}: role-state {filled} -> rc={p.returncode}")
    print(f"ran={ran} refused={refused}")
    sys.exit(1 if refused else (3 if ran == 0 else 0))


main()
