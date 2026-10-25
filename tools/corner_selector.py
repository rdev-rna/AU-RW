import struct
import subprocess
from pathlib import Path

ROOT = Path(r"C:\Users\Administrator\AU-RW")
GAME = Path(r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Assassin's Creed Unity")
EXE = ROOT / "acu-rs/target/debug/acu-resource.exe"


def read_index(name):
    d = (ROOT / "local" / name).read_bytes()
    n = struct.unpack_from("<I", d, 8)[0]
    out = {}
    for i in range(n):
        at = 12 + i * 12
        out.setdefault(struct.unpack_from("<Q", d, at)[0], []).append(
            struct.unpack_from("<I", d, at + 8)[0])
    return out


selectors = read_index("paris-selectors.bin")
entries = selectors.get(0x114E089B9F, [])
print("selector entries:", entries[:4])
if entries:
    out = ROOT / "local/probe" / "sel-corner.bin"
    subprocess.run([str(EXE), str(GAME / "DataPC_ACU_Paris.forge"), "--entry", str(entries[0]),
                    "--fid", "000000114e089b9f", "--len", "0", "--out", str(out)],
                   capture_output=True)
    if out.exists():
        d = out.read_bytes()
        self_id = struct.unpack_from("<Q", d, 1)[0]
        print("payload", len(d), "self", hex(self_id))
        tag = bytes.fromhex("9a3160a3")
        pos = 0
        while True:
            at = d.find(tag, pos)
            if at < 0:
                break
            st = at - 8
            rid = struct.unpack_from("<Q", d, st)[0]
            flags = d[at + 4:at + 6]
            ref = struct.unpack_from("<Q", d, at + 6)[0]
            print(f"  rec @{st} id {rid:016x} flags {flags.hex()} ref {ref:016x}")
            pos = at + 1
