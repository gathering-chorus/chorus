#!/usr/bin/env python3
"""Tests for doc-inventory-classify.py — 3-signal precedence + bucket coverage."""
import sys
sys.dont_write_bytecode = True  # avoid __pycache__ pollution next to the loaded script
import importlib.util
import os
import tempfile
import unittest
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
SCRIPT = SCRIPT_DIR.parent / "scripts" / "doc-inventory-classify.py"

spec = importlib.util.spec_from_file_location("classify_mod", SCRIPT)
classify_mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(classify_mod)


class PathSignalTests(unittest.TestCase):
    def test_chorus_designing_path_wins(self):
        self.assertEqual(classify_mod.classify_by_path("chorus", "designing/docs/foo.md"), "chorus")

    def test_chorus_roles_path_wins(self):
        self.assertEqual(classify_mod.classify_by_path("chorus", "roles/silas/notes.md"), "chorus")

    def test_gathering_akasha_routes_to_akasha(self):
        self.assertEqual(classify_mod.classify_by_path("gathering", "public/akasha/poem.html"), "akasha")

    def test_gathering_chorus_docs_routes_to_chorus(self):
        self.assertEqual(classify_mod.classify_by_path("gathering", "public/chorus-docs/api.html"), "chorus")

    def test_gathering_archived_routes_to_archive(self):
        self.assertEqual(classify_mod.classify_by_path("gathering", "data/about/_archived/old.md"), "archive")

    def test_gathering_public_default(self):
        self.assertEqual(classify_mod.classify_by_path("gathering", "public/index.html"), "gathering")

    def test_unknown_path_returns_none(self):
        self.assertIsNone(classify_mod.classify_by_path("gathering", "data/about/mixed.md"))

    def test_longest_prefix_wins(self):
        self.assertEqual(classify_mod.classify_by_path("gathering", "public/akasha/x.md"), "akasha")


class FilenameSignalTests(unittest.TestCase):
    def test_chorus_prefix_wins(self):
        self.assertEqual(classify_mod.classify_by_filename("data/about/chorus-design.md"), "chorus")

    def test_borg_prefix_wins(self):
        self.assertEqual(classify_mod.classify_by_filename("data/about/borg-loop.md"), "chorus")

    def test_gathering_prefix_wins(self):
        self.assertEqual(classify_mod.classify_by_filename("data/about/gathering-vision.md"), "gathering")

    def test_uppercase_chorus_token_substring(self):
        self.assertEqual(classify_mod.classify_by_filename("data/about/CHORUS_COMMAND_CARD.html"), "chorus")

    def test_ambiguous_filename_returns_none(self):
        self.assertIsNone(classify_mod.classify_by_filename("data/about/notes.md"))

    def test_both_substring_tokens_returns_none(self):
        # No prefix match; both tokens present in name → conflict, no decision.
        self.assertIsNone(classify_mod.classify_by_filename("data/about/CHORUS_GATHERING_NOTES.md"))


class ContentSignalTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.gathering = os.path.join(self.tmp, "gathering")
        self.chorus = os.path.join(self.tmp, "chorus")
        os.makedirs(self.gathering)
        os.makedirs(self.chorus)
        self._orig_g = classify_mod.GATHERING
        self._orig_c = classify_mod.CHORUS
        classify_mod.GATHERING = self.gathering
        classify_mod.CHORUS = self.chorus

    def tearDown(self):
        classify_mod.GATHERING = self._orig_g
        classify_mod.CHORUS = self._orig_c

    def _write(self, repo, rel, body):
        base = self.gathering if repo == "gathering" else self.chorus
        path = os.path.join(base, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write(body)

    def test_chorus_dominant_content(self):
        self._write("gathering", "data/about/x.md",
                    "chorus chorus chorus borg werk wren silas kade")
        self.assertEqual(classify_mod.classify_by_content("gathering", "data/about/x.md"), "chorus")

    def test_gathering_dominant_content(self):
        self._write("gathering", "data/about/y.md",
                    "gathering garden blog photo wordpress " * 3)
        self.assertEqual(classify_mod.classify_by_content("gathering", "data/about/y.md"), "gathering")

    def test_balanced_content_returns_none(self):
        self._write("gathering", "data/about/z.md",
                    "chorus gathering chorus gathering")
        self.assertIsNone(classify_mod.classify_by_content("gathering", "data/about/z.md"))

    def test_missing_file_returns_none(self):
        self.assertIsNone(classify_mod.classify_by_content("gathering", "data/about/nope.md"))

    def test_low_signal_returns_none(self):
        self._write("gathering", "data/about/q.md", "one mention of chorus, nothing else")
        self.assertIsNone(classify_mod.classify_by_content("gathering", "data/about/q.md"))


class PrecedenceTests(unittest.TestCase):
    """Verify path > filename > content per AC."""

    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.gathering = os.path.join(self.tmp, "gathering")
        os.makedirs(self.gathering)
        self._orig_g = classify_mod.GATHERING
        classify_mod.GATHERING = self.gathering

    def tearDown(self):
        classify_mod.GATHERING = self._orig_g

    def test_path_beats_filename(self):
        result, sig = classify_mod.classify("gathering", "public/akasha/gathering-poem.md")
        self.assertEqual(result, "akasha")
        self.assertEqual(sig, "path")

    def test_path_beats_content(self):
        path = os.path.join(self.gathering, "public/chorus-docs/x.md")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write("gathering garden blog photo " * 5)
        result, sig = classify_mod.classify("gathering", "public/chorus-docs/x.md")
        self.assertEqual(result, "chorus")
        self.assertEqual(sig, "path")

    def test_filename_beats_content(self):
        path = os.path.join(self.gathering, "data/about/chorus-thing.md")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write("gathering garden blog photo wordpress " * 3)
        result, sig = classify_mod.classify("gathering", "data/about/chorus-thing.md")
        self.assertEqual(result, "chorus")
        self.assertEqual(sig, "filename")

    def test_content_when_path_and_filename_silent(self):
        path = os.path.join(self.gathering, "data/about/notes.md")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write("chorus borg werk wren silas " * 3)
        result, sig = classify_mod.classify("gathering", "data/about/notes.md")
        self.assertEqual(result, "chorus")
        self.assertEqual(sig, "content")

    def test_data_about_fallback(self):
        path = os.path.join(self.gathering, "data/about/blank.md")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write("nothing relevant here")
        result, sig = classify_mod.classify("gathering", "data/about/blank.md")
        self.assertEqual(result, "chorus")
        self.assertEqual(sig, "fallback-data-about")

    def test_truly_ambiguous(self):
        result, sig = classify_mod.classify("gathering", "random/path/file.md")
        self.assertEqual(result, "ambiguous")
        self.assertEqual(sig, "none")


class BucketCoverageTests(unittest.TestCase):
    """AC drift note: AC lists 'lost' as a bucket, but classify.py does not emit it.

    'lost' is a state column produced by doc-inventory.sh, not a classification
    column emitted by classify.py. Buckets covered: chorus / gathering / akasha
    / archive / ambiguous.
    """

    def test_chorus_bucket(self):
        self.assertEqual(classify_mod.classify("chorus", "designing/docs/x.md")[0], "chorus")

    def test_gathering_bucket(self):
        self.assertEqual(classify_mod.classify("gathering", "public/index.html")[0], "gathering")

    def test_akasha_bucket(self):
        self.assertEqual(classify_mod.classify("gathering", "public/akasha/poem.html")[0], "akasha")

    def test_archive_bucket(self):
        self.assertEqual(classify_mod.classify("gathering", "data/about/_archived/old.md")[0], "archive")

    def test_ambiguous_bucket(self):
        self.assertEqual(classify_mod.classify("gathering", "no/known/place.md")[0], "ambiguous")


if __name__ == "__main__":
    unittest.main()
