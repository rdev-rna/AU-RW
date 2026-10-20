"""Evidence-aware SQLite catalogue of animation occurrences in a coverage JSONL.

Only metadata is copied; no animation payloads are extracted. Generated databases
belong under local/. Names describe discovery hints, never proven actor ownership.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sqlite3
import tempfile


TYPES = {
    "0fa3067f": "Animation",
    "0181efe8": "AnimTrackData",
    "653caa76": "AnimTrackDataMapping",
    "41682213": "AnimTrackEvent",
}
ROLE_PATTERNS = {
    "walking": r"walk",
    "sprinting": r"sprint",
    "running": r"(?:^|_)run(?:_|$)|jog",
    "climbing": r"climb|ladder",
    "idle": r"idle",
    "traversal": r"parkour|freerun|vault|ledge|wallrun",
    "jumping": r"jump|leap",
    "combat": r"attack|parry|assassinat|fight",
    "cinematic": r"(?:^|_)cin(?:_|$)|cinematic",
    "facial": r"facial|(?:^|_)face(?:_|$)|lip.?sync",
}
SCHEMA = """
PRAGMA user_version=1;
CREATE TABLE metadata(key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE occurrences(
 id INTEGER PRIMARY KEY, archive TEXT NOT NULL, entry_index INTEGER NOT NULL,
 entry_file_id TEXT NOT NULL CHECK(length(entry_file_id)=16),
 entry_offset INTEGER NOT NULL, stored_length INTEGER NOT NULL,
 stored_sha256 TEXT NOT NULL, decoded_sha256 TEXT NOT NULL,
 resource_index INTEGER NOT NULL, file_id TEXT NOT NULL CHECK(length(file_id)=16),
 type_id TEXT NOT NULL, name TEXT NOT NULL,
 payload_start INTEGER NOT NULL, payload_end INTEGER NOT NULL,
 payload_sha256 TEXT NOT NULL, source_line INTEGER NOT NULL,
 UNIQUE(archive,entry_index,resource_index));
CREATE INDEX occurrence_id ON occurrences(file_id);
CREATE INDEX occurrence_type ON occurrences(type_id);
CREATE TABLE role_hints(
 occurrence_id INTEGER NOT NULL REFERENCES occurrences(id), role TEXT NOT NULL,
 evidence_kind TEXT NOT NULL CHECK(evidence_kind='name_hint'),
 PRIMARY KEY(occurrence_id,role));
CREATE TABLE bindings(
 id INTEGER PRIMARY KEY,
 occurrence_id INTEGER NOT NULL REFERENCES occurrences(id),
 relation TEXT NOT NULL CHECK(relation IN ('actor','role','rig','track_data','bone','event','reference')),
 target TEXT NOT NULL, evidence_kind TEXT NOT NULL
 CHECK(evidence_kind IN ('decoded_reference','runtime_observation','manual_hypothesis')),
 provenance TEXT NOT NULL, note TEXT NOT NULL);
CREATE INDEX binding_occurrence ON bindings(occurrence_id);
CREATE TABLE tracks(
 occurrence_id INTEGER NOT NULL REFERENCES occurrences(id), track_index INTEGER NOT NULL,
 bone_hash TEXT, channel TEXT, codec TEXT, key_count INTEGER,
 duration_seconds REAL, data_offset INTEGER, data_length INTEGER,
 decoder TEXT NOT NULL, provenance TEXT NOT NULL,
 PRIMARY KEY(occurrence_id,track_index));
"""


def hex_id(value, digits):
    if not isinstance(value, str) or not re.fullmatch(rf"[0-9a-fA-F]{{{digits}}}", value):
        raise ValueError(f"expected {digits}-digit hexadecimal string: {value!r}")
    return value.lower()


def natural(value):
    if type(value) is not int or not 0 <= value <= (1 << 63) - 1:
        raise ValueError(f"invalid bounded nonnegative integer: {value!r}")
    return value


def build(source, database):
    """Build atomically; refuse to discard an existing annotated database."""
    source, destination = Path(source), Path(database)
    if destination.exists():
        raise ValueError("destination already exists; choose a new snapshot path")
    destination.parent.mkdir(parents=True, exist_ok=True)
    handle, temporary = tempfile.mkstemp(prefix=destination.name + ".", suffix=".tmp", dir=destination.parent)
    os.close(handle)
    con = None
    digest = hashlib.sha256()
    lines = resources = occurrences = 0
    try:
        con = sqlite3.connect(temporary)
        con.executescript("PRAGMA foreign_keys=ON;" + SCHEMA)
        with con, source.open("rb") as stream:
            for lines, line in enumerate(stream, 1):
                digest.update(line)
                record = json.loads(line)
                for index, resource in enumerate(record.get("resources", [])):
                    resources += 1
                    if resource.get("type_id") not in TYPES:
                        continue
                    entry, payload = record["entry"], resource["payload"]
                    start, end = natural(payload["start"]), natural(payload["end"])
                    if end < start:
                        raise ValueError(f"reversed payload range at line {lines}")
                    cursor = con.execute(
                        "INSERT INTO occurrences VALUES(NULL,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                        (record["archive"], natural(entry["index"]), hex_id(entry["file_id"], 16),
                         natural(entry["offset"]), natural(entry["stored_length"]),
                         hex_id(record["stored_sha256"], 64), hex_id(record["decoded_sha256"], 64),
                         index, hex_id(resource["file_id"], 16), resource["type_id"], resource["name"],
                         start, end, hex_id(resource["payload_sha256"], 64), lines),
                    )
                    occurrence_id = cursor.lastrowid
                    occurrences += 1
                    if resource["type_id"] == "0fa3067f":
                        for role, pattern in ROLE_PATTERNS.items():
                            if re.search(pattern, resource["name"], re.IGNORECASE):
                                con.execute("INSERT INTO role_hints VALUES(?,?,'name_hint')", (occurrence_id, role))
            metadata = {
                "source_path": str(source.resolve()), "source_sha256": digest.hexdigest(),
                "source_lines": lines, "resources_seen": resources,
                "occurrences": occurrences, "type_names": TYPES,
                "scope": "All matching typed envelopes in this scan; not proof all install animations were discovered.",
                "ownership": "Unknown unless supported by an explicit binding; names are hints only.",
            }
            con.executemany("INSERT INTO metadata VALUES(?,?)", ((k, json.dumps(v)) for k, v in metadata.items()))
        if con.execute("PRAGMA quick_check").fetchone()[0] != "ok":
            raise ValueError("SQLite integrity check failed")
        con.close()
        con = None
        # Publish the finished inode without overwriting a concurrently created
        # snapshot. Both paths are on the same filesystem; unlink temp in finally.
        os.link(temporary, destination)
        return metadata
    finally:
        if con is not None:
            con.close()
        Path(temporary).unlink(missing_ok=True)


def connect(database, writable=False):
    con = sqlite3.connect(Path(database).resolve().as_uri() + ("?mode=rw" if writable else "?mode=ro"), uri=True)
    con.row_factory = sqlite3.Row
    con.execute("PRAGMA foreign_keys=ON")
    if con.execute("PRAGMA user_version").fetchone()[0] != 1:
        con.close()
        raise ValueError("unsupported catalogue schema")
    return con


def query(database, name=None, role=None, actor=None, file_id=None, limit=20):
    if not 1 <= limit <= 500:
        raise ValueError("limit must be between 1 and 500")
    where, params = ["o.type_id='0fa3067f'"], []
    if name:
        where.append("instr(lower(o.name),lower(?))>0")
        params.append(name)
    if file_id:
        where.append("o.file_id=?")
        params.append(hex_id(file_id, 16))
    if role:
        where.append("(EXISTS(SELECT 1 FROM role_hints h WHERE h.occurrence_id=o.id AND h.role=?) OR EXISTS(SELECT 1 FROM bindings b WHERE b.occurrence_id=o.id AND b.relation='role' AND b.target=?))")
        params.extend([role, role])
    if actor:
        # Hypotheses cannot turn an ownership query into a verified association.
        where.append("EXISTS(SELECT 1 FROM bindings b WHERE b.occurrence_id=o.id AND b.relation='actor' AND b.evidence_kind!='manual_hypothesis' AND instr(lower(b.target),lower(?))>0)")
        params.append(actor)
    con = connect(database)
    try:
        sql = "SELECT o.* FROM occurrences o WHERE " + " AND ".join(where) + " ORDER BY o.id LIMIT ?"
        rows = [dict(row) for row in con.execute(sql, params + [limit])]
        for row in rows:
            identity = (row["id"],)
            row["role_hints"] = [dict(x) for x in con.execute("SELECT role,evidence_kind FROM role_hints WHERE occurrence_id=? ORDER BY role", identity)]
            row["bindings"] = [dict(x) for x in con.execute("SELECT relation,target,evidence_kind,provenance,note FROM bindings WHERE occurrence_id=? ORDER BY id", identity)]
            row["tracks"] = [dict(x) for x in con.execute("SELECT * FROM tracks WHERE occurrence_id=? ORDER BY track_index", identity)]
            row["actor_ownership"] = "evidenced_binding" if any(x["relation"] == "actor" and x["evidence_kind"] != "manual_hypothesis" for x in row["bindings"]) else "unknown"
        return rows
    finally:
        con.close()


def annotate(database, occurrence, relation, target, evidence_kind, provenance, note=""):
    if not target.strip() or not provenance.strip():
        raise ValueError("target and reproducible evidence provenance are required")
    con = connect(database, writable=True)
    try:
        with con:
            con.execute("INSERT INTO bindings VALUES(NULL,?,?,?,?,?,?)", (occurrence, relation, target, evidence_kind, provenance, note))
    finally:
        con.close()


def stats(database):
    con = connect(database)
    try:
        return {
            "metadata": {row["key"]: json.loads(row["value"]) for row in con.execute("SELECT * FROM metadata")},
            "types": [dict(x) for x in con.execute("SELECT type_id,COUNT(*) AS occurrences,COUNT(DISTINCT file_id) AS distinct_ids FROM occurrences GROUP BY type_id")],
            "roles": [dict(x) for x in con.execute("SELECT role,COUNT(*) AS name_hints FROM role_hints GROUP BY role ORDER BY role")],
            "bindings": con.execute("SELECT COUNT(*) FROM bindings").fetchone()[0],
            "tracks": con.execute("SELECT COUNT(*) FROM tracks").fetchone()[0],
        }
    finally:
        con.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    create = commands.add_parser("build")
    create.add_argument("source")
    create.add_argument("database")
    find = commands.add_parser("query")
    find.add_argument("database")
    for field in ("name", "role", "actor", "file-id"):
        find.add_argument("--" + field)
    find.add_argument("--limit", type=int, default=20)
    summary = commands.add_parser("stats")
    summary.add_argument("database")
    add = commands.add_parser("annotate")
    add.add_argument("database")
    add.add_argument("occurrence", type=int)
    add.add_argument("relation", choices=["actor", "role", "rig", "track_data", "bone", "event", "reference"])
    add.add_argument("target")
    add.add_argument("--evidence-kind", required=True, choices=["decoded_reference", "runtime_observation", "manual_hypothesis"])
    add.add_argument("--provenance", required=True)
    add.add_argument("--note", default="")
    args = vars(parser.parse_args())
    command = args.pop("command")
    try:
        result = {"build": build, "query": query, "stats": stats, "annotate": annotate}[command](**args)
        print(json.dumps(result, indent=2))
    except (ValueError, OSError, KeyError, sqlite3.Error) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
