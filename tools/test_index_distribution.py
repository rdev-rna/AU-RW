"""Packaging and release-download of the asset index, end to end; synthetic data and a local
HTTP server standing in for GitHub Releases."""

import contextlib
import gzip
import http.server
import io
import json
import os
from pathlib import Path
import sqlite3
import tempfile
import threading
import time
import unittest
from unittest import mock

import asset_graph as ag
import audit_asset_db as au
import build_asset_db as bdb
import index_status as st
import package_asset_index as pk

GAME = "D:/Games/ACU"
ENT, RES = "0000004000000001", "0000004000000002"
SUMMARY = {"target_ids": 3, "entries_scanned": 1, "entry_decode_failures": 0}


def make_index(root):
    scan = root / "entries.jsonl"
    row = {"archive": f"{GAME}/A.forge", "entry": {"file_id": "00000040000000a0", "index": 0,
           "offset": 0, "record_offset": 1, "stored_length": 8}, "stored_sha256": "s",
           "resources": [
               {"file_id": f, "type_id": "t1", "name": n, "record_offset": 0, "record_length": 8,
                "payload": {"start": 0, "end": 8}, "payload_sha256": "p"}
               for f, n in ((ENT, "E"), (RES, "R"))]}
    scan.write_text(json.dumps(row), encoding="utf-8")
    (root / "summary.json").write_text(json.dumps({"manifest": [
        {"path": f"{GAME}/A.forge", "length": 1000}]}), encoding="utf-8")
    index = root / "index.sqlite"
    bdb.build(scan, index, GAME)
    tsv = root / "links.tsv"
    tsv.write_text("archive\tentry\tordinal\tsrc_file_id\toffset\tdst_file_id\thits\n"
                   f"A.forge\t0\t0\t{ENT}\t8\t{RES}\t1\n", encoding="utf-8")
    ag.merge(index, [tsv])
    return index


class Server:
    """Serves one blob like a release asset: honours Range unless told not to."""

    def __init__(self, blob, mode="normal"):
        self.blob, self.mode, self.ranges = blob, mode, []
        outer = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_GET(self):
                if outer.mode == "404":
                    self.send_error(404)
                    return
                data, status, start = outer.blob, 200, 0
                header = self.headers.get("Range")
                outer.ranges.append(header)
                if header and outer.mode != "ignore_range":
                    start = int(header.split("=")[1].rstrip("-"))
                    data, status = data[start:], 206
                if outer.mode == "truncate":
                    data = data[: len(data) // 2]
                self.send_response(status)
                self.send_header("Content-Length", str(len(outer.blob) if outer.mode == "truncate"
                                                       else len(data)))
                self.end_headers()
                try:
                    self.wfile.write(data)
                except OSError:
                    pass

        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.url = f"http://127.0.0.1:{self.httpd.server_address[1]}/asset_index.sqlite.gz"
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()

    def close(self):
        self.httpd.shutdown()
        self.httpd.server_close()


class PackageTests(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.root = Path(self.dir.name)
        self.index = make_index(self.root)
        self.dist, self.data = self.root / "dist", self.root / "data"
        patcher = mock.patch.dict(au.EXPECTED, {
            "archives": (1, "t"), "entries_total": (1, "t"), "control_entries": (0, "t")})
        patcher.start()
        self.addCleanup(patcher.stop)

    def tearDown(self):
        self.dir.cleanup()

    def package(self, **kw):
        kw.setdefault("repo", "owner/name")
        return pk.package(self.index, self.dist, SUMMARY, manifest_dir=self.data, **kw)

    def test_archive_stays_out_of_the_committed_directory(self):
        manifest = self.package(version="2026-10-22.1")
        self.assertTrue((self.dist / "asset_index.sqlite.gz").is_file())
        self.assertEqual(sorted(p.name for p in self.data.iterdir()),
                         ["asset_index.manifest.json"])
        self.assertEqual(manifest["url"], "https://github.com/owner/name/releases/download/"
                         "asset-index-2026-10-22.1/asset_index.sqlite.gz")
        self.assertEqual(manifest["gz_bytes"], (self.dist / "asset_index.sqlite.gz").stat().st_size)

    def test_scrubs_paths_stamps_the_version_and_round_trips(self):
        manifest = self.package(version="v9")
        unpacked = self.root / "unpacked.sqlite"
        with gzip.open(self.dist / "asset_index.sqlite.gz", "rb") as src:
            unpacked.write_bytes(src.read())
        db = sqlite3.connect(unpacked)
        meta = dict(db.execute("SELECT key, value FROM meta"))
        db.close()
        self.assertEqual((meta["game_dir"], meta["index_version"]), ("<install>", "v9"))
        self.assertFalse(any(pk.PATH_LIKE.search(v) for v in meta.values()))
        self.assertEqual(st.sha256_of(unpacked), manifest["sqlite_sha256"])
        self.assertEqual(manifest["counts"]["entries"], 1)

    def test_the_working_index_is_not_modified(self):
        before = st.sha256_of(self.index)
        self.package()
        self.assertEqual(st.sha256_of(self.index), before)

    def test_refuses_to_package_an_index_with_a_hole(self):
        db = sqlite3.connect(self.index)
        db.execute("UPDATE meta SET value = '3' WHERE key = 'link_orphans'")
        db.commit()
        db.close()
        with self.assertRaises(pk.PackageError) as caught:
            self.package()
        self.assertIn("merge dropped no rows", str(caught.exception))
        self.assertFalse((self.dist / "asset_index.sqlite.gz").exists())
        self.assertFalse(self.data.exists() and any(self.data.iterdir()))

    def test_missing_index_and_unknown_repo_fail_clearly(self):
        with self.assertRaises(pk.PackageError):
            pk.package(self.root / "nope.sqlite", self.dist, repo="o/n")
        with mock.patch.object(pk, "repo_from_remote", return_value=None):
            with self.assertRaises(pk.PackageError) as caught:
                pk.package(self.index, self.dist, SUMMARY)
        self.assertIn("--repo", str(caught.exception))

    def test_repo_is_read_from_common_remote_shapes(self):
        for url in ("https://github.com/rdev-rna/AU-RW.git", "git@github.com:rdev-rna/AU-RW.git",
                    "https://github.com/rdev-rna/AU-RW"):
            fake = mock.Mock(stdout=url + "\n")
            with mock.patch.object(pk.subprocess, "run", return_value=fake):
                self.assertEqual(pk.repo_from_remote(), "rdev-rna/AU-RW", url)


class DownloadTests(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.root = Path(self.dir.name)
        patcher = mock.patch.dict(au.EXPECTED, {
            "archives": (1, "t"), "entries_total": (1, "t"), "control_entries": (0, "t")})
        patcher.start()
        self.addCleanup(patcher.stop)
        index = make_index(self.root)
        self.dist, self.data = self.root / "dist", self.root / "data"
        manifest = pk.package(index, self.dist, SUMMARY, version="v1", manifest_dir=self.data,
                              repo="o/n")
        self.blob = (self.dist / "asset_index.sqlite.gz").read_bytes()
        self.manifest_path = self.data / "asset_index.manifest.json"
        self.paths = dict(manifest_path=self.manifest_path,
                          archive=self.root / "local" / "downloads" / "asset_index.sqlite.gz",
                          local=self.root / "local" / "asset_index.sqlite",
                          lock=self.root / "local" / "downloads" / "fetch.lock")
        self.servers = []

    def tearDown(self):
        for server in self.servers:
            server.close()
        self.dir.cleanup()

    def serve(self, blob=None, mode="normal"):
        server = Server(self.blob if blob is None else blob, mode)
        self.servers.append(server)
        manifest = json.loads(self.manifest_path.read_text())
        manifest["url"] = server.url
        self.manifest_path.write_text(json.dumps(manifest))
        return server

    def fetch(self, stream=None):
        return st.fetch_and_unpack(stream=stream or io.StringIO(), allow_any_url=True,
                                   use_curl=False, **self.paths)

    def state(self):
        return st.status(**self.paths)[0]

    def test_fetch_installs_reports_ready_and_leaves_no_lock_or_partial(self):
        self.serve()
        self.assertEqual(self.state(), "need_fetch")
        self.assertEqual(self.fetch(), "installed")
        self.assertEqual(self.state(), "ready")
        self.assertEqual(st.local_version(self.paths["local"]), "v1")
        self.assertFalse(self.paths["lock"].exists())
        self.assertFalse(self.paths["archive"].with_name("asset_index.sqlite.gz.partial").exists())

    def test_fetch_when_already_ready_does_nothing(self):
        self.serve()
        self.fetch()
        self.assertEqual(self.fetch(), "ready")

    def test_non_terminal_progress_prints_one_line_per_ten_percent(self):
        self.serve()
        out = io.StringIO()
        self.fetch(out)
        bars = [line for line in out.getvalue().splitlines() if line.startswith("[")]
        self.assertTrue(bars)
        self.assertLessEqual(len(bars), 11)
        self.assertIn("100.0%", bars[-1])
        self.assertIn("MB/s", bars[-1])
        self.assertTrue(all(line.endswith("Database by R. :3") for line in bars))
        self.assertIn("Database by R. :3", out.getvalue().splitlines()[0])

    def test_interrupted_download_is_resumed_with_a_range_request(self):
        server = self.serve()
        partial = self.paths["archive"].with_name("asset_index.sqlite.gz.partial")
        partial.parent.mkdir(parents=True)
        half = len(self.blob) // 2
        partial.write_bytes(self.blob[:half])
        self.assertEqual(self.fetch(), "installed")
        self.assertEqual(server.ranges, [f"bytes={half}-"])
        self.assertEqual(self.state(), "ready")

    def test_server_that_ignores_range_restarts_cleanly(self):
        self.serve(mode="ignore_range")
        partial = self.paths["archive"].with_name("asset_index.sqlite.gz.partial")
        partial.parent.mkdir(parents=True)
        partial.write_bytes(self.blob[:50])
        self.assertEqual(self.fetch(), "installed")
        self.assertEqual(self.state(), "ready")

    def test_corrupted_download_is_rejected_deleted_and_not_installed(self):
        bad = bytearray(self.blob)
        bad[len(bad) // 2] ^= 0xFF
        self.serve(bytes(bad))
        with self.assertRaises(st.FetchError) as caught:
            self.fetch()
        self.assertIn("checksum", str(caught.exception))
        self.assertFalse(self.paths["archive"].exists())
        self.assertFalse(self.paths["local"].exists())
        self.assertFalse(self.paths["lock"].exists())

    def test_removed_release_gives_a_useful_message(self):
        self.serve(mode="404")
        with self.assertRaises(st.FetchError) as caught:
            self.fetch()
        self.assertIn("HTTP 404", str(caught.exception))
        self.assertIn("removed", str(caught.exception))
        self.assertFalse(self.paths["lock"].exists())

    def test_short_transfer_keeps_the_partial_for_resume(self):
        self.serve(mode="truncate")
        with self.assertRaises(st.FetchError) as caught:
            self.fetch()
        self.assertIn("incomplete", str(caught.exception))
        partial = self.paths["archive"].with_name("asset_index.sqlite.gz.partial")
        self.assertTrue(partial.exists())
        self.assertFalse(self.paths["archive"].exists())

    def test_only_github_urls_are_accepted_by_default(self):
        with self.assertRaises(st.FetchError) as caught:
            st.download("http://127.0.0.1:9/x", self.paths["archive"], 10)
        self.assertIn("must start with", str(caught.exception))
        with self.assertRaises(st.FetchError):
            st.download("https://evil.example/github.com/x", self.paths["archive"], 10)

    def test_manifest_without_a_url_says_the_release_is_not_published(self):
        manifest = json.loads(self.manifest_path.read_text())
        manifest["url"] = ""
        self.manifest_path.write_text(json.dumps(manifest))
        with self.assertRaises(st.FetchError) as caught:
            self.fetch()
        self.assertIn("not published", str(caught.exception))

    def test_curl_is_skipped_when_not_installed(self):
        with mock.patch.object(st.shutil, "which", return_value=None):
            self.assertIsNone(st.curl_download("https://github.com/x", self.paths["archive"], 1))

    def test_not_enough_disk_space_is_reported_before_writing(self):
        self.serve()
        with mock.patch.object(st.shutil, "disk_usage", return_value=mock.Mock(free=10)):
            with self.assertRaises(st.FetchError) as caught:
                self.fetch()
        self.assertIn("GiB free", str(caught.exception))
        self.assertFalse(self.paths["local"].exists())


class StatusTests(DownloadTests):
    def test_states_walk_from_fetch_to_unpack_to_ready(self):
        self.serve()
        self.assertEqual(self.state(), "need_fetch")
        manifest = json.loads(self.manifest_path.read_text())
        st.download(manifest["url"], self.paths["archive"], len(self.blob), io.StringIO(),
                    allow_any_url=True)
        self.assertEqual(self.state(), "need_fetch")  # downloaded but not yet verified
        st.verify_download(self.paths["archive"], manifest)
        self.assertEqual(self.state(), "need_unpack")
        st.unpack(self.manifest_path, self.paths["archive"], self.paths["local"])
        self.assertEqual(self.state(), "ready")

    def test_new_manifest_version_is_reported_as_stale(self):
        self.serve()
        self.fetch()
        manifest = json.loads(self.manifest_path.read_text())
        manifest["index_version"] = "v2"
        manifest["gz_sha256"] = "f" * 64   # a real new version is a different file
        self.manifest_path.write_text(json.dumps(manifest))
        state, message, details = st.status(**self.paths)
        self.assertEqual(state, "need_fetch")
        self.assertTrue(details["stale"])
        self.assertIn("NEW", message)
        self.assertIn("v2", message)
        self.assertIn("v1", message)

    def test_same_version_label_with_the_same_archive_needs_no_new_download(self):
        self.serve()
        self.fetch()
        manifest = json.loads(self.manifest_path.read_text())
        manifest["index_version"] = "v1b"
        self.manifest_path.write_text(json.dumps(manifest))
        state, _, details = st.status(**self.paths)
        self.assertEqual((state, details["stale"]), ("need_unpack", True))

    def test_an_older_archive_with_the_same_size_is_not_mistaken_for_the_new_one(self):
        self.serve()
        self.fetch()
        manifest = json.loads(self.manifest_path.read_text())
        manifest["index_version"], manifest["gz_sha256"] = "v2", "e" * 64
        self.manifest_path.write_text(json.dumps(manifest))
        self.assertEqual(st.status(**self.paths)[0], "need_fetch")  # same gz_bytes, other hash

    def test_a_fresh_lock_means_a_download_is_in_progress_and_an_old_one_is_ignored(self):
        self.paths["lock"].parent.mkdir(parents=True)
        self.paths["lock"].write_text(json.dumps({"pid": 1, "started": time.time()}))
        self.assertEqual(self.state(), "fetching")
        self.paths["lock"].write_text(json.dumps({"pid": 1, "started": time.time() - 7200}))
        self.assertEqual(self.state(), "need_fetch")
        self.paths["lock"].write_text("garbage")
        self.assertEqual(self.state(), "need_fetch")

    def test_missing_and_unreadable_manifest(self):
        self.manifest_path.unlink()
        self.assertEqual(self.state(), "missing")
        self.manifest_path.write_text("{broken")
        self.assertEqual(self.state(), "corrupt")


class MainTests(DownloadTests):
    def run_main(self, *argv, spawner=None):
        out = io.StringIO()
        with mock.patch.multiple(st, MANIFEST=self.paths["manifest_path"],
                                 ARCHIVE=self.paths["archive"], LOCAL=self.paths["local"],
                                 LOCK=self.paths["lock"]):
            with contextlib.redirect_stdout(out):
                code = st.main(list(argv), spawner=spawner)
        return code, out.getvalue()

    def test_hook_starts_the_download_in_the_background_and_never_fails(self):
        spawned = []
        code, out = self.run_main("--hook", spawner=lambda: spawned.append(1))
        self.assertEqual((code, len(spawned)), (0, 1))
        self.assertIn("background", out)
        self.assertIn("must be downloaded", out)

    def test_hook_does_not_spawn_when_ready_or_already_downloading(self):
        self.serve()
        self.fetch()
        spawned = []
        code, out = self.run_main("--hook", spawner=lambda: spawned.append(1))
        self.assertEqual((code, spawned), (0, []))
        self.assertIn("READY", out)
        self.assertIn("asset_query.py", out)
        os.remove(self.paths["local"])
        self.paths["lock"].write_text(json.dumps({"pid": 1, "started": time.time()}))
        code, out = self.run_main("--hook", spawner=lambda: spawned.append(1))
        self.assertEqual((code, spawned), (0, []))
        self.assertIn("in progress", out)

    def test_plain_mode_exit_code_reflects_readiness_and_tells_what_to_run(self):
        code, out = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("--fetch", out)
        self.serve()
        self.fetch()
        self.assertEqual(self.run_main()[0], 0)

    def test_fetch_failure_is_reported_not_raised(self):
        self.serve(mode="404")
        code, out = self.run_main("--fetch")
        self.assertEqual(code, 1)
        self.assertIn("NOT usable", out)


if __name__ == "__main__":
    unittest.main()
