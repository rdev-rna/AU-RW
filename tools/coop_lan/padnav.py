"""padnav.py <pid> <out.png> [STEP ...]
Presses pad input in the game's in-process pad (focus shim), then saves a 960x540 screenshot of the game window.
Steps: BUTTON[+BUTTON][:hold_s]  wait:s  LSx,y[+BUTTON]:s (left stick, -1..1)  HPx,y[+BUTTON]:s (same with the right
trigger held, high profile)  RSx,y:s (camera). Needs native_probe from branch acu-multi-launch-direct (focus,
capture_run, controller_autoplay). Used for ACU-RE-0105 to change gear in the menus and fight."""
import sys, time
sys.path.insert(0, str(__import__("pathlib").Path(__file__).resolve().parents[1] / "native_probe"))
import focus, capture_run as cr
from controller_autoplay import capture_window

pid, out, seq = int(sys.argv[1]), sys.argv[2], sys.argv[3:]
with focus.InProcessPad(pid) as pad:
    def hold(a, s):
        e = time.monotonic() + s
        while time.monotonic() < e:
            pad.update(a); time.sleep(1 / 60)
        pad.update({})
    hold({}, 0.5)
    for step in seq:
        name, _, arg = step.partition(":")
        if name == "wait":
            hold({}, float(arg or 1)); continue
        if name.startswith(("LS", "HP")):  # LSx,y:seconds (-1..1); HP = same with right trigger held (high profile)
            stick, *btn = name[2:].split("+")
            x, y = (float(v) for v in stick.split(","))
            act = {"left_stick": [int(x * 32767), int(y * 32767)]}
            if btn: act["buttons"] = btn
            if name.startswith("HP"): act["right_trigger"] = 255
            hold(act, float(arg or 0.3)); hold({}, 0.4); continue
        if name.startswith("RS"):  # camera
            x, y = (float(v) for v in name[2:].split(","))
            hold({"right_stick": [int(x * 32767), int(y * 32767)]}, float(arg or 0.3)); continue
        hold({"buttons": name.split("+")}, float(arg or 0.15)); hold({}, 0.6)
h = next(h for q, h in cr.engine_windows() if q == pid)
capture_window(h).resize((960, 540)).save(out)
print("ok", out)
