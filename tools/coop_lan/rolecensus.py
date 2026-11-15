"""rolecensus.py <out.json> <pid>...: run world_entities.py (radius 120) in each game, then READ entity+212 for every
entity and keep those whose role bits are replica (0x80000) or whose kind bits are non-zero. Output: per game, list of
{entity, file_id, stem, pos, raw}. Reads only. Run two censuses around a position change and diff by file_id to
see ownership migrate (ACU-RE-0092 observations 18-19). Needs tools/native_probe (branch acu-multi-launch-direct)."""
import json, os, pathlib, struct, subprocess, sys
NP = pathlib.Path(__file__).resolve().parents[1] / "native_probe"
sys.path.insert(0, str(NP))
from client import checked
out_path = sys.argv[1]; pids = [int(p) for p in sys.argv[2:]]
S = os.path.dirname(os.path.abspath(out_path)); res = {}
for p in pids:
    tmp = os.path.join(S, f"census_ents_{p}.json")
    subprocess.run([sys.executable, str(NP / "world_entities.py"),
                    str(p), tmp, "--radius", "120"], check=True, capture_output=True, timeout=180)
    d = json.load(open(tmp)); rows = []
    for e in d["entities"]:
        a = int(e["entity"], 16)
        try: v = struct.unpack("<I", bytes.fromhex(checked(p, f"READ {a+212:x} 4")["hex"]))[0]
        except Exception: continue
        if (v & 0xC0000) == 0x80000 or (v & 7) in (1, 3) or (v & 0xFF) not in (0, 2):
            rows.append({"entity": e["entity"], "file_id": e.get("file_id"), "stem": e.get("stem"), "animated": e.get("animated"),
                         "pos": [round(x, 2) for x in e["position"]], "raw": f"{v:#x}"})
    res[str(p)] = {"player": d["player_position"], "rows": rows}
    print(p, "player", [round(x, 1) for x in d["player_position"]], "kept", len(rows),
          "replicas", sum(1 for r in rows if (int(r["raw"], 16) & 0xC0000) == 0x80000), flush=True)
json.dump(res, open(out_path, "w"), indent=1)
