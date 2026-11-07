"""#4634 — every LaunchAgent that calls a runner tool names a tool that exists.

The herald tool was renamed three times (underscore, then borg-tend-herald, then herald-run)
and the sensor agents kept calling a dead name from 10-17 to 10-18, exiting 2
every run while nothing reported it. Usage:
    plist-tool-check.py <launchd-dir> <skills.json>
skills.json is the door's /v1/skills/skills answer. Prints one line per agent
calling a missing tool and exits 1 if there is any.
"""
import json
import pathlib
import plistlib
import re
import sys

CALL = re.compile(r"chorus-mcp-call\.sh\s+(\S+)\s+([A-Za-z0-9_-]+)")


def called_tools(plist_path):
    with open(plist_path, 'rb') as f:
        args = plistlib.load(f).get('ProgramArguments', [])
    tools = [m.group(2) for a in args for m in CALL.finditer(a)]
    for i, a in enumerate(args):
        if a.endswith('chorus-mcp-call.sh') and i + 2 < len(args):
            tools.append(args[i + 2])
    return tools


def missing(launchd_dir, skill_names):
    out = []
    for p in sorted(pathlib.Path(launchd_dir).glob('*.plist')):
        for t in called_tools(p):
            if t not in skill_names:
                out.append(f"{p.name}: calls {t}, which is not a tool")
    return out


def main():
    skills = json.load(open(sys.argv[2]))
    names = {r.get('name') for r in skills.get('data', [])}
    bad = missing(sys.argv[1], names)
    print('\n'.join(bad) if bad else 'every agent calls a tool that exists')
    sys.exit(1 if bad else 0)


if __name__ == '__main__':
    main()
