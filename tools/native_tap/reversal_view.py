"""Create a local visible skeleton/clip comparison for the four reversal review.

Usage: reversal_view.py native-meta.json rust-stdout.log output.html
This is a diagnostic, not a calibrated pose parity gate. Each rig retains its
own hierarchy. Native QPC/unix alignment is measured on this machine at export.
"""
import ctypes as C
import csv
import json
import math
from pathlib import Path
import struct
import sys
import time
import tap_events as E

meta=json.loads(Path(sys.argv[1]).read_text())
audit=Path(__file__).resolve().parents[2]/'local/locomotion-audit-20261019/locomotion-candidates.csv'
names={r['file_id']:r['name'] for r in csv.DictReader(audit.open(encoding='utf-8'))} if audit.exists() else {}
rust=[]
for line in Path(sys.argv[2]).read_text(encoding='utf-8').splitlines():
    if not line.startswith('REVERSAL_REVIEW '):continue
    try:
        r=json.loads(line.split(' ',1)[1])
    except json.JSONDecodeError:
        continue # The visible viewer may still be writing its final line.
    if rust and r['tick']<=rust[-1]['tick']:break
    rust.append(r)
if len(rust)<175:raise SystemExit(f'Wait for one visible replay cycle: {len(rust)}/180 samples')
counter=C.c_longlong();frequency=C.c_longlong()
C.windll.kernel32.QueryPerformanceFrequency(C.byref(frequency))
before=time.time();C.windll.kernel32.QueryPerformanceCounter(C.byref(counter));after=time.time()
unix_minus_qpc=(before+after)/2-counter.value/frequency.value
timeline=Path(meta['profile']).parent/('run-'+Path(meta['profile']).name.removesuffix('-profile.json'))/'input-timeline.jsonl'
steps=[json.loads(line)for line in timeline.read_text().splitlines()if json.loads(line)['kind']=='step_start']
start=steps[0]['unix_time']
native=[]
parents=[None if (p:=struct.unpack_from('<H',bytes.fromhex(meta['hierarchy_hex']),i*12)[0])==65535 else p for i in range(meta['joints'])]
initial_yaw=None
for e in E.parse(meta['dump']):
    if e['hook']!=0 or not e['phase'] or not e['snaps'].get(7):continue
    t=e['qpc']/frequency.value+unix_minus_qpc-start
    if not 0<=t<18:continue
    matrix=struct.unpack('<16f',e['snaps'][3]);yaw=math.atan2(matrix[1],matrix[0])
    if initial_yaw is None:initial_yaw=yaw
    pos=[struct.unpack_from('<3f',e['snaps'][7],j*32)for j in range(meta['joints'])]
    root=pos[0];pos=[[p[k]-root[k]for k in range(3)]for p in pos]
    leaves=[dict(file_id=l['file_id'],name=names.get(l['file_id'],''),weight=l['weight'],time900=l['time900'],offset876=l['offset876'])for l in E.parse_graph(e['snaps'][2])['leaves']if l['weight']>.01]
    native.append(dict(time=t,pose=pos,yaw=yaw-initial_yaw,clips=leaves))
if not native:raise SystemExit('No native frames in input interval')
frames=[]
for r in rust:
    t=r['tick']/60
    n=min(native,key=lambda n:abs(n['time']-t))
    root=r['pose'][0]
    frames.append(dict(time=t,native=n,rust=dict(pose=[[p[k]-root[k]for k in range(3)]for p in r['pose']],yaw=r['yaw'],clip=r['clip'],name=r['name'])))
data=dict(frames=frames,native_parents=parents,rust_parents=rust[0]['parents'],steps=steps,native_frames=len(native),source=meta['dump'],alignment='Post-capture QPC/unix calibration; input reports at 60 Hz; not a pose parity gate')
template=Path(__file__).with_name('reversal_view.html').read_text(encoding='utf-8')
Path(sys.argv[3]).write_text(template.replace('/*CAPTURE_DATA*/',json.dumps(data)),encoding='utf-8')
print(Path(sys.argv[3]).resolve(),len(native),'native frames',len(frames),'display frames')
