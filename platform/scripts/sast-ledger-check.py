#!/usr/bin/env python3
"""#4107 — reconcile semgrep's findings against the SAST exemption ledger.

Reads semgrep's text report on stdin, the ledger path as argv[1], and semgrep's
own exit code as argv[2]. Exits 0 only when the two agree exactly.

The point is that the ledger is checked in BOTH directions:

  unledgered finding  -> RED. A new violation is never quietly absorbed by the
                        fact that some exemptions exist.
  stale ledger entry  -> RED. An exemption whose finding is gone (fixed, moved,
                        renamed, or the rule deleted) fails loud, so the ledger
                        cannot rot into a blind spot that silently covers
                        something else later.

That second direction is the one that matters: it is what separates a debt
ledger from a suppression list.
"""
import json
import os
import re
import sys


def parse_findings(report, target):
    """Return {(relative path, rule id)} from semgrep's text report.

    semgrep prints the file path on its own line, then one or more
    `❯❯❱ <rule id>` lines under it. The path is ABSOLUTE when semgrep was given
    an absolute target and RELATIVE when it was given `.`, so match on the shape
    of a source path rather than on a leading slash. The first version of this
    parser only accepted a leading slash: on the stale-ledger negative proof it
    printed "no parseable finding" while semgrep's report listed two right below
    it, which means a real violation would have read as clean.
    """
    findings = set()
    current = None
    target = os.path.realpath(target)
    path_re = re.compile(r"^[\w./~-]+\.(ts|tsx|js|jsx|py|rs|sh|go|rb|java)$")
    for line in report.splitlines():
        stripped = line.strip()
        if "/" in stripped and path_re.match(stripped):
            path = os.path.realpath(os.path.join(target, stripped))
            current = os.path.relpath(path, target) if path.startswith(target) else stripped
            continue
        m = re.search(r"❯+❱\s+(\S+)", stripped)
        if m and current:
            findings.add((current, m.group(1)))
    return findings


def main():
    ledger_path, semgrep_rc = sys.argv[1], int(sys.argv[2])
    target = os.environ.get("SAST_TARGET", ".")
    report = sys.stdin.read()

    with open(ledger_path) as fh:
        entries = json.load(fh)["exemptions"]

    ledgered = {(e["path"], e["rule"]): e for e in entries}
    found = parse_findings(report, target)

    unledgered = sorted(found - set(ledgered))
    stale = sorted(set(ledgered) - found)

    if semgrep_rc != 0 and not found:
        # semgrep failed for a reason that is not a finding (bad ruleset, crash).
        print("  SAST: semgrep exited %d but reported no parseable finding" % semgrep_rc)
        print(report)
        return 1

    rc = 0
    if unledgered:
        rc = 1
        print("  SAST FINDINGS (not in the exemption ledger):")
        for path, rule in unledgered:
            print("    %s  %s" % (path, rule))
    if stale:
        rc = 1
        print("  SAST LEDGER STALE — these exemptions no longer match any finding.")
        print("  Remove them; an exemption that matches nothing is a blind spot waiting for a new violation:")
        for path, rule in stale:
            print("    %s  %s" % (path, rule))
    if rc == 0:
        print("  SAST clean (%d finding(s), all named in the exemption ledger):" % len(found))
        for path, rule in sorted(found):
            print("    DEBT %s  %s  since %s  owner %s"
                  % (path, rule.rsplit(".", 1)[-1], ledgered[(path, rule)]["since"],
                     ledgered[(path, rule)]["owner"]))
    return rc


if __name__ == "__main__":
    sys.exit(main())
