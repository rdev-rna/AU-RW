"""Portability and source export boundary checks; no game data required."""

import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile
from argparse import Namespace

import acu
import source_bundle


class CollaborationTests(unittest.TestCase):
    def test_configure_preserves_existing_settings(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            args = Namespace(game_dir="my-game", decompiled_dir="my-corpus",
                             for_honor_project=None)
            with patch.object(acu, "ROOT", root):
                acu.configure(args)
                before = (root / "local/project.json").read_bytes()
                with self.assertRaises(FileExistsError):
                    acu.configure(args)
                self.assertEqual(before, (root / "local/project.json").read_bytes())

    def test_external_paths_and_optional_reference(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "config").mkdir()
            (root / "local").mkdir()
            example = {"schema_version": 1, "game_dir": "local/game-install",
                       "decompiled_dir": "local/decompiled"}
            (root / "config/project.example.json").write_text(json.dumps(example))
            with patch.object(acu, "ROOT", root), patch.dict(os.environ, {}, clear=True):
                _, paths = acu.settings()
                self.assertIsNone(paths["for_honor_project"])
                self.assertEqual(paths["game_dir"], (root / "local/game-install").resolve())
                local = dict(example, game_dir="my-game", decompiled_dir="my-corpus")
                (root / "local/project.json").write_text(json.dumps(local))
                _, paths = acu.settings()
                self.assertEqual(paths["decompiled_dir"], (root / "my-corpus").resolve())
                with patch.dict(os.environ, {"ACU_GAME_DIR": str(root / "external-game"),
                                             "ACU_DECOMPILED_DIR": str(root / "external-corpus")}):
                    _, paths = acu.settings()
                    self.assertEqual(paths["game_dir"], (root / "external-game").resolve())
                    self.assertEqual(paths["decompiled_dir"], (root / "external-corpus").resolve())
                with patch.dict(os.environ, {"ACU_DECOMPILED_DIR": ""}):
                    with self.assertRaises(ValueError):
                        acu.settings()

    def test_bundle_excludes_private_data_and_checks_hashes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in source_bundle.ROOT_FILES + ("docs/templates/CAPTURE.json",):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("authored source")
            for name in ("ACU - Decompiled/140001000.c", "local/project.json",
                         "assets/private.rs", "docs/private.forge", "tools/.env",
                         "acu-rs/crates/target/private.rs", "other-decomp/private.c"):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("DO NOT EXPORT")
            output = root / "local/review.zip"
            manifest = source_bundle.bundle(root, output)
            with zipfile.ZipFile(output) as archive:
                self.assertEqual(set(archive.namelist()),
                                 {row["path"] for row in manifest["files"]} | {"SOURCE_MANIFEST.json"})
                for row in manifest["files"]:
                    data = archive.read(row["path"])
                    self.assertNotIn(b"DO NOT EXPORT", data)
                    self.assertEqual(source_bundle.hashlib.sha256(data).hexdigest(), row["sha256"])
            with self.assertRaises(FileExistsError):
                source_bundle.bundle(root, output)
            with self.assertRaises(ValueError):
                source_bundle.bundle(root, root / "public.zip")

    def test_bundle_rejects_linked_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "docs").mkdir()
            target = root / "private.md"
            target.write_text("private")
            link = root / "docs/linked.md"
            try:
                link.symlink_to(target)
            except OSError:
                self.skipTest("symlink creation unavailable")
            with patch.object(source_bundle, "ROOT_FILES", ()):
                with self.assertRaisesRegex(ValueError, "linked source path"):
                    list(source_bundle.source_files(root))


if __name__ == "__main__":
    unittest.main()
