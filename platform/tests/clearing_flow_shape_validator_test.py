#!/usr/bin/env python3
# Tests for clearing-flow-shape-validator.py (#2333).
# Drives the validator directly with JSON inputs — no HTTP stub.
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
VALIDATOR = REPO / "platform" / "scripts" / "clearing-flow-shape-validator.py"


def run_validator(body: str, *, log_lines=None):
    """Run the validator as a subprocess. Returns (exit_code, stderr, log_lines).

    log_lines is captured via a chorus-log stub that appends to a tempfile.
    """
    with tempfile.TemporaryDirectory() as td:
        out = Path(td) / "out"
        stub = Path(td) / "log-stub.sh"
        stub.write_text(f'#!/bin/bash\necho "$@" >> "{out}"\n')
        stub.chmod(0o755)
        env = os.environ.copy()
        env["CHORUS_LOG"] = str(stub)
        env["ROLE"] = "test"
        proc = subprocess.run(
            [sys.executable, str(VALIDATOR)],
            input=body,
            capture_output=True,
            text=True,
            env=env,
        )
        captured = out.read_text().splitlines() if out.exists() else []
        return proc.returncode, proc.stderr, captured


class ValidatorTests(unittest.TestCase):
    # AC3: first card sequences must be array.
    def test_fail_when_first_card_no_sequences(self):
        rc, stderr, log = run_validator(
            json.dumps({"domains": {"chorus": {"cards": [{"id": 1, "title": "x"}]}}})
        )
        self.assertEqual(rc, 1)
        self.assertTrue(any("stage=shape_first_card" in l for l in log), log)

    def test_fail_when_first_card_sequences_is_string(self):
        rc, _, log = run_validator(
            json.dumps({"domains": {"chorus": {"cards": [{"id": 1, "sequences": "clearing"}]}}})
        )
        self.assertEqual(rc, 1)
        self.assertTrue(any("stage=shape_first_card" in l for l in log))

    def test_fail_when_chorus_cards_empty(self):
        rc, _, log = run_validator(json.dumps({"domains": {"chorus": {"cards": []}}}))
        self.assertEqual(rc, 1)
        self.assertTrue(any("stage=flow_shape" in l for l in log))

    def test_fail_when_json_malformed(self):
        rc, _, log = run_validator("not json")
        self.assertEqual(rc, 1)
        self.assertTrue(any("stage=flow_json" in l for l in log))

    # Happy paths — AC1-3 satisfied.
    def test_pass_with_single_seq_emits_no_multi_seq_warn(self):
        rc, _, log = run_validator(json.dumps({"domains": {"chorus": {"cards": [
            {"id": 1, "sequences": ["clearing"]},
            {"id": 2, "sequences": ["werk"]},
        ]}}}))
        self.assertEqual(rc, 0)
        self.assertTrue(any("clearing.smoke.no_multi_seq" in l for l in log))
        self.assertTrue(any("clearing.smoke.passed" in l for l in log))
        self.assertFalse(any("clearing.smoke.failed" in l for l in log))

    # AC4 conditional — auto-activates when #2329 ships multi-seq cards.
    def test_pass_when_multi_seq_labels_valid(self):
        rc, _, log = run_validator(json.dumps({"domains": {"chorus": {"cards": [
            {"id": 1, "sequences": ["clearing", "werk"]},
        ]}}}))
        self.assertEqual(rc, 0)
        self.assertTrue(any("multi_seq=1" in l for l in log))
        self.assertFalse(any("clearing.smoke.no_multi_seq" in l for l in log))

    def test_fail_when_multi_seq_has_empty_label(self):
        rc, _, log = run_validator(json.dumps({"domains": {"chorus": {"cards": [
            {"id": 1, "sequences": ["clearing", ""]},
        ]}}}))
        self.assertEqual(rc, 1)
        self.assertTrue(any("stage=multi_seq_labels" in l for l in log))

    def test_fail_when_multi_seq_has_non_string_label(self):
        rc, _, log = run_validator(json.dumps({"domains": {"chorus": {"cards": [
            {"id": 1, "sequences": ["clearing", None]},
        ]}}}))
        self.assertEqual(rc, 1)
        self.assertTrue(any("stage=multi_seq_labels" in l for l in log))


if __name__ == "__main__":
    unittest.main()
