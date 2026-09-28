#!/usr/bin/env python3
"""#4336 — pull one step's `run:` script (or one job-level env value) out of a
GitHub Actions workflow so a bats case can EXECUTE it, instead of grepping the
YAML for a string. No PyYAML on the box, so this reads the block by indent.

  gha-step.py <workflow.yml> run <step-name>      -> the step's run script
  gha-step.py <workflow.yml> env <job> <KEY>      -> that job-level env value
Exit 3 when the step/key is not found — a missing target must be loud (#3734).
"""
import re
import sys


def indent(line):
    return len(line) - len(line.lstrip(" "))


def step_run(lines, name):
    for i, line in enumerate(lines):
        m = re.match(r"^(\s*)- name:\s*(.+?)\s*$", line)
        if not m or m.group(2).strip("'\"") != name:
            continue
        step_ind = len(m.group(1))
        for j in range(i + 1, len(lines)):
            l = lines[j]
            if l.strip() and indent(l) <= step_ind:
                return None  # next step began before a run: key
            r = re.match(r"^(\s*)run:\s*\|\s*$", l)
            if r:
                key_ind = len(r.group(1))
                body = []
                for k in range(j + 1, len(lines)):
                    b = lines[k]
                    if b.strip() and indent(b) <= key_ind:
                        break
                    body.append(b)
                ind = min((indent(b) for b in body if b.strip()), default=0)
                return "\n".join(b[ind:] for b in body).rstrip() + "\n"
    return None


def job_env(lines, job, key):
    in_job = in_env = False
    job_ind = env_ind = -1
    for line in lines:
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        if re.match(rf"^\s*{re.escape(job)}:\s*$", line) and not in_job:
            in_job, job_ind = True, indent(line)
            continue
        if in_job and indent(line) <= job_ind:
            return None
        if in_job and not in_env and re.match(r"^\s*env:\s*$", line):
            in_env, env_ind = True, indent(line)
            continue
        if in_env:
            if indent(line) <= env_ind:
                in_env = False
                continue
            m = re.match(rf"^\s*{re.escape(key)}:\s*(.*?)\s*$", line)
            if m:
                return m.group(1).strip("'\"")
    return None


def main():
    lines = open(sys.argv[1]).read().splitlines()
    if sys.argv[2] == "run":
        out = step_run(lines, sys.argv[3])
    else:
        out = job_env(lines, sys.argv[3], sys.argv[4])
    if out is None:
        sys.stderr.write("gha-step: target not found\n")
        sys.exit(3)
    sys.stdout.write(out if sys.argv[2] == "run" else out + "\n")


main()
