#!/usr/bin/env python3
"""Check ACU co-op reverse-engineering against the definition of done in docs/coop/COVERAGE.md (issue #85).

Inputs:
- the item universe `local/coop/coverage_systems.json` (messages, states and objects per system, ACU-RE-0072);
- tool evidence: `local/coop/message_payloads.json` (fields, ACU-RE-0093), `local/coop/message_handlers.json`
  (handlers, ACU-RE-0098) and `local/coop/memento_codecs.json` (state fields and bind, ACU-RE-0095/0099);
- the verdict file `docs/coop/verdicts.json` (verdicts, behaviour findings, traces, cross-cutting, checklist, adapter);
- `docs/findings/` (every finding ID named must exist as a file).

Criteria (COVERAGE.md "Definition of done"):
- A: every item has fields, a handler, a behaviour finding and a verdict;
- B: every cross-cutting gap and checklist item is closed by a finding;
- C: every "port" system (a system with at least one port item) has a saved live trace;
- D: every port system replays its trace in coop/core (replay) and passes shadow mode against ACU (shadow);
- E: the game-adapter interface is closed and every call has an ACU reference meaning with a finding.

Exits 1 while anything is unknown, unless --no-fail. --update-ledger rewrites the block between
`<!-- coverage-check:begin -->` and `<!-- coverage-check:end -->` in the given Markdown file. Read only otherwise.
"""
from __future__ import annotations

import argparse
import fnmatch
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VERDICTS = ("port", "engine-local", "diagnostic", "acu-only")
BEGIN, END = "<!-- coverage-check:begin -->", "<!-- coverage-check:end -->"
FINDING_ID = re.compile(r"^ACU-RE-\d{4}$")


def finding_ids(findings_dir: Path) -> set[str]:
    return {m.group(0) for p in findings_dir.glob("ACU-RE-*.md") if (m := re.match(r"ACU-RE-\d{4}", p.name))}


def load(path: Path):
    return json.loads(path.read_text(encoding="utf-8")) if path.exists() else None


def verdict_for(name: str, vf: dict) -> dict:
    """Per-item entry, else the first matching pattern, else {}."""
    if name in vf.get("items", {}):
        return vf["items"][name]
    for p in vf.get("patterns", []):
        if fnmatch.fnmatchcase(name, p["match"]):
            return p
    return {}


def build(universe: dict, payloads: dict | None, handlers: dict | None, mementos: dict | None, vf: dict,
          known: set[str], root: Path = ROOT) -> dict:
    pay = {m["name"]: m for m in (payloads or {}).get("messages", [])}
    hnd = {m["name"]: m for m in (handlers or {}).get("messages", [])}
    mem = {m["name"]: m for m in (mementos or {}).get("mementos", [])}
    errors: list[str] = []

    def check_ids(ids, where):
        ok = []
        for i in ids:
            if not FINDING_ID.match(str(i)):
                errors.append(f"{where}: malformed finding id {i!r}")
            elif i not in known:
                errors.append(f"{where}: finding {i} has no file in docs/findings")
            else:
                ok.append(i)
        return ok

    items, port_systems = [], set()
    for system, groups in universe["systems"].items():
        for kind, key in (("message", "messages"), ("state", "states"), ("object", "objects")):
            for name in groups.get(key, []):
                v = verdict_for(name, vf)
                verdict = v.get("verdict")
                if verdict is not None and verdict not in VERDICTS:
                    errors.append(f"{name}: unknown verdict {verdict!r}")
                    verdict = None
                # fields and handler: a finding override, else tool evidence
                if v.get("fields"):
                    fields = bool(check_ids([v["fields"]], f"{name}.fields"))
                elif kind == "message":
                    fields = bool(pay.get(name, {}).get("fields"))
                elif kind == "state":
                    fields = mem.get(name, {}).get("status") in ("decoded", "slot memento")
                else:
                    fields = False
                if v.get("handler"):
                    handler = bool(check_ids([v["handler"]], f"{name}.handler"))
                elif kind == "message":
                    handler = bool(hnd.get(name, {}).get("handler"))
                elif kind == "state":
                    handler = bool(mem.get(name, {}).get("bind") or mem.get(name, {}).get("status") == "slot memento")
                else:
                    handler = False
                behaviour = bool(check_ids(v.get("behaviour", []), f"{name}.behaviour"))
                trace = v.get("trace") or (vf.get("systems", {}).get(system) or {}).get("trace")
                if verdict == "port":
                    port_systems.add(system)
                item = {"name": name, "kind": kind, "system": system, "verdict": verdict, "fields": fields,
                        "handler": handler, "behaviour": behaviour, "trace": bool(trace)}
                item["done"] = bool(verdict and fields and handler and behaviour and (verdict != "port" or trace))
                items.append(item)

    def closed(rows, where):
        return [bool(r.get("finding")) and bool(check_ids([r["finding"]], f"{where}.{r['id']}")) for r in rows]

    cc = closed(vf.get("cross_cutting", []), "cross_cutting")
    cl = closed(vf.get("checklist", []), "checklist")
    sysrows = {s: vf.get("systems", {}).get(s) or {} for s in sorted(port_systems)}
    traced = {s: bool(r.get("trace")) and (root / r["trace"]).exists() for s, r in sysrows.items()}
    for s, r in sysrows.items():
        if r.get("trace") and not traced[s]:
            errors.append(f"systems.{s}: trace {r['trace']} not found locally")
    calls = vf.get("adapter", {}).get("calls", [])
    call_ok = [bool(c.get("acu_reference")) and bool(c.get("finding"))
               and bool(check_ids([c["finding"]], f"adapter.{c['name']}")) for c in calls]

    def pct(xs):
        xs = list(xs)
        return round(100.0 * sum(xs) / len(xs), 1) if xs else 0.0

    by_kind = {}
    for k in ("message", "state", "object"):
        sub = [i for i in items if i["kind"] == k]
        by_kind[k] = {"total": len(sub), **{c: sum(i[c] for i in sub) for c in ("fields", "handler", "behaviour")},
                      "verdict": sum(bool(i["verdict"]) for i in sub), "done": sum(i["done"] for i in sub)}
    summary = {
        "A_items": {"total": len(items), "done_pct": pct(i["done"] for i in items), "by_kind": by_kind,
                    "verdicts": {v: sum(i["verdict"] == v for i in items) for v in VERDICTS}},
        "B_cross_cutting": {"total": len(cc), "closed": sum(cc), "pct": pct(cc)},
        "B_checklist": {"total": len(cl), "closed": sum(cl), "pct": pct(cl)},
        "C_port_system_traces": {"port_systems": sorted(port_systems), "traced": sum(traced.values()),
                                 "pct": pct(traced.values())},
        "D_replay": {"pct": pct(bool(r.get("replay")) for r in sysrows.values())},
        "D_shadow": {"pct": pct(bool(r.get("shadow")) for r in sysrows.values())},
        "E_adapter": {"calls": len(calls), "with_reference": sum(call_ok), "pct": pct(call_ok),
                      "closed": bool(vf.get("adapter", {}).get("closed"))},
    }
    complete = (not errors and all(i["done"] for i in items) and all(cc) and all(cl) and port_systems
                and all(traced.values()) and all(r.get("replay") and r.get("shadow") for r in sysrows.values())
                and calls and all(call_ok) and summary["E_adapter"]["closed"])
    return {"summary": summary, "complete": bool(complete), "errors": errors, "items": items}


def markdown(res: dict) -> str:
    s = res["summary"]
    a = s["A_items"]
    lines = [BEGIN, "", "| Criterion | Status |", "|---|---|"]
    lines.append(f"| A. Items done (fields, handler, behaviour finding, verdict; port items traced) | "
                 f"{a['done_pct']}% of {a['total']} |")
    for k, v in a["by_kind"].items():
        lines.append(f"| A. {k}s ({v['total']}) | fields {v['fields']}, handler {v['handler']}, behaviour "
                     f"{v['behaviour']}, verdict {v['verdict']}, done {v['done']} |")
    lines.append("| A. Verdicts given | " + ", ".join(f"{k} {n}" for k, n in a["verdicts"].items()) + " |")
    lines.append(f"| B. Cross-cutting gaps closed | {s['B_cross_cutting']['closed']} of "
                 f"{s['B_cross_cutting']['total']} ({s['B_cross_cutting']['pct']}%) |")
    lines.append(f"| B. Easy-to-miss checklist closed | {s['B_checklist']['closed']} of {s['B_checklist']['total']} "
                 f"({s['B_checklist']['pct']}%) |")
    c = s["C_port_system_traces"]
    lines.append(f"| C. Port systems with a live trace | {c['traced']} of {len(c['port_systems'])} ({c['pct']}%): "
                 f"{', '.join(c['port_systems']) or 'none yet'} |")
    lines.append(f"| D. Trace replay in coop/core / shadow mode | {s['D_replay']['pct']}% / {s['D_shadow']['pct']}% |")
    e = s["E_adapter"]
    lines.append(f"| E. Adapter calls with an ACU reference | {e['with_reference']} of {e['calls']} ({e['pct']}%); "
                 f"interface {'closed' if e['closed'] else 'open'} |")
    lines += ["", f"Complete: {'yes' if res['complete'] else 'no'}. Errors: {len(res['errors'])}.", "", END]
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--coop-dir", type=Path, default=ROOT / "local" / "coop")
    ap.add_argument("--verdicts", type=Path, default=ROOT / "docs" / "coop" / "verdicts.json")
    ap.add_argument("--findings", type=Path, default=ROOT / "docs" / "findings")
    ap.add_argument("--update-ledger", type=Path)
    ap.add_argument("--json", type=Path, help="write the full per-item result here")
    ap.add_argument("--no-fail", action="store_true")
    a = ap.parse_args(argv)
    universe = load(a.coop_dir / "coverage_systems.json")
    if universe is None:
        print("unavailable: local/coop/coverage_systems.json is missing (run the ACU-RE-0072 atlas tools)")
        return 2
    res = build(universe, load(a.coop_dir / "message_payloads.json"), load(a.coop_dir / "message_handlers.json"),
                load(a.coop_dir / "memento_codecs.json"), load(a.verdicts) or {}, finding_ids(a.findings))
    if a.json:
        a.json.write_text(json.dumps(res, indent=1), encoding="utf-8")
    md = markdown(res)
    if a.update_ledger:
        text = a.update_ledger.read_text(encoding="utf-8")
        if BEGIN not in text or END not in text:
            ap.error(f"{a.update_ledger} has no {BEGIN} ... {END} block")
        pre, rest = text.split(BEGIN, 1)
        post = rest.split(END, 1)[1]
        a.update_ledger.write_text(pre + md + post, encoding="utf-8")
    print(md)
    for e in res["errors"]:
        print("error:", e)
    return 0 if res["complete"] or a.no_fail else 1


if __name__ == "__main__":
    raise SystemExit(main())
