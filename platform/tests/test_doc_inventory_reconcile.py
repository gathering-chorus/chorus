#!/usr/bin/env python3
"""Tests for doc-inventory-reconcile.py — catalog-path resolution + bucketing.

AC drift note: AC2 frames this as a 'round-trip test for the 10-col tsv'. The
script is read-only on the tsv (it diffs catalog vs tsv and prints stats; tsv
is 8 cols, not 10). Tests cover the actual code surface: resolve_catalog_path,
load_tsv, and the TSV_NAME_EXCLUDES filter logic. Documented on card #2514.
"""
import sys
sys.dont_write_bytecode = True  # avoid __pycache__ pollution next to the loaded script
import importlib.util
import os
import tempfile
import unittest
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
SCRIPT = SCRIPT_DIR.parent / "scripts" / "doc-inventory-reconcile.py"

spec = importlib.util.spec_from_file_location("reconcile_mod", SCRIPT)
reconcile_mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reconcile_mod)


class ResolveCatalogPathTests(unittest.TestCase):
    """resolve_catalog_path probes existing files at base/dir/fn{,.md,.html}."""

    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.gathering = os.path.join(self.tmp, "gathering")
        self.chorus = os.path.join(self.tmp, "chorus")
        for d in [
            f"{self.gathering}/public",
            f"{self.gathering}/public/gathering-docs",
            f"{self.gathering}/public/akasha",
            f"{self.gathering}/data/about",
            f"{self.chorus}/roles/wren/artifacts",
            f"{self.chorus}/designing/docs",
        ]:
            os.makedirs(d, exist_ok=True)
        self._orig_g = reconcile_mod.GATHERING
        self._orig_c = reconcile_mod.CHORUS
        reconcile_mod.GATHERING = self.gathering
        reconcile_mod.CHORUS = self.chorus

    def tearDown(self):
        reconcile_mod.GATHERING = self._orig_g
        reconcile_mod.CHORUS = self._orig_c

    def _touch(self, base_attr, rel):
        base = getattr(reconcile_mod, base_attr)
        path = os.path.join(base, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write("x")

    def test_resolves_gathering_docs_md(self):
        self._touch("GATHERING", "public/gathering-docs/home.md")
        self.assertEqual(
            reconcile_mod.resolve_catalog_path("/gathering-docs/home", "gathering-docs"),
            ("gathering", "public/gathering-docs/home.md"),
        )

    def test_resolves_html_when_md_absent(self):
        self._touch("GATHERING", "public/akasha/poem.html")
        self.assertEqual(
            reconcile_mod.resolve_catalog_path("/akasha/poem", "akasha"),
            ("gathering", "public/akasha/poem.html"),
        )

    def test_resolves_chorus_source(self):
        self._touch("CHORUS", "roles/wren/artifacts/note.md")
        self.assertEqual(
            reconcile_mod.resolve_catalog_path("/wren/artifacts/note", "wren/artifacts"),
            ("chorus", "roles/wren/artifacts/note.md"),
        )

    def test_unknown_source_returns_none(self):
        self.assertIsNone(reconcile_mod.resolve_catalog_path("/x/y", "manual"))

    def test_missing_file_returns_none(self):
        # source mapped but file does not exist on disk
        self.assertIsNone(
            reconcile_mod.resolve_catalog_path("/gathering-docs/ghost", "gathering-docs")
        )

    def test_extension_priority_no_ext_first(self):
        # If both exist, the no-extension form wins (loop checks '' first).
        base = reconcile_mod.GATHERING
        os.makedirs(os.path.join(base, "public/gathering-docs"), exist_ok=True)
        bare = os.path.join(base, "public/gathering-docs/index")
        with open(bare, "w") as f:
            f.write("x")
        with open(bare + ".md", "w") as f:
            f.write("x")
        result = reconcile_mod.resolve_catalog_path("/gathering-docs/index", "gathering-docs")
        self.assertEqual(result, ("gathering", "public/gathering-docs/index"))


class LoadTsvTests(unittest.TestCase):
    """load_tsv skips comments and blank lines; returns (repo, rel) pairs."""

    def setUp(self):
        self.tsv_fd, self.tsv_path = tempfile.mkstemp(suffix=".tsv")
        os.close(self.tsv_fd)
        self._orig = reconcile_mod.TSV
        reconcile_mod.TSV = self.tsv_path

    def tearDown(self):
        reconcile_mod.TSV = self._orig
        os.unlink(self.tsv_path)

    def _write_tsv(self, body):
        with open(self.tsv_path, "w") as f:
            f.write(body)

    def test_basic_rows(self):
        self._write_tsv(
            "gathering\tpublic/index.html\tok\tgathering\n"
            "chorus\tdesigning/docs/x.md\tok\tchorus\n"
        )
        rows = reconcile_mod.load_tsv()
        self.assertEqual(rows, [
            ("gathering", "public/index.html"),
            ("chorus", "designing/docs/x.md"),
        ])

    def test_skips_comments_and_blanks(self):
        self._write_tsv(
            "# header comment\n"
            "\n"
            "gathering\tpublic/a.md\n"
            "  \n"
            "# another comment\n"
            "chorus\troles/silas/b.md\n"
        )
        rows = reconcile_mod.load_tsv()
        self.assertEqual(rows, [
            ("gathering", "public/a.md"),
            ("chorus", "roles/silas/b.md"),
        ])

    def test_skips_short_rows(self):
        # Rows with < 2 fields are dropped.
        self._write_tsv("just-one-field\n")
        rows = reconcile_mod.load_tsv()
        self.assertEqual(rows, [])


class TsvNameExcludesTests(unittest.TestCase):
    """The TSV_NAME_EXCLUDES set drives the documented exclusion bucket."""

    def test_known_excludes_present(self):
        # These are the curated-by-design exclusions; regression guard.
        for name in [
            "CLAUDE.md",
            "TEAM_PROTOCOL.md",
            "team-architecture.md",
            "next-session.md",
            "README.md",
        ]:
            self.assertIn(name, reconcile_mod.TSV_NAME_EXCLUDES,
                          f"{name} should be in TSV_NAME_EXCLUDES")

    def test_excludes_filter_basenames(self):
        # The exclusion check uses os.path.basename. Verify the membership
        # contract used by main(): filenames at any depth are matched on basename.
        candidates = [
            ("chorus", "roles/silas/CLAUDE.md"),
            ("chorus", "designing/CLAUDE.md"),
            ("gathering", "data/about/notes.md"),
        ]
        excluded = {(r, p) for (r, p) in candidates
                    if os.path.basename(p) in reconcile_mod.TSV_NAME_EXCLUDES}
        self.assertEqual(excluded, {
            ("chorus", "roles/silas/CLAUDE.md"),
            ("chorus", "designing/CLAUDE.md"),
        })


class SourceMapTests(unittest.TestCase):
    """SOURCE_MAP must cover catalog sources we expect on disk."""

    def test_expected_sources_present(self):
        for source in ["public", "gathering-docs", "chorus-docs", "akasha",
                       "data/about", "wren/artifacts", "architect/adr",
                       "designing/docs"]:
            self.assertIn(source, reconcile_mod.SOURCE_MAP)

    def test_repo_values_constrained(self):
        for source, (repo, _) in reconcile_mod.SOURCE_MAP.items():
            self.assertIn(repo, {"gathering", "chorus"},
                          f"{source} maps to unexpected repo {repo}")


if __name__ == "__main__":
    unittest.main()
