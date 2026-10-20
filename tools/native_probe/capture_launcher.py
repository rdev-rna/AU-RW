#!/usr/bin/env python3
"""Category-based ACU controller and native-capture launcher (Windows)."""
from __future__ import annotations
import ctypes as C
from ctypes import wintypes as W
from datetime import datetime
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import threading
import tkinter as tk
from tkinter import ttk, messagebox

ROOT = Path(__file__).resolve().parents[2]
PROBE = ROOT/"local/native-probe"
LAUNCHER = PROBE/"acu_probe_launcher.exe"
DLL = PROBE/"acu_probe.dll"
CATEGORIES = [
    ("animation", "Animation and blend state"), ("skeleton", "Skeleton and bone transforms"),
    ("locomotion", "Locomotion and root motion"), ("traversal", "Traversal and parkour"),
    ("climbing", "Climbing and ledges"), ("collision", "Collision contacts"),
    ("raycast", "Raycasts and queries"), ("physics", "Physics"),
    ("input", "Controller input API"), ("camera", "Camera"), ("ai", "AI"),
    ("world", "World state"), ("streaming", "World streaming"),
    ("rendering", "Rendering and animation upload"),
]
ACTIVE = {"input": "XInputGetState is resolved from the live process; byte-guarded entry hooks are available."}

K = C.WinDLL("kernel32", use_last_error=True)
K.EnumProcesses = C.WinDLL("psapi").EnumProcesses
K.EnumProcesses.argtypes = [C.POINTER(W.DWORD), W.DWORD, C.POINTER(W.DWORD)]
K.EnumProcesses.restype = W.BOOL
K.OpenProcess.argtypes = [W.DWORD, W.BOOL, W.DWORD]
K.OpenProcess.restype = W.HANDLE
K.QueryFullProcessImageNameW.argtypes = [W.HANDLE, W.DWORD, W.LPWSTR, C.POINTER(W.DWORD)]
K.QueryFullProcessImageNameW.restype = W.BOOL
K.CloseHandle.argtypes = [W.HANDLE]
USER = C.WinDLL("user32", use_last_error=True)
USER.EnumWindows.argtypes = [C.c_void_p, C.c_void_p]
USER.EnumWindows.restype = W.BOOL
USER.IsWindowVisible.argtypes = [W.HWND]
USER.IsWindowVisible.restype = W.BOOL
USER.GetWindowThreadProcessId.argtypes = [W.HWND, C.POINTER(W.DWORD)]
USER.GetWindowThreadProcessId.restype = W.DWORD
USER.ShowWindow.argtypes = [W.HWND, C.c_int]
USER.SetForegroundWindow.argtypes = [W.HWND]
USER.GetForegroundWindow.restype = W.HWND


def acu_processes():
    ids = (W.DWORD*4096)()
    needed = W.DWORD()
    if not K.EnumProcesses(ids, C.sizeof(ids), C.byref(needed)):
        raise C.WinError(C.get_last_error())
    result = []
    for pid in ids[:needed.value//C.sizeof(W.DWORD)]:
        handle = K.OpenProcess(0x1000, False, pid)  # PROCESS_QUERY_LIMITED_INFORMATION
        if not handle:
            continue
        try:
            buf = C.create_unicode_buffer(32768)
            size = W.DWORD(len(buf))
            if K.QueryFullProcessImageNameW(handle, 0, buf, C.byref(size)):
                path = Path(buf.value)
                if path.name.casefold() == "acu.exe":
                    result.append((pid, str(path)))
        finally:
            K.CloseHandle(handle)
    return result


def focus_pid(pid):
    candidates = []
    callback_type = C.WINFUNCTYPE(W.BOOL, W.HWND, C.c_void_p)
    def callback(hwnd, _):
        target = W.DWORD()
        USER.GetWindowThreadProcessId(hwnd, C.byref(target))
        if target.value == pid and USER.IsWindowVisible(hwnd):
            candidates.append(hwnd)
        return True
    callback_fn = callback_type(callback)
    USER.EnumWindows(callback_fn, None)
    if not candidates:
        raise RuntimeError("selected ACU process has no visible window")
    USER.ShowWindow(candidates[0], 9)  # SW_RESTORE
    USER.SetForegroundWindow(candidates[0])
    foreground = USER.GetForegroundWindow()
    foreground_pid = W.DWORD()
    if foreground:
        USER.GetWindowThreadProcessId(foreground, C.byref(foreground_pid))
    if foreground_pid.value != pid:
        raise RuntimeError("Windows did not grant foreground focus to the ACU window; activate it and retry")


class CaptureLauncher(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title("ACU Native Capture Launcher")
        self.geometry("900x760")
        self.resizable(True, True)
        self.processes = []
        self.category_vars = {}
        self._build()
        self.refresh()

    def _build(self):
        root = ttk.Frame(self, padding=12)
        root.pack(fill="both", expand=True)
        ttk.Label(root, text="Target ACU process", font=("Segoe UI", 12, "bold")).pack(anchor="w")
        row = ttk.Frame(root); row.pack(fill="x", pady=(4, 12))
        self.pid_var = tk.StringVar()
        self.process_box = ttk.Combobox(row, textvariable=self.pid_var, state="readonly", width=74)
        self.process_box.pack(side="left", fill="x", expand=True)
        ttk.Button(row, text="Refresh", command=self.refresh).pack(side="left", padx=(8, 0))

        category_heading = ttk.Frame(root); category_heading.pack(fill="x")
        ttk.Label(category_heading, text="Capture categories", font=("Segoe UI", 12, "bold")).pack(side="left")
        ttk.Button(category_heading, text="Select all", command=lambda: self.set_categories(True)).pack(side="right")
        ttk.Button(category_heading, text="Clear", command=lambda: self.set_categories(False)).pack(side="right", padx=6)
        ttk.Label(root, text="Only categories with verified probe sites produce native events. Memory snapshots are process-wide.",
                  foreground="#555").pack(anchor="w", pady=(2, 8))
        grid = ttk.Frame(root); grid.pack(fill="x", pady=(0, 10))
        for index, (key, label) in enumerate(CATEGORIES):
            cell = ttk.Frame(grid, padding=(3, 2))
            cell.grid(row=index//2, column=index%2, sticky="ew")
            var = tk.BooleanVar(value=(key == "input"))
            self.category_vars[key] = var
            ttk.Checkbutton(cell, text=label, variable=var).pack(anchor="w")
            ttk.Label(cell, text=ACTIVE.get(key, "No verified native probe mapped yet"),
                      foreground="#167247" if key in ACTIVE else "#777", wraplength=400).pack(anchor="w", padx=(24, 0))
        grid.columnconfigure(0, weight=1); grid.columnconfigure(1, weight=1)

        options = ttk.LabelFrame(root, text="Controller replay and memory", padding=8)
        options.pack(fill="x", pady=6)
        self.replay_var = tk.StringVar(value="menu")
        ttk.Radiobutton(options, text="Enter world (two A presses)", variable=self.replay_var,
                        value="menu").grid(row=0, column=0, sticky="w")
        ttk.Radiobutton(options, text="Sweep all ordinary X360 controls", variable=self.replay_var,
                        value="sweep").grid(row=0, column=1, sticky="w")
        ttk.Label(options, text="Memory checkpoints:").grid(row=1, column=0, sticky="w", pady=(8, 0))
        self.snapshot_var = tk.StringVar(value="none")
        ttk.Combobox(options, textvariable=self.snapshot_var, values=("none", "main", "all"),
                     state="readonly", width=12).grid(row=1, column=1, sticky="w", pady=(8, 0))
        ttk.Label(options, text="Readable-memory budget (GiB):").grid(row=1, column=2, sticky="e", pady=(8, 0))
        self.budget_var = tk.StringVar(value="16")
        ttk.Entry(options, textvariable=self.budget_var, width=7).grid(row=1, column=3, sticky="w", pady=(8, 0))
        options.columnconfigure(1, weight=1)

        xref = ttk.LabelFrame(root, text="Optional live-image/decompile cross-reference", padding=8)
        xref.pack(fill="x", pady=6)
        self.functions_var = tk.StringVar()
        self.summary_var = tk.StringVar()
        self.corpus_var = tk.StringVar(value=str(ROOT/"ACU - Decompiled"))
        for i, (label, var) in enumerate((("functions.jsonl", self.functions_var),
                                          ("image summary.json", self.summary_var),
                                          ("decompile corpus", self.corpus_var))):
            ttk.Label(xref, text=label).grid(row=i, column=0, sticky="w", pady=2)
            ttk.Entry(xref, textvariable=var).grid(row=i, column=1, sticky="ew", padx=6, pady=2)
        xref.columnconfigure(1, weight=1)

        controls = ttk.Frame(root); controls.pack(fill="x", pady=(10, 4))
        self.start_button = ttk.Button(controls, text="Attach and Run", command=self.start)
        self.start_button.pack(side="left")
        self.status_var = tk.StringVar(value="Ready")
        ttk.Label(controls, textvariable=self.status_var).pack(side="left", padx=12)
        self.log = tk.Text(root, height=10, wrap="word", state="disabled")
        self.log.pack(fill="both", expand=True, pady=(8, 0))

    def refresh(self):
        try:
            self.processes = acu_processes()
            values = [f"{pid} — {path}" for pid, path in self.processes]
            self.process_box["values"] = values
            if values:
                current = self.pid_var.get()
                self.pid_var.set(current if current in values else values[0])
                self.status_var.set(f"Found {len(values)} ACU process(es)")
            else:
                self.pid_var.set(""); self.status_var.set("No ACU process found — launch through Ubisoft Connect")
        except Exception as error:
            self.status_var.set(str(error))

    def log_line(self, value):
        self.log.configure(state="normal"); self.log.insert("end", value+"\n")
        self.log.see("end"); self.log.configure(state="disabled")

    def selected(self):
        value = self.pid_var.get()
        return next((entry for entry in self.processes if value.startswith(str(entry[0])+" — ")), None)

    def start(self):
        target = self.selected()
        if not target:
            messagebox.showerror("ACU not found", "Launch ACU through Ubisoft Connect, then click Refresh.")
            return
        try:
            budget = int(float(self.budget_var.get())*1024**3)
            if not 1 <= budget <= 64*1024**3:
                raise ValueError("budget must be 1..64 GiB")
        except ValueError as error:
            messagebox.showerror("Invalid memory budget", str(error)); return
        selected = sorted(key for key, var in self.category_vars.items() if var.get())
        pid, path = target
        replay = self.replay_var.get()
        snapshot = self.snapshot_var.get()
        xref_values = (self.functions_var.get(), self.summary_var.get(), self.corpus_var.get())
        self.start_button.configure(state="disabled")
        self.status_var.set("Attaching probe and preparing capture…")
        threading.Thread(target=self._start_worker,
                         args=(pid, path, selected, budget, replay, snapshot, xref_values), daemon=True).start()

    def _start_worker(self, pid, game_path, categories, budget, replay, snapshot, xref_values):
        try:
            if not LAUNCHER.is_file() or not DLL.is_file():
                raise RuntimeError("Build tools/native_probe/build.ps1 first; probe DLL/launcher are missing")
            probe_client = ROOT/"tools/native_probe/client.py"
            # Existing attached sessions respond to STATUS; otherwise inject only
            # the project's hash-guarded observer DLL into this verified ACU PID.
            check = subprocess.run([sys.executable, str(probe_client), str(pid), "command", "STATUS"],
                                   capture_output=True, text=True)
            if check.returncode or '"ok": true' not in check.stdout:
                attached = subprocess.run([str(LAUNCHER), "--attach", str(pid), "--dll", str(DLL)],
                                          capture_output=True, text=True, timeout=15)
                if attached.returncode:
                    raise RuntimeError(attached.stdout+attached.stderr)
            status = subprocess.run([sys.executable, str(probe_client), str(pid), "command", "MODULES"],
                                    capture_output=True, text=True, check=True)
            module_info = json.loads(status.stdout)
            main = next(m for m in module_info["modules"] if m["name"].casefold() == "acu.exe")
            digest = hashlib.sha256(Path(main["path"]).read_bytes()).hexdigest()
            stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
            output = PROBE/f"capture-{stamp}"
            profile_path = PROBE/f"capture-profile-{stamp}-{pid}.json"
            plan_path = PROBE/f"capture-plan-{stamp}-{pid}.json"
            replay_name = "menu_then_world.example.json" if replay == "menu" else "controller_sweep.example.json"
            source_profile = json.loads((ROOT/"tools/native_probe"/replay_name).read_text(encoding="utf-8"))
            # Keep the selected capture to one final checkpoint; an all-region
            # snapshot is intentionally not repeated at every route waypoint.
            for step in source_profile["steps"]:
                step.pop("checkpoint", None)
            source_profile["steps"][-1]["checkpoint"] = True
            profile_path.write_text(json.dumps(source_profile, indent=2)+"\n", encoding="utf-8")
            plan = {"pid":pid, "game_path":game_path, "game_sha256":digest,
                    "categories":categories, "available_native_categories":sorted(ACTIVE),
                    "replay_profile":replay_name, "snapshot_mode":snapshot,
                    "memory_budget_bytes":budget, "input_reactions_beyond_XInputGetState":"not decoded"}
            plan_path.write_text(json.dumps(plan, indent=2)+"\n", encoding="utf-8")
            probe_profile = None
            if "input" in categories:
                probe_profile = PROBE/f"verified-input-probes-{stamp}-{pid}.json"
                generated = subprocess.run([sys.executable, str(ROOT/"tools/native_probe/xinput_profile.py"),
                                            str(pid), str(probe_profile)], capture_output=True, text=True)
                if generated.returncode:
                    raise RuntimeError(generated.stdout+generated.stderr)
            focus_pid(pid)
            cmd = [sys.executable, str(ROOT/"tools/native_probe/controller_autoplay.py"), str(pid),
                   str(profile_path), str(output), "--expected-sha256", digest,
                   "--snapshot", snapshot, "--max-bytes", str(budget), "--plan-file", str(plan_path),
                   *[item for category in categories for item in ("--category", category)]]
            if probe_profile:
                cmd += ["--probe-profile", str(probe_profile)]
            function_file, summary_file, corpus = (Path(v).expanduser() for v in xref_values)
            if function_file.is_file() and summary_file.is_file() and corpus.is_dir():
                cmd += ["--xref-functions", str(function_file), "--xref-summary", str(summary_file),
                        "--corpus", str(corpus)]
            self.after(0, lambda: self.log_line("Launching: "+subprocess.list2cmdline(cmd)))
            self.after(0, lambda: self.status_var.set("Running controller replay and selected captures"))
            result = subprocess.run(cmd, capture_output=True, text=True)
            self.after(0, lambda: self.log_line(result.stdout+result.stderr))
            self.after(0, lambda: self.status_var.set(f"Run finished (exit {result.returncode}): {output}"))
        except Exception as error:
            message = str(error)
            self.after(0, lambda message=message: self.log_line(message))
            self.after(0, lambda: self.status_var.set("Capture setup failed"))
        finally:
            self.after(0, lambda: self.start_button.configure(state="normal"))

    def set_categories(self, enabled):
        for variable in self.category_vars.values():
            variable.set(enabled)


if __name__ == "__main__":
    if sys.platform != "win32":
        raise SystemExit("ACU capture launcher requires Windows")
    CaptureLauncher().mainloop()
