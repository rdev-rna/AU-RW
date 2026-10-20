"""Import archive-backed animation audit results and export all locomotion hints.

Names discover candidates; they never establish Arno ownership or loop eligibility.
The existing catalogue and its annotations are preserved. Each import is atomic.
"""
import argparse
import collections
import csv
import hashlib
import json
import math
from pathlib import Path
import re

from animation_catalogue import connect, hex_id, natural


PATTERN = re.compile(r"nav|locom|walk|sprint|(?:^|_)run(?:_|$)|jog|idle|wait|climb|ladder|parkour|freerun|vault|ledge|wallrun|jump|leap", re.I)


def import_audit(database, source):
    source = Path(source)
    with source.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    con = connect(database, writable=True)
    counts = collections.Counter()
    try:
        with con:
            con.execute("""CREATE TABLE IF NOT EXISTS decode_results(
                occurrence_id INTEGER PRIMARY KEY REFERENCES occurrences(id),
                status TEXT NOT NULL, error TEXT, duration REAL, audit_sha256 TEXT NOT NULL,
                decoder TEXT NOT NULL, provenance TEXT NOT NULL)""")
            con.execute("""CREATE TABLE IF NOT EXISTS track_metrics(
                occurrence_id INTEGER NOT NULL REFERENCES occurrences(id), track_index INTEGER NOT NULL,
                max_component_delta REAL NOT NULL, PRIMARY KEY(occurrence_id,track_index))""")
            # Compare explicit archive paths without mistaking slash conventions for identity.
            identities = {(r["archive"].replace("\\", "/").casefold(), r["entry_index"], r["resource_index"]): r
                          for r in con.execute("SELECT * FROM occurrences")}
            with source.open(encoding="utf-8") as stream:
                for line_number, line in enumerate(stream, 1):
                    r = json.loads(line)
                    key = (r["archive"].replace("\\", "/").casefold(), natural(r["entry"]["index"]), natural(r["resource_index"]))
                    o = identities.get(key)
                    if o is None:
                        raise ValueError(f"occurrence absent at line {line_number}")
                    for k in ("stored_sha256", "decoded_sha256", "payload_sha256"):
                        if hex_id(r[k], 64) != o[k]:
                            raise ValueError(f"{k} mismatch at line {line_number}")
                    if hex_id(r["resource"]["file_id"], 16) != o["file_id"]:
                        raise ValueError(f"file ID mismatch at line {line_number}")
                    if (hex_id(r["entry"]["file_id"], 16) != o["entry_file_id"] or
                            natural(r["entry"]["offset"]) != o["entry_offset"] or
                            natural(r["entry"]["stored_length"]) != o["stored_length"]):
                        raise ValueError(f"archive entry identity mismatch at line {line_number}")
                    if r["resource"]["payload"] != {"start": o["payload_start"], "end": o["payload_end"]}:
                        raise ValueError(f"payload range mismatch at line {line_number}")
                    if r["status"] not in ("decoded", "unsupported"):
                        raise ValueError("unknown audit status")
                    duration = r.get("duration")
                    if r["status"] == "decoded" and (not isinstance(duration, (int, float)) or not math.isfinite(duration) or not 0 < duration <= 600):
                        raise ValueError("invalid decoded duration")
                    provenance = f"{source.resolve()}:{line_number}; sha256={digest}; payload={o['payload_sha256']}"
                    con.execute("INSERT OR REPLACE INTO decode_results VALUES(?,?,?,?,?,?,?)",
                                (o["id"], r["status"], r.get("error"), r.get("duration"), digest, r["decoder"], provenance))
                    con.execute("DELETE FROM tracks WHERE occurrence_id=?", (o["id"],))
                    con.execute("DELETE FROM track_metrics WHERE occurrence_id=?", (o["id"],))
                    for index, t in enumerate(r.get("tracks", [])):
                        if (natural(t["track_index"]) != index or not 0 < natural(t["key_count"]) <= 36001 or
                                natural(t["data_offset"]) + natural(t["data_length"]) > o["payload_end"]-o["payload_start"] or
                                not math.isfinite(t["max_component_delta"]) or t["max_component_delta"] < 0):
                            raise ValueError("invalid decoded track span/count/metric")
                        con.execute("INSERT INTO tracks VALUES(?,?,?,?,?,?,?,?,?,?,?)",
                                    (o["id"], natural(t["track_index"]), hex_id(t["bone_hash"], 8), t["channel"],
                                     str(t["codec"]), natural(t["key_count"]), r["duration"], natural(t["data_offset"]),
                                     natural(t["data_length"]), r["decoder"], provenance))
                        con.execute("INSERT INTO track_metrics VALUES(?,?,?)", (o["id"], t["track_index"], t["max_component_delta"]))
                    counts[r["status"]] += 1
            counts["track_rows"] = con.execute("SELECT COUNT(*) FROM tracks").fetchone()[0]
        return dict(counts)
    finally:
        con.close()


def export_candidates(database, output):
    con = connect(database)
    try:
        decoded = {r["occurrence_id"]: dict(r) for r in con.execute("SELECT * FROM decode_results")}
        rows = [dict(r) for r in con.execute("SELECT * FROM occurrences WHERE type_id='0fa3067f'") if PATTERN.search(r["name"])]
        fields = ["id", "file_id", "name", "archive", "entry_index", "resource_index", "payload_sha256", "status", "error", "duration", "ownership"]
        with Path(output).open("w", encoding="utf-8", newline="") as stream:
            writer = csv.DictWriter(stream, fieldnames=fields, extrasaction="ignore")
            writer.writeheader()
            for r in rows:
                r.update({k: v for k, v in decoded.get(r["id"], {}).items() if k in ("status", "error", "duration")})
                r.setdefault("status", "not_audited")
                r["ownership"] = "unknown; name hint only"
                writer.writerow(r)
        return {"candidate_occurrences": len(rows), "distinct_ids": len({r["file_id"] for r in rows}),
                "decode_status": dict(collections.Counter(r["status"] for r in rows)),
                "scope": "Every matching name in full metadata snapshot; not proven Arno bindings or complete install coverage."}
    finally:
        con.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("database")
    parser.add_argument("audit")
    parser.add_argument("output_csv")
    args = parser.parse_args()
    print(json.dumps({"import": import_audit(args.database, args.audit),
                      "candidates": export_candidates(args.database, args.output_csv)}, indent=2))
