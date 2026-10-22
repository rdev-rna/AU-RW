"""Baseline hygiene classification on synthetic trees; no game data required."""

import os
from pathlib import Path
import tempfile
import unittest
import unittest.mock

import baseline


def touch(path, data=b""):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


class BaselineTests(unittest.TestCase):
    def build(self, root):
        touch(root / "DataPC.forge", b"A" * 10)
        touch(root / "DataPC_patch_01.forge", b"B" * 4)
        touch(root / "dlc_10" / "DataPC_10_dlc.forge", b"C" * 6)
        touch(root / "dlc_10" / "sfx.pck")
        touch(root / "ACU.exe")
        touch(root / "ACUFixes" / "plugins" / "AssetOverrides" / "x" / "1.LODSelector")
        touch(root / "ACUFixes" / "plugins" / "AssetOverrides" / "x" / "2.LODSelector")
        touch(root / "ACUFixes" / "plugins" / "AssetOverrides" / "load_order.json")
        touch(root / "ACUFixes" / "plugins" / "ACUFixes.dll")
        touch(root / "ACUFixesAndPluginLoader-readme.txt")
        touch(root / "Backups" / "DataPC.forge", b"A" * 10)
        touch(root / "Extracted" / "DataPC_ACU_Paris.forge" / "0_-_Meta.MetaFile")
        touch(root / "game backup.rar", b"R")
        touch(root / "ACU_dxgi.log", b"x")
        (root / "videos").mkdir()

    def test_carriers_are_only_original_forge_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.build(root)
            report = baseline.classify(root)
            self.assertEqual(
                [c["path"] for c in report["carriers"]],
                ["DataPC.forge", "DataPC_patch_01.forge", "dlc_10/DataPC_10_dlc.forge"])
            self.assertEqual(report["carrier_count"], 3)

    def test_exclusions_carry_rule_reason_and_counts(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.build(root)
            report = baseline.classify(root)
            by_path = {e["path"]: e for e in report["excluded"]}
            self.assertEqual(set(by_path), {
                "ACUFixes", "ACUFixesAndPluginLoader-readme.txt", "Backups",
                "Extracted", "game backup.rar", "ACU_dxgi.log"})
            self.assertEqual(by_path["ACUFixes"]["rule"], "mod-layer")
            self.assertNotIn("files", by_path["ACUFixes"])
            self.assertEqual(by_path["Backups"]["rule"], "non-original-copy")
            self.assertEqual(by_path["game backup.rar"]["rule"], "non-original-copy")
            self.assertEqual(by_path["ACU_dxgi.log"]["rule"], "runtime-log")
            for item in report["excluded"]:
                self.assertTrue(item["reason"])

    def test_mod_layer_is_excluded_without_being_opened(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.build(root)
            with unittest.mock.patch.object(baseline.os, "walk",
                                            wraps=baseline.os.walk) as walk:
                baseline.classify(root)
            walked = {Path(c.args[0]).name for c in walk.call_args_list}
            self.assertNotIn("ACUFixes", walked)

    def test_forge_named_directory_is_not_a_carrier(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.build(root)
            touch(root / "Stray.forge" / "inner.bin")
            report = baseline.classify(root)
            self.assertNotIn("Stray.forge", [c["path"] for c in report["carriers"]])
            self.assertIn("Stray.forge", [o["path"] for o in report["other_top_level"]])
            extracted = next(e for e in report["excluded"] if e["path"] == "Extracted")
            self.assertEqual(extracted["forge_named_entries"],
                             [{"name": "DataPC_ACU_Paris.forge", "is_dir": True,
                               "size": None, "same_size_as_baseline": False}])

    def test_backup_copy_is_flagged_by_size_not_trusted(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.build(root)
            backups = next(e for e in baseline.classify(root)["excluded"]
                           if e["path"] == "Backups")
            self.assertTrue(backups["forge_named_entries"][0]["same_size_as_baseline"])
            self.assertNotIn("Backups/DataPC.forge",
                             [c["path"] for c in baseline.classify(root)["carriers"]])

    def test_rule_names_match_case_insensitively(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            touch(root / "DataPC.forge")
            touch(root / "backups" / "a.bin")
            report = baseline.classify(root)
            self.assertEqual([e["path"] for e in report["excluded"]], ["backups"])

    def test_missing_directory_fails_clearly(self):
        with self.assertRaises(ValueError):
            baseline.classify(Path("definitely/not/here"))

    def test_empty_install_has_no_carriers_and_no_error(self):
        with tempfile.TemporaryDirectory() as tmp:
            report = baseline.classify(Path(tmp))
            self.assertEqual(report["carrier_count"], 0)

    @unittest.skipUnless(hasattr(os, "symlink"), "symlinks unavailable")
    def test_symlinked_directory_is_not_followed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "game"
            outside = Path(tmp) / "outside"
            touch(outside / "Big.forge", b"Z")
            root.mkdir()
            touch(root / "DataPC.forge")
            try:
                os.symlink(outside, root / "dlc_99", target_is_directory=True)
            except OSError:
                self.skipTest("symlink creation not permitted")
            report = baseline.classify(root)
            self.assertEqual([c["path"] for c in report["carriers"]], ["DataPC.forge"])


if __name__ == "__main__":
    unittest.main()
