#!/usr/bin/env python3
"""Ask the local symbol index questions (read-only). Build it with tools/symbol_index.py.

    python tools/symbol_query.py info                    inputs, hashes and coverage
    python tools/symbol_query.py name 0x141CE1FF0        proposed names, vtable slots, corpus file
    python tools/symbol_query.py class BipedComponent    class info, parent chain, vtables
    python tools/symbol_query.py methods Entity [--own]  vtable slots of a class
    python tools/symbol_query.py fields BipedComponentBase [--inherited]
    python tools/symbol_query.py find velocity           names / classes containing text
    python tools/symbol_query.py callers 0x14262AF90     call edges into a function (candidates)
    python tools/symbol_query.py callees 0x14262AF90
    python tools/symbol_query.py sql "SELECT ..."        any read-only SELECT

Tables: class, vtable, method, field, name, function, call, meta (VAs are INTEGER; use
printf('0x%X', va)). Every name has a source and confidence: capture-finding (cites a
finding), reflection-vtable (Class::vf<slot>; a slot number, not a method name),
reflection-property (accessor of a reflected property), propagation (low: the callee is
only called from one class's named functions). See docs/SYMBOL_INDEX.md.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import sqlite3
import sys

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_DB = ROOT / "local" / "symbol_index.sqlite"
RANK = "CASE source WHEN 'capture-finding' THEN 0 WHEN 'reflection-vtable' THEN 1 " \
       "WHEN 'reflection-property' THEN 2 ELSE 3 END"


class QueryError(ValueError):
    pass


def connect(path):
    path = Path(path)
    if not path.is_file():
        raise QueryError(f"no symbol index at {path}. Build it: python tools/symbol_index.py --image <mapped image>")
    db = sqlite3.connect(f"file:{path.as_posix()}?mode=ro", uri=True)
    db.execute("PRAGMA query_only = 1")
    db.row_factory = sqlite3.Row
    return db


def parse_va(text):
    t = text.strip().lower().removeprefix("0x").removesuffix(".c")
    try:
        va = int(t, 16)
    except ValueError:
        raise QueryError(f"'{text}' is not a hexadecimal VA") from None
    if not 0 < va < 1 << 64:
        raise QueryError(f"'{text}' is out of range")
    return va


def hx(va):
    return None if va is None else f"0x{va:X}"


def meta(db):
    return {r["key"]: r["value"] for r in db.execute("SELECT key, value FROM meta")}


def corpus_header(db, va):
    """Read the first line of the corpus file for one VA (never modifies it)."""
    corpus = meta(db).get("corpus_dir") or os.environ.get("ACU_DECOMPILED_DIR")
    if not corpus or corpus == "None":
        return None, None
    path = Path(corpus) / f"{va:X}.c"
    if not path.is_file():
        return str(path), None
    with path.open("rb") as f:
        return str(path), f.readline(400).decode("utf-8", "replace").strip()


def find_class(db, name):
    rows = db.execute("SELECT * FROM class WHERE name = ?", (name,)).fetchall()
    if not rows:
        like = db.execute("SELECT name FROM class WHERE name LIKE ? ORDER BY name LIMIT 10",
                          (f"%{name}%",)).fetchall()
        hint = f" Similar: {', '.join(r['name'] for r in like)}" if like else ""
        raise QueryError(f"no class named '{name}'.{hint}")
    return rows


def chain(db, ci):
    out, seen = [], set()
    while ci is not None and ci not in seen:
        seen.add(ci)
        row = db.execute("SELECT * FROM class WHERE ci_va = ?", (ci,)).fetchone()
        if row is None:
            break
        out.append(row)
        ci = row["parent_ci_va"]
    return out


def cmd_info(db, args):
    m = meta(db)
    cov = json.loads(m.pop("coverage", "{}"))
    return dict(meta=m, coverage=cov)


def cmd_name(db, args):
    va = parse_va(args.va)
    names = [dict(name=r["name"], source=r["source"], confidence=r["confidence"],
                  detail=json.loads(r["detail"]))
             for r in db.execute(f"SELECT * FROM name WHERE va = ? ORDER BY {RANK}, name", (va,))]
    slots = [dict(class_name=r["class_name"], slot=r["slot"], slot_offset=hx(r["slot"] * 8),
                  vtable=hx(r["vtable_va"]), origin=r["origin"])
             for r in db.execute("SELECT * FROM method WHERE fn_va = ? ORDER BY slot, class_name", (va,))]
    fn = db.execute("SELECT * FROM function WHERE va = ?", (va,)).fetchone()
    path, header = corpus_header(db, va)
    callers = db.execute("SELECT COUNT(DISTINCT caller_va) FROM call WHERE callee_va = ?", (va,)).fetchone()[0]
    callees = db.execute("SELECT COUNT(DISTINCT callee_va) FROM call WHERE caller_va = ?", (va,)).fetchone()[0]
    return dict(va=hx(va), best=names[0]["name"] if names else None, names=names,
                vtable_slots=slots[:args.limit], vtable_slot_count=len(slots),
                in_corpus=None if fn is None else fn["in_corpus"],
                corpus_name=None if fn is None else fn["corpus_name"],
                corpus_file=path, corpus_header=header, callers=callers, callees=callees)


def class_row(r):
    return dict(name=r["name"], classinfo=hx(r["ci_va"]), getter=hx(r["getter_va"]),
                getter_count=r["getter_count"], class_global=hx(r["global_va"]),
                parent=r["parent_name"], parent_source=r["parent_source"], depth=r["depth"], type_crc=r["crc_own"],
                crc_matches_name=bool(r["crc_ok"]), object_size=hx(r["object_size"]),
                isa_interval=[r["isa_lo"], r["isa_hi"]], properties=r["prop_count"],
                property_count_method=r["prop_count_method"], factory=hx(r["factory_va"]),
                source=r["source"])


def cmd_class(db, args):
    out = []
    for r in find_class(db, args.name):
        vts = [dict(va=hx(v["va"]), slots=v["slot_count"], end=v["end_reason"])
               for v in db.execute("SELECT * FROM vtable WHERE ci_va = ? ORDER BY va", (r["ci_va"],))]
        kids = [k["name"] for k in db.execute(
            "SELECT name FROM class WHERE parent_ci_va = ? ORDER BY name", (r["ci_va"],))]
        out.append(dict(class_row(r), chain=[c["name"] for c in chain(db, r["ci_va"])],
                        vtables=vts, children=kids[:args.limit], child_count=len(kids)))
    return out


def cmd_methods(db, args):
    out = []
    for r in find_class(db, args.name):
        rows = db.execute("""
            SELECT m.*, (SELECT n.name FROM name n WHERE n.va = m.fn_va ORDER BY """ + RANK + """ LIMIT 1) AS best,
                   (SELECT n.confidence FROM name n WHERE n.va = m.fn_va ORDER BY """ + RANK + """ LIMIT 1) AS conf
            FROM method m WHERE m.ci_va = ? ORDER BY m.vtable_va, m.slot""", (r["ci_va"],)).fetchall()
        out.extend(dict(vtable=hx(m["vtable_va"]), slot=m["slot"], slot_offset=hx(m["slot"] * 8),
                        fn=hx(m["fn_va"]), origin=m["origin"], name=m["best"], confidence=m["conf"])
                   for m in rows if not args.own or m["origin"] in ("own", "new-slot"))
    return out


def cmd_fields(db, args):
    rows = find_class(db, args.name)
    out = []
    for r in rows:
        classes = chain(db, r["ci_va"]) if args.inherited else [r]
        for c in reversed(classes):
            for f in db.execute("SELECT * FROM field WHERE ci_va = ? ORDER BY idx", (c["ci_va"],)):
                out.append(dict(owner=c["name"], idx=f["idx"], name=f["name"], name_hash=f["name_hash"],
                                offset=hx(f["offset"]), accessor_offset=hx(f["accessor_offset"]),
                                offset_check=f["offset_check"], type_code=f["type_code"],
                                type=f["type_name"], type_hash=f["type_hash"], flags=f["flags"],
                                accessors=[hx(f[k]) for k in ("fn0", "fn1", "fn2", "fn3") if f[k]]))
    return out


def cmd_find(db, args):
    pat = f"%{args.text}%"
    names = [dict(va=hx(r["va"]), name=r["name"], source=r["source"], confidence=r["confidence"])
             for r in db.execute(f"SELECT * FROM name WHERE name LIKE ? ORDER BY {RANK}, name LIMIT ?",
                                 (pat, args.limit))]
    classes = [r["name"] for r in db.execute("SELECT name FROM class WHERE name LIKE ? ORDER BY name LIMIT ?",
                                             (pat, args.limit))]
    fields = [dict(owner=r["class_name"], name=r["name"], offset=hx(r["offset"])) for r in db.execute(
        "SELECT * FROM field WHERE name LIKE ? ORDER BY class_name, idx LIMIT ?", (pat, args.limit))]
    return dict(classes=classes, names=names, fields=fields)


def edges(db, args, mine, other):
    va = parse_va(args.va)
    rows = db.execute(f"""
        SELECT c.{other} AS va, c.kind,
               (SELECT n.name FROM name n WHERE n.va = c.{other} ORDER BY {RANK} LIMIT 1) AS best
        FROM call c WHERE c.{mine} = ? ORDER BY c.{other} LIMIT ?""", (va, args.limit)).fetchall()
    return [dict(va=hx(r["va"]), kind=r["kind"], name=r["best"]) for r in rows]


def cmd_sql(db, args):
    if not args.query.lstrip().lower().startswith(("select", "with")):
        raise QueryError("only SELECT/WITH queries are allowed")
    cur = db.execute(args.query)
    return [dict(zip([d[0] for d in cur.description], row)) for row in cur.fetchmany(args.limit)]


def render(value, indent=0):
    pad = "  " * indent
    if isinstance(value, list):
        if not value:
            print(pad + "(none)")
        for item in value:
            if isinstance(item, dict):
                print(pad + "- " + ", ".join(f"{k}={v}" for k, v in item.items() if v not in (None, [], {})))
            else:
                print(pad + f"- {item}")
    elif isinstance(value, dict):
        for k, v in value.items():
            if isinstance(v, (list, dict)) and v:
                print(f"{pad}{k}:")
                render(v, indent + 1)
            elif v not in (None, [], {}):
                print(f"{pad}{k}: {v}")
    else:
        print(pad + str(value))


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--db", type=Path, default=DEFAULT_DB)
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--limit", type=int, default=200)
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("info")
    for name in ("name", "callers", "callees"):
        sub.add_parser(name).add_argument("va")
    for name in ("class", "methods", "fields"):
        p = sub.add_parser(name)
        p.add_argument("name")
        if name == "methods":
            p.add_argument("--own", action="store_true", help="only own/new-slot entries")
        if name == "fields":
            p.add_argument("--inherited", action="store_true", help="include parent classes")
    sub.add_parser("find").add_argument("text")
    sub.add_parser("sql").add_argument("query")
    args = ap.parse_args(argv)
    handlers = dict(info=cmd_info, name=cmd_name, cls=cmd_class, methods=cmd_methods,
                    fields=cmd_fields, find=cmd_find, sql=cmd_sql,
                    callers=lambda d, a: edges(d, a, "callee_va", "caller_va"),
                    callees=lambda d, a: edges(d, a, "caller_va", "callee_va"))
    db = None
    try:
        db = connect(args.db)
        result = handlers["cls" if args.cmd == "class" else args.cmd](db, args)
    except (QueryError, sqlite3.Error) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    finally:
        if db is not None:
            db.close()
    if args.json:
        print(json.dumps(result, indent=2))
    else:
        render(result)
    return 0


if __name__ == "__main__":
    sys.exit(main())
