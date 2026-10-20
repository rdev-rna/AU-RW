"""Visible native right/left reversal capture requested in the 2026-10-20 review.

Four repetitions: full right 2 s, full left 2 s, neutral bookends. Captures
selected graph leaves, animation output and final model pose on the game thread.
"""
import ctypes as C
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import time

os.environ.setdefault('ACU_TAP_STEM','acu_tap13')
import tap_client as t

ROOT=Path(__file__).resolve().parents[2]
NP=ROOT/'local/native-probe'
SHA='748abdc6456397fe7d8080f975f2a5ed3123bce548e7bf1c68b23f47e8b27e40'

def foreground(pid):
    from ctypes import wintypes as W
    u=C.WinDLL('user32',use_last_error=True)
    u.GetForegroundWindow.restype=W.HWND
    u.SetForegroundWindow.argtypes=[W.HWND]
    u.ShowWindow.argtypes=[W.HWND,C.c_int]
    windows=[]
    callback=C.WINFUNCTYPE(W.BOOL,W.HWND,W.LPARAM)
    def visit(hwnd,_):
        owner=W.DWORD();u.GetWindowThreadProcessId(hwnd,C.byref(owner))
        if owner.value==pid and u.IsWindowVisible(hwnd): windows.append(hwnd)
        return True
    u.EnumWindows(callback(visit),0)
    if not windows: raise RuntimeError('target has no visible window')
    u.keybd_event(0x12,0,0,0)
    try:
        u.ShowWindow(windows[0],9);u.SetForegroundWindow(windows[0])
    finally: u.keybd_event(0x12,0,2,0)
    time.sleep(.5)
    owner=W.DWORD();u.GetWindowThreadProcessId(u.GetForegroundWindow(),C.byref(owner))
    if owner.value!=pid: raise RuntimeError('Windows did not focus target; no inputs sent')

def main():
    pid=int(sys.argv[1]);comp=int(sys.argv[2],16);tag=sys.argv[3]
    c=lambda s:t.command(pid,s,timeout=8)
    status=c('STATUS');base=int(status['image_base'],16)
    pose=struct.unpack('<Q',t.read(pid,comp+0xd8,8))[0]
    # The skeleton for these pose objects is at +0x170 of the model pose.
    model=struct.unpack('<Q',t.read(pid,pose+0x288,8))[0]
    skel=struct.unpack('<Q',t.read(pid,model+0x170,8))[0]
    hp,_,n=struct.unpack('<QHH',t.read(pid,skel+0x68,12))
    if n>512 or n<1: raise RuntimeError('joint count outside capture bound')
    hierarchy=t.read(pid,hp,n*12)
    steps=[dict(name='idle_before',duration_ms=1000)]
    for k in range(4):
        for label,x in [('right',32767),('left',-32767)]:
            steps.append(dict(name=f'{label}_{k+1}',duration_ms=2000,left_stick=[x,0]))
    steps.append(dict(name='idle_after',duration_ms=1000))
    profile=NP/f'{tag}-profile.json'
    profile.write_text(json.dumps(dict(schema_version=1,report_hz=60,steps=steps),indent=1))
    guard='40534883ec30488bd9488b89d0000000'
    if t.read(pid,base+0xe9cc0,16).hex()!=guard: raise RuntimeError('live guard mismatch')
    c('RESET');c('CLEARSNAPS')
    assert c(f'DRFILTER 0 {comp:x} 0')['ok']
    for spec in ['SNAPGRAPH 0 exit 2 a0 +d0 *','SNAP 0 exit 3 64 a0 +10 * +20',
                 'SNAP 0 exit 4 384 a0 +d8 * +280 *','SNAP 0 exit 5 384 a0 +d8 * +288 *',
                 f'SNAP 0 exit 6 {n*32} a0 +d8 * +280 * +178 *',
                 f'SNAP 0 exit 7 {n*32} a0 +d8 * +288 * +178 *']:
        assert c(spec)['ok'],spec
    foreground(pid)
    stage_guard=None
    if '--stages' in sys.argv:
        stage_guard=t.read(pid,base+0x377e0,16).hex()
        assert c(f'DRFILTER 1 {pose:x} 0')['ok']
        for phase,local_id,model_id in [('entry',20,21),('exit',22,23)]:
            for sid,offset in [(local_id,'280'),(model_id,'288')]:
                assert c(f'SNAP 1 {phase} {sid} {n*32} a0 +{offset} * +178 *')['ok']
    started=time.time()
    try:
        assert c(f'DRHOOK 0 e9cc0 {guard} reversal_comp exit')['ok']
        if stage_guard:
            assert c(f'DRHOOK 1 377e0 {stage_guard} reversal_modifiers exit')['ok']
        run=subprocess.run([sys.executable,str(ROOT/'tools/native_probe/controller_autoplay.py'),str(pid),
                            str(profile),str(NP/f'run-{tag}'),'--expected-sha256',SHA,'--snapshot','none'],
                           cwd=ROOT,timeout=55,capture_output=True,text=True)
    finally:
        c('DRUNHOOK 0')
        if stage_guard:c('DRUNHOOK 1')
    dump=NP/f'ev-{tag}.bin';assert c(f'DUMP {dump.as_posix()}')['ok']
    meta=dict(pid=pid,component=hex(comp),pose_component=hex(pose),image_base=hex(base),image_sha256=SHA,
              joints=n,hierarchy_hex=hierarchy.hex(),profile=str(profile),guard=guard,stage_guard=stage_guard,started=started,
              dump=str(dump),dump_sha256=hashlib.sha256(dump.read_bytes()).hexdigest(),status=c('STATUS'),
              runner_rc=run.returncode,runner_stdout=run.stdout,runner_stderr=run.stderr)
    (NP/f'{tag}-meta.json').write_text(json.dumps(meta,indent=1))
    print(json.dumps({k:meta[k] for k in ['dump','runner_rc','runner_stderr']}),flush=True)
    if run.returncode: raise SystemExit('input run failed; partial capture retained')

if __name__=='__main__':main()
