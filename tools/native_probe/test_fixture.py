"""Meaningful native fixture regression; builds must already exist. Windows only."""
import json
import hashlib
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import time
import unittest
from client import checked, command, dump, scan, session

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "local/native-probe"


def exports(path):
    b = path.read_bytes()
    pe = struct.unpack_from("<I", b, 0x3c)[0]
    sections, optional = struct.unpack_from("<H", b, pe+6)[0], pe+24
    size_opt = struct.unpack_from("<H", b, pe+20)[0]
    ranges = [struct.unpack_from("<IIII", b, optional+size_opt+i*40+8) for i in range(sections)]
    def offset(rva):
        for virtual_size, va, raw_size, raw in ranges:
            if va <= rva < va+min(virtual_size, raw_size):
                return raw+rva-va
        raise ValueError("RVA not backed by file bytes")
    directory = offset(struct.unpack_from("<I", b, optional+112)[0])
    names = struct.unpack_from("<I", b, directory+24)[0]
    funcs, name_table, ordinals = [offset(v) for v in struct.unpack_from("<III", b, directory+28)]
    result = {}
    for i in range(names):
        name_off = offset(struct.unpack_from("<I", b, name_table+4*i)[0])
        name = b[name_off:b.index(0, name_off)].decode()
        ordinal = struct.unpack_from("<H", b, ordinals+2*i)[0]
        result[name] = struct.unpack_from("<I", b, funcs+4*ordinal)[0]
    return result


class FixtureTest(unittest.TestCase):
    def test_native_lifecycle(self):
        with tempfile.TemporaryFile(mode="w+", encoding="utf-8") as log:
            environment = dict(os.environ, ACU_PROBE_STARTUP_DUMP="1", ACU_PROBE_STARTUP_DUMP_DELAY_MS="0")
            launched = subprocess.run([str(OUT/"acu_probe_launcher.exe"), "--launch", str(OUT/"acu_probe_fixture.exe"), "--dll", str(OUT/"acu_probe.dll")], stdout=log, stderr=subprocess.STDOUT, text=True, timeout=20, env=environment)
            log.seek(0)
            output = log.read()
        self.assertEqual(launched.returncode, 0, output)
        record = next(json.loads(s) for s in output.splitlines() if '"injected"' in s)
        self.assertTrue(record["process_running"])
        self.assertTrue(record["probe_status_ready"])
        self.assertIsNone(record["process_exit_code"])
        pid = record["pid"]
        try:
            time.sleep(.1)
            status = checked(pid, "STATUS")
            startup_manifests = list(OUT.glob(f"startup-{pid}-*.json"))
            self.assertEqual(len(startup_manifests), 1)
            startup = json.loads(startup_manifests[0].read_text())
            self.assertTrue(startup["completed_io"])
            self.assertFalse(startup["read_failures"])
            mapped = Path(startup["mapped_path"]).read_bytes()
            self.assertEqual(mapped[:2], b"MZ")
            self.assertEqual(hashlib.sha256(mapped).hexdigest(), startup["mapped_sha256"])
            base = int(status["image_base"], 16)
            sites = exports(OUT/"acu_probe_fixture.exe")
            tick = sites["ProbeFixtureTick"]
            counter = base+sites["ProbeFixtureCounter"]
            self.assertFalse(command(pid, "READ 1 16")["ok"])
            self.assertFalse(command(pid, "READ 0 65537")["ok"])
            self.assertFalse(command(pid, f"HOOK 0 {tick:x} 00 wrong")["ok"])
            expected = checked(pid, f"READ {base+tick:x} 8")["hex"]
            pattern = " ".join(expected[i:i+2] for i in range(0,len(expected),2))
            self.assertIn(f"{base+tick:016x}", scan(pid, base+tick, 128, pattern)["matches"])
            checked(pid, f"WATCH counter {counter:x} 8")
            initial = checked(pid, "WATCHES")["watches"][0]
            checked(pid, f"HOOK 0 {tick:x} {expected} fixture_tick 1 0 0 0")
            time.sleep(.15)
            event = checked(pid, "EVENTS 32")["events"][-1]
            self.assertEqual(int(event["registers"][0], 16), 1)  # actual ABI argument
            self.assertEqual(int(event["rip"], 16), base+tick)
            self.assertEqual(len(event["xmm0_5_hex"]), 192)
            self.assertGreater(len(event["stack_hex"]), 0)
            checked(pid, f"HOOKVA 0 {base+tick:x} {expected} old_label 1 0 0 0 animation")
            time.sleep(.07)
            checked(pid, f"HOOKVA 0 {base+tick:x} {expected} new_label 1 0 0 0 traversal")
            time.sleep(.07)
            rotated = checked(pid, "EVENTS 32")["events"]
            self.assertEqual({e["label"] for e in rotated}, {"old_label", "new_label"})
            self.assertEqual({e["category"] for e in rotated}, {"animation", "traversal"})
            self.assertEqual(len({e["revision"] for e in rotated}), 2)
            changed = checked(pid, "WATCHES")["watches"][0]
            self.assertGreater(changed["changes"], initial["changes"])
            self.assertGreater(int.from_bytes(bytes.fromhex(changed["hex"]), "little"), int.from_bytes(bytes.fromhex(initial["hex"]), "little"))
            # Pointer-array gather: RCX is the array, RDX points at a pointer to it.
            select = sites["ProbeFixtureSelect"]
            select_bytes = checked(pid, f"READ {base+select:x} 8")["hex"]
            self.assertFalse(command(pid, "GATHER 1 0 0 0 0 10 4 16")["ok"])  # no hook in slot 1
            checked(pid, f"HOOK 1 {select:x} {select_bytes} fixture_select")
            self.assertFalse(command(pid, "GATHER 1 0 0 0 0 10 17 16")["ok"])  # too many elements
            self.assertFalse(command(pid, "GATHER 1 0 0 0 0 10 4 769")["ok"])  # element too large
            self.assertFalse(command(pid, "GATHER 1 16 0 0 0 10 4 16")["ok"])  # bad register
            self.assertFalse(command(pid, "GATHER 1 0 0 2 0 10 4 16")["ok"])  # deref is 0 or 1
            expected_elements = [bytes(0x10*(i+1)+k for k in range(16)).hex() for i in range(3)]
            for reg, deref in ((0, 0), (1, 1)):
                checked(pid, f"HOOK 1 {select:x} {select_bytes} fixture_select")
                checked(pid, f"GATHER 1 {reg} 0 {deref} 0 10 4 16")
                time.sleep(.1)
                gathered = [e for e in checked(pid, "EVENTS 64")["events"] if e["label"] == "fixture_select"]
                self.assertTrue(gathered)
                g = gathered[-1]["gather"]
                self.assertTrue(g["ok"])
                self.assertEqual(g["count"], 3)
                self.assertEqual(int(g["base"], 16), int(gathered[-1]["registers"][0], 16))
                self.assertEqual([e["hex"] for e in g["elements"]], expected_elements)
            checked(pid, f"GATHER 1 0 0 0 0 10 2 16")  # bounded below the array length
            time.sleep(.1)
            g = [e for e in checked(pid, "EVENTS 64")["events"] if e["label"] == "fixture_select"][-1]["gather"]
            self.assertEqual((g["count"], len(g["elements"])), (3, 2))
            checked(pid, f"HOOK 1 {select:x} {select_bytes} fixture_select")  # HOOK clears gather
            time.sleep(.1)
            self.assertNotIn("gather", [e for e in checked(pid, "EVENTS 64")["events"] if e["label"] == "fixture_select"][-1])
            checked(pid, "UNHOOK 1")
            for _ in range(100):
                checked(pid, "UNHOOK 0")
                checked(pid, f"HOOK 0 {tick:x} {expected} fixture_tick")
                time.sleep(.005)
            checked(pid, "UNHOOK 0")
            hits = checked(pid, "STATUS")["hits"]
            time.sleep(.1)
            self.assertEqual(checked(pid, "STATUS")["hits"], hits)
            # Data-write watch: the fixture writes ProbeFixtureCounter in ProbeFixtureTick every 20 ms.
            self.assertFalse(command(pid, f"HOOKDATA 2 {counter+1:x} 8 misaligned")["ok"])
            self.assertFalse(command(pid, f"HOOKDATA 2 {counter:x} 3 badlen")["ok"])
            self.assertFalse(command(pid, f"HOOKDATA 2 {base+tick:x} 1 code")["ok"])
            checked(pid, "EVENTS 256")
            checked(pid, f"HOOKDATA 2 {counter:x} 8 counter_write 1 0 8")
            time.sleep(.2)
            writes = [e for e in checked(pid, "EVENTS 256")["events"] if e["label"] == "counter_write"]
            self.assertGreaterEqual(len(writes), 3)
            for e in writes:
                self.assertEqual(int(e["probe_address"], 16), counter)
                self.assertTrue(0 < int(e["rip"], 16) - (base+tick) < 0x100, e["rip"])  # trap is after the write
                self.assertEqual(e["category"], "data_watch")
                self.assertGreater(len(e["stack_hex"]), 128)  # data watches keep 512 stack bytes
            values = [struct.unpack("<Q", bytes.fromhex(e["snapshot_hex"]))[0] for e in writes]
            self.assertEqual(values, sorted(values))
            checked(pid, "UNHOOK 2")
            hits = checked(pid, "STATUS")["hits"]
            time.sleep(.1)
            self.assertEqual(checked(pid, "STATUS")["hits"], hits)
            with tempfile.TemporaryDirectory(dir=OUT) as tmp:
                manifest = dump(pid, Path(tmp))
                self.assertFalse(manifest["read_failures"])
                self.assertEqual((Path(tmp)/"main-image.mapped.bin").read_bytes()[:2], b"MZ")
                profile = Path(tmp)/"profile.json"
                profile.write_text(json.dumps({"probes": [{"module": "acu_probe_fixture.exe", "rva": f"{tick:x}", "expected_hex": expected, "label": "module_tick", "category": "fixture"}]}))
                trace = Path(tmp)/"trace.jsonl"
                session(pid, profile, trace, .2, 1024**2)
                records = [json.loads(line) for line in trace.read_text().splitlines()]
                self.assertTrue(records[0]["modules"][0]["disk_sha256"])
                captured = [e for record in records if record["kind"] == "events" for e in record["events"]]
                self.assertTrue(any(e["label"] == "module_tick" for e in captured))
            checked(pid, "SHUTDOWN")
            time.sleep(.1)
            # Probe shutdown must leave original application running.
            running = subprocess.run(["powershell", "-NoProfile", "-Command", f"Get-Process -Id {pid} -ErrorAction Stop | Out-Null"], capture_output=True)
            self.assertEqual(running.returncode, 0)
        finally:
            subprocess.run(["taskkill", "/PID", str(pid), "/F"], capture_output=True)
            for generated in OUT.glob(f"startup-{pid}-*"):
                generated.unlink()


if __name__ == "__main__":
    unittest.main()

