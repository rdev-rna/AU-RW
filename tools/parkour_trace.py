"""Decode a native parkour candidate-selection trace (ACU-RE-0041).

Input is a `client.py session` JSONL written with
`tools/native_probe/parkour_selection.profile.json`. Each entry event of
0x140133B00 carries the raw candidate objects (pointer array a9); each exit
event at the shared epilogue 0x140133D4B carries EAX (selected index into the
sorted array, or -1) and the same array after filtering and sorting.

Only offsets re-derived against the installed build are decoded as fields:
  +0x08  fancy-table pointer (selection calls through [object+8])
  +0xC8  f32 read by the distance getter 0x1401A2100
  +0x204 f32 read by the fitness getter 0x1401A7620
  enum   imm32 returned by fancy entry 0x45 of the object's table (needs --image)
ACUFixes' names for +0xC0/+0xC4 are reported under `unverified_acufixes`.
Raw candidate bytes stay in the session file; this tool never invents fields.

    python tools/parkour_trace.py <native-session.jsonl> <out-dir> [--image main-image.mapped.bin]
"""
from __future__ import annotations

import argparse
import collections
import hashlib
import json
import struct
from pathlib import Path

IMAGE_BASE = 0x140000000
FANCY_ENTRY = 0x20
ENUM_SLOT = 0x45
ENTRY_LABEL = "parkour_select_entry"
EXIT_LABEL = "parkour_select_exit"


def f32(raw: bytes, offset: int) -> float | None:
    return struct.unpack_from("<f", raw, offset)[0] if len(raw) >= offset + 4 else None


def u64(raw: bytes, offset: int) -> int | None:
    return struct.unpack_from("<Q", raw, offset)[0] if len(raw) >= offset + 8 else None


class EnumResolver:
    """Resolve a fancy table to its action enum via `mov eax, imm32; ret` at slot 0x45."""

    def __init__(self, image: bytes | None, live_base: int):
        self.image, self.live_base, self.cache = image, live_base, {}

    def _read(self, va: int, n: int) -> bytes | None:
        off = va - self.live_base
        if self.image is None or off < 0 or off + n > len(self.image):
            return None
        return self.image[off:off + n]

    def __call__(self, table: int | None) -> int | None:
        if table is None or self.image is None:
            return None
        if table not in self.cache:
            value = None
            entry = self._read(table + ENUM_SLOT * FANCY_ENTRY, FANCY_ENTRY)
            if entry is not None and struct.unpack_from("<I", entry, 4)[0] == ENUM_SLOT:
                code = self._read(struct.unpack_from("<Q", entry, 0x10)[0], 6)
                if code is not None and code[0] == 0xB8 and code[5] == 0xC3:
                    value = struct.unpack_from("<i", code, 1)[0]
            self.cache[table] = value
        return self.cache[table]


def candidate(element: dict, resolve: EnumResolver) -> dict:
    raw = bytes.fromhex(element["hex"])
    table = u64(raw, 0x08)
    return {
        "ptr": element["ptr"],
        "bytes": len(raw),
        "fancy_table": None if table is None else f"{table:016x}",
        "enum": resolve(table),
        "fitness": f32(raw, 0x204),
        "distance": f32(raw, 0xC8),
        "unverified_acufixes": {"height_difference": f32(raw, 0xC0),
                                "horizontal_difference": f32(raw, 0xC4)},
    }


def decode(records: list[dict], image: bytes | None) -> tuple[list[dict], dict]:
    header = next((r for r in records if r.get("kind") == "header"), None)
    live_base = int(header["status"]["image_base"], 16) if header else IMAGE_BASE
    resolve = EnumResolver(image, live_base)
    events = [e for r in records if r.get("kind") == "events" for e in r.get("events", [])]
    events.sort(key=lambda e: e["sequence"])
    pending: dict[tuple[int, str], dict] = {}
    decisions, problems = [], collections.Counter()
    for e in events:
        g = e.get("gather")
        if e["label"] not in (ENTRY_LABEL, EXIT_LABEL):
            continue
        if not g or not g.get("ok"):
            problems["gather_unreadable"] += 1
            continue
        key = (e["tid"], g["base"])
        cands = [candidate(x, resolve) for x in g["elements"]]
        if e["label"] == ENTRY_LABEL:
            if key in pending:
                problems["entry_without_exit"] += 1
            pending[key] = {"entry_sequence": e["sequence"], "qpc": e["qpc"], "tid": e["tid"],
                            "array": g["base"], "tester": e["registers"][0],
                            "considered_count": g["count"], "considered": cands}
            continue
        index = struct.unpack("<i", struct.pack("<I", int(e["registers"][4], 16) & 0xFFFFFFFF))[0]
        decision = pending.pop(key, None)
        if decision is None:
            problems["exit_without_entry"] += 1
            decision = {"tid": e["tid"], "array": g["base"], "qpc": e["qpc"]}
        selected = None
        if index >= 0:
            if index < len(cands):
                selected = cands[index]
            else:
                problems["selected_outside_gather"] += 1
        decision.update({"exit_sequence": e["sequence"], "selected_index": index,
                         "ranked_count": g["count"], "ranked": cands, "selected": selected})
        decisions.append(decision)
    if pending:
        problems["entry_without_exit"] += len(pending)
    drops = [r.get("dropped_or_overwritten") for r in records if r.get("kind") == "status"]
    summary = {
        "decisions": len(decisions),
        "with_selection": sum(d["selected_index"] >= 0 for d in decisions),
        "no_selection": sum(d["selected_index"] < 0 for d in decisions),
        "paired": sum("entry_sequence" in d for d in decisions),
        "problems": dict(problems),
        "probe_drops_or_overwrites_last_status": drops[-1] if drops else None,
        "considered_enum_counts": dict(collections.Counter(
            c["enum"] for d in decisions for c in d.get("considered", []))),
        "selected_enum_counts": dict(collections.Counter(
            d["selected"]["enum"] for d in decisions if d["selected"])),
        "enum_resolution": "image" if image is not None else "unavailable (no --image)",
    }
    return decisions, summary


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("session", type=Path)
    ap.add_argument("output", type=Path)
    ap.add_argument("--image", type=Path, help="hash-bound mapped main image of the installed build")
    a = ap.parse_args()
    records = [json.loads(line) for line in a.session.read_text(encoding="utf-8").splitlines() if line.strip()]
    image = a.image.read_bytes() if a.image else None
    decisions, summary = decode(records, image)
    summary["session_sha256"] = hashlib.sha256(a.session.read_bytes()).hexdigest()
    if image is not None:
        summary["image_sha256"] = hashlib.sha256(image).hexdigest()
    a.output.mkdir(parents=True, exist_ok=False)
    with (a.output / "decisions.jsonl").open("x", encoding="utf-8") as f:
        for d in decisions:
            f.write(json.dumps(d) + "\n")
    (a.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
