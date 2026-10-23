#!/usr/bin/env python3
"""Is the local asset index present and current? Downloads and unpacks it when it can.

    python tools/index_status.py            report status; say what to run if not ready
    python tools/index_status.py --fetch    download (progress bar), verify, unpack
    python tools/index_status.py --unpack   unpack an already-downloaded archive
    python tools/index_status.py --hook     session-start mode: short report, starts any
                                            download in the BACKGROUND, never fails

The index is a GitHub Release asset. The repo holds only data/asset_index.manifest.json (URL,
size, SHA-256, counts); a `git pull` that brings a new manifest is how a new version is noticed.
Standard library only. Writes under local/ (ignored by git) and nothing else.
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "data" / "asset_index.manifest.json"
DOWNLOAD_DIR = ROOT / "local" / "downloads"
ARCHIVE = DOWNLOAD_DIR / "asset_index.sqlite.gz"
LOCAL = ROOT / "local" / "asset_index.sqlite"
LOCK = DOWNLOAD_DIR / "fetch.lock"
LOG = DOWNLOAD_DIR / "fetch.log"
LOCK_MAX_AGE = 30 * 60
ALLOWED_URL_PREFIX = "https://github.com/"
CREDIT = "Database by R. :3"
USE = "Use it: python tools/asset_query.py info | where <id> | find <name> | types | walk <name>"


class FetchError(ValueError):
    pass


def sha256_of(path):
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def number(value):
    return f"{value:,}" if isinstance(value, int) else "?"


def local_version(path):
    """index_version stored inside the unpacked database, or None."""
    try:
        db = sqlite3.connect(f"file:{Path(path).as_posix()}?mode=ro", uri=True, timeout=5)
        try:
            row = db.execute("SELECT value FROM meta WHERE key = 'index_version'").fetchone()
        finally:
            db.close()
        return row[0] if row else None
    except sqlite3.Error:
        return None


def sidecar(archive):
    """File recording the SHA-256 of an archive that was fully downloaded and verified."""
    return Path(archive).with_name(Path(archive).name + ".sha256")


def archive_matches(archive, manifest):
    """True when `archive` is the verified download of exactly this manifest's version.

    Size alone is not trusted: an older version's archive can have the same byte count."""
    try:
        return (Path(archive).is_file()
                and Path(archive).stat().st_size == manifest.get("gz_bytes")
                and sidecar(archive).read_text().strip() == manifest["gz_sha256"])
    except (OSError, KeyError):
        return False


def lock_age(lock=None):
    """Seconds since the lock was taken, or None when there is no (readable) lock."""
    try:
        return time.time() - json.loads(Path(lock or LOCK).read_text())["started"]
    except (OSError, ValueError, KeyError, TypeError):
        return None


def status(manifest_path=None, archive=None, local=None, lock=None):
    """(state, message, details). States: ready, fetching, need_fetch, need_unpack, missing,
    corrupt. `details['stale']` is true when a different version is already unpacked.
    Paths default to the repo's, resolved at call time."""
    manifest_path, archive = manifest_path or MANIFEST, archive or ARCHIVE
    local, lock = local or LOCAL, lock or LOCK
    if not Path(manifest_path).is_file():
        return ("missing", "No asset index in this checkout (data/asset_index.manifest.json "
                "is absent). See docs/ASSET_INDEX.md or ask the maintainer.", {})
    try:
        manifest = json.loads(Path(manifest_path).read_text(encoding="utf-8"))
        version = manifest["index_version"]
    except (ValueError, KeyError, OSError):
        return ("corrupt", "data/asset_index.manifest.json is unreadable.", {})
    counts = manifest.get("counts", {})
    have = local_version(local) if Path(local).is_file() else None
    details = {"version": version, "local_version": have, "counts": counts,
               "stale": have is not None and have != version,
               "download_bytes": manifest.get("gz_bytes")}
    if have == version and Path(local).stat().st_size == manifest.get("sqlite_bytes"):
        return ("ready", f"Asset index READY (version {version}): "
                f"{number(counts.get('archives'))} archives, {number(counts.get('resources'))} "
                f"resources, {number(counts.get('links'))} candidate links.", details)
    prefix = (f"NEW asset index version {version} available (you have {have})"
              if details["stale"] else f"Asset index version {version} is not installed")
    age = lock_age(lock)
    if age is not None and age < LOCK_MAX_AGE:
        return ("fetching", f"{prefix}; a download is in progress "
                f"(started {int(age)}s ago, log: local/downloads/fetch.log).", details)
    if archive_matches(archive, manifest):
        return ("need_unpack", f"{prefix}; the download is complete but not unpacked.", details)
    size = manifest.get("gz_bytes")
    mb = f" (~{size / 2**20:,.0f} MB)" if isinstance(size, int) else ""
    return ("need_fetch", f"{prefix}; it must be downloaded{mb}.", details)


def check_url(url, allow_any=False):
    if not allow_any and not url.startswith(ALLOWED_URL_PREFIX):
        raise FetchError(f"refusing to download from {url!r}: manifest URLs must start with "
                         f"{ALLOWED_URL_PREFIX}")
    return url


def bar(done, total, started, width=30):
    fraction = done / total if total else 0
    rate = done / max(time.time() - started, 1e-6) / 2**20
    filled = int(width * fraction)
    return (f"[{'#' * filled}{'-' * (width - filled)}] {fraction:6.1%}  "
            f"{done / 2**20:8.1f}/{total / 2**20:.1f} MB  {rate:5.1f} MB/s  {CREDIT}")


def download(url, destination, expected_bytes, stream=None, allow_any_url=False):
    """Download with resume and a progress display. Returns the finished path.

    On a terminal the bar redraws in place; otherwise (agents capture stdout) one line is
    printed per 10%. A partial file from an interrupted run is resumed."""
    stream = stream or sys.stdout
    check_url(url, allow_any_url)
    destination = Path(destination)
    destination.parent.mkdir(parents=True, exist_ok=True)
    partial = destination.with_name(destination.name + ".partial")
    have = partial.stat().st_size if partial.exists() else 0
    if have > expected_bytes:
        partial.unlink()
        have = 0
    request = urllib.request.Request(url, headers={"User-Agent": "au-rw-index-fetch"})
    if have:
        request.add_header("Range", f"bytes={have}-")
    try:
        response = urllib.request.urlopen(request, timeout=60)
    except urllib.error.HTTPError as error:
        if error.code == 416 and have == expected_bytes:  # already complete
            response = None
        else:
            raise FetchError(f"download failed: HTTP {error.code} for {url} "
                             "(the release may have been removed; ask the maintainer)") from None
    except (urllib.error.URLError, OSError) as error:
        raise FetchError(f"download failed: {getattr(error, 'reason', error)}") from None
    if response is not None:
        with response:
            if have and response.status != 206:  # server ignored Range: start over
                have = 0
            tty = hasattr(stream, "isatty") and stream.isatty()
            started, done, next_mark = time.time(), have, 10
            with open(partial, "ab" if have else "wb") as target:
                while True:
                    block = response.read(1 << 20)
                    if not block:
                        break
                    target.write(block)
                    done += len(block)
                    if tty:
                        stream.write("\r" + bar(done, expected_bytes, started))
                        stream.flush()
                    elif expected_bytes and done * 100 // expected_bytes >= next_mark:
                        stream.write(bar(done, expected_bytes, started) + "\n")
                        stream.flush()
                        next_mark = done * 100 // expected_bytes // 10 * 10 + 10
            if tty:
                stream.write("\n")
    if partial.stat().st_size != expected_bytes:
        raise FetchError(f"download incomplete: got {partial.stat().st_size:,} of "
                         f"{expected_bytes:,} bytes; run --fetch again to resume")
    os.replace(partial, destination)
    return destination


def curl_download(url, destination, expected_bytes, allow_any_url=False):
    """Use curl's own progress bar (with resume) when attached to a terminal.

    Returns None when curl is not installed, so the caller falls back to `download`."""
    check_url(url, allow_any_url)
    curl = shutil.which("curl")
    if not curl:
        return None
    destination = Path(destination)
    destination.parent.mkdir(parents=True, exist_ok=True)
    partial = destination.with_name(destination.name + ".partial")
    result = subprocess.run([curl, "-L", "--fail", "--progress-bar", "-C", "-", "-o",
                             str(partial), url])
    if result.returncode != 0 or not partial.exists() or \
            partial.stat().st_size != expected_bytes:
        raise FetchError("curl did not complete the download; run --fetch again to resume")
    os.replace(partial, destination)
    return destination


def verify_download(archive, manifest):
    """Check a finished download against the manifest and record that it is good.

    A corrupt archive is deleted so it is never reused."""
    if sha256_of(archive) != manifest["gz_sha256"]:
        Path(archive).unlink(missing_ok=True)
        sidecar(archive).unlink(missing_ok=True)
        raise FetchError("the downloaded archive does not match its manifest checksum; "
                         "run --fetch again")
    sidecar(archive).write_text(manifest["gz_sha256"])


def unpack(manifest_path=None, archive=None, local=None):
    """Verify and unpack the archive into `local`. Raises FetchError with the reason."""
    manifest_path, archive, local = manifest_path or MANIFEST, archive or ARCHIVE, local or LOCAL
    manifest = json.loads(Path(manifest_path).read_text(encoding="utf-8"))
    if not Path(archive).is_file():
        raise FetchError("the archive has not been downloaded; run --fetch")
    if sha256_of(archive) != manifest["gz_sha256"]:
        raise FetchError("the downloaded archive does not match its manifest checksum; "
                         "run --fetch again")
    local = Path(local)
    local.parent.mkdir(parents=True, exist_ok=True)
    needed = manifest["sqlite_bytes"]
    free = shutil.disk_usage(local.parent).free
    if free < needed * 1.1:
        raise FetchError(f"need about {needed / 2**30:.1f} GiB free in {local.parent}, "
                         f"have {free / 2**30:.1f} GiB")
    partial = local.with_name(local.name + ".partial")
    try:
        with gzip.open(archive, "rb") as source, open(partial, "wb") as target:
            shutil.copyfileobj(source, target, 1 << 20)
        if partial.stat().st_size != needed or sha256_of(partial) != manifest["sqlite_sha256"]:
            raise FetchError("the unpacked database does not match the manifest; not installed")
        os.replace(partial, local)
    finally:
        partial.unlink(missing_ok=True)
    return local


def fetch_and_unpack(manifest_path=None, archive=None, local=None, lock=None, stream=None,
                     allow_any_url=False, use_curl=None):
    """The whole job: download if needed, verify, unpack. Safe to call repeatedly."""
    manifest_path, archive = manifest_path or MANIFEST, archive or ARCHIVE
    local, lock = local or LOCAL, lock or LOCK
    stream = stream or sys.stdout
    manifest = json.loads(Path(manifest_path).read_text(encoding="utf-8"))
    if status(manifest_path, archive, local, lock)[0] == "ready":
        return "ready"
    lock = Path(lock)
    lock.parent.mkdir(parents=True, exist_ok=True)
    lock.write_text(json.dumps({"pid": os.getpid(), "started": time.time()}))
    try:
        # Judge the files alone: ignore our own lock when deciding whether a download is due.
        state = status(manifest_path, archive, local, lock.with_name("absent.lock"))[0]
        if state == "need_fetch":
            url = manifest.get("url")
            if not url:
                raise FetchError("the manifest has no download URL (release not published yet)")
            tty = hasattr(stream, "isatty") and stream.isatty()
            print(f"Downloading asset index {manifest['index_version']} "
                  f"({manifest['gz_bytes'] / 2**20:,.0f} MB)  -  {CREDIT}", file=stream,
                  flush=True)
            done = None
            if use_curl if use_curl is not None else tty:
                done = curl_download(url, archive, manifest["gz_bytes"], allow_any_url)
            if done is None:
                download(url, archive, manifest["gz_bytes"], stream, allow_any_url)
            print("Verifying checksum ...", file=stream, flush=True)
            verify_download(archive, manifest)
        print("Unpacking ...", file=stream, flush=True)
        try:
            unpack(manifest_path, archive, local)
        except FetchError as error:
            if "checksum" in str(error):
                Path(archive).unlink(missing_ok=True)
                sidecar(archive).unlink(missing_ok=True)
            raise
    finally:
        lock.unlink(missing_ok=True)
    return "installed"


def spawn_background():
    """Start `index_status.py --fetch` detached, logging to local/downloads/fetch.log."""
    DOWNLOAD_DIR.mkdir(parents=True, exist_ok=True)
    log = open(LOG, "ab")
    kwargs = {"stdout": log, "stderr": log, "stdin": subprocess.DEVNULL, "close_fds": True}
    if os.name == "nt":
        kwargs["creationflags"] = 0x00000008 | 0x00000200  # DETACHED_PROCESS | NEW_PROCESS_GROUP
    else:
        kwargs["start_new_session"] = True
    subprocess.Popen([sys.executable, str(Path(__file__).resolve()), "--fetch"], **kwargs)


def main(argv=None, spawner=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--fetch", action="store_true", help="download if needed, then unpack")
    parser.add_argument("--unpack", action="store_true", help="unpack a downloaded archive")
    parser.add_argument("--hook", action="store_true",
                        help="session-start mode: report, start downloads in the background")
    args = parser.parse_args(argv)
    state, message, details = status()
    try:
        if args.fetch:
            fetch_and_unpack()
            state, message, details = status()
        elif args.unpack:
            unpack()
            state, message, details = status()
        elif args.hook and state in ("need_fetch", "need_unpack"):
            (spawner or spawn_background)()
            message += (" Started in the background; run `python tools/index_status.py` in a "
                        "few minutes (log: local/downloads/fetch.log).")
    except (FetchError, OSError, ValueError) as error:
        state, message = "error", f"Asset index NOT usable: {error}"
    print(message)
    if state == "ready":
        print(USE)
        print("Query the index before scanning archives. Links are byte-match candidates; "
              "type labels carry a confidence level. Guide: docs/ASSET_INDEX_QUICKSTART.md")
    elif state in ("need_fetch", "need_unpack") and not args.hook:
        print("Run: python tools/index_status.py --fetch")
    if args.hook:
        return 0
    return 0 if state == "ready" else 1


if __name__ == "__main__":
    sys.exit(main())
