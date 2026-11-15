"""Put Arno in an outfit at runtime through the focus shim (modes 6/7 of the teleport section).

ACU-RE-0048 (equip routine, AvatarGearManager), ACU-RE-0070 (Arno's Tailored is the `_02` set),
ACU-RE-0081 (cloth comparison). Lead for the visual-selection step: the ACUFixes plugin
"Gear Visual Customization 365" 1.2.5, read statically; none of its code is reused.

    python tools/native_probe/outfit.py <pid> tailored            # visual only (mode 6)
    python tools/native_probe/outfit.py <pid> tailored --persist  # equip + refresh (mode 7)

Mode 6 copies the active loadout, puts the items in it and runs the game's visual selection
(0x140D25540) on the copy: nothing is written to the loadout, and the next refresh (closing the
gear menu) restores the real outfit. Mode 7 calls the game's equip routine (0x140D2B240) per item
and then its refresh (0x140D527F0); the loadout changes and a later save stores it, so pin and
put back the saves around it (capture_run --save-pin does).
"""
import argparse
import json
from pathlib import Path
import struct
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import teleport  # noqa: E402

# ACU-RE-0070: Arno's Tailored, items `ACU_AvatarGear_<Slot>_02` (InventoryItemSettings).
OUTFITS = {
    "tailored": {
        "head": 0x00000011A8F2593F,
        "chest": 0x00000011A8F25AAE,
        "forearms": 0x00000011A8F25BF1,
        "waist": 0x00000011A8F25B51,
        "legs": 0x00000011A8F25C94,
    },
}
MODE_VISUAL, MODE_PERSIST = 6, 7
STATUS = {-1: "no PPM or active loadout", -2: "no AvatarGearManager", -3: "exception",
          -5: "world travel in progress", -7: "not every item found among the gears",
          -8: "an item is not armour (type > 4)"}


def pack_ids(ids: list[int]) -> list[float]:
    """Eight u64 slots carried in the section's 16-float matrix (unused slots zero)."""
    raw = struct.pack("<8Q", *(ids + [0] * (8 - len(ids))))
    return list(struct.unpack("<16f", raw))


def apply(pid: int, outfit: str, persist: bool = False, timeout: float = 5.0) -> dict:
    items = OUTFITS[outfit]
    ids = list(items.values())
    shared = teleport.Shared(pid)
    try:
        shared.call(len(ids), 0, pack_ids(ids), timeout=timeout,
                    mode=MODE_PERSIST if persist else MODE_VISUAL)
        row = shared.read()
    finally:
        shared.close()
    status, found, exception = row[3], row[5], row[22]
    gears = {name: {"found": bool(found >> i & 1), "type": (found >> (8 + 4 * i)) & 0xF}
             for i, name in enumerate(items)}
    return {"outfit": outfit, "mode": "persist" if persist else "visual", "status": status,
            "meaning": "done" if status == len(ids) else STATUS.get(status, "?"),
            "gears": gears, "exception": f"0x{exception:08X}" if exception else None}


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("pid", type=int)
    ap.add_argument("outfit", choices=sorted(OUTFITS))
    ap.add_argument("--persist", action="store_true", help="equip through the game (mode 7)")
    a = ap.parse_args()
    print(json.dumps(apply(a.pid, a.outfit, a.persist), indent=1))


if __name__ == "__main__":
    main()
