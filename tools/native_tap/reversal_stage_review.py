"""Locate pose edits during the controlled reversal capture (no fitted offsets).

Usage: reversal_stage_review.py capture-meta.json output.json
Requires reversal_capture.py --stages. Compares nested game-thread snapshots.
"""
import json
import math
from pathlib import Path
import struct
import sys
import tap_events as E

meta=json.loads(Path(sys.argv[1]).read_text())
n=meta['joints']
stats={k:[[] for _ in range(n)] for k in ['modifier_local','modifier_model']}
def compare(label,a,b):
    for j in range(n):
        qa=struct.unpack_from('<4f',a,j*32+16);qb=struct.unpack_from('<4f',b,j*32+16)
        norm=math.sqrt(sum(v*v for v in qa)*sum(v*v for v in qb))
        if not norm:raise ValueError(f'zero quaternion {label} joint {j}')
        dot=min(1.,abs(sum(x*y for x,y in zip(qa,qb)))/norm)
        stats[label][j].append(math.degrees(2*math.acos(dot)))
active={};frames=0;unmatched=0
for e in E.parse(meta['dump']):
    if e['hook']!=1:continue
    if not e['phase']:
        active.setdefault(e['tid'],[]).append(e)
    else:
        stack=active.get(e['tid'],[])
        if not stack:unmatched+=1;continue
        a=stack.pop()['snaps'];b=e['snaps']
        compare('modifier_local',a[20],b[22]);compare('modifier_model',a[21],b[23])
        frames+=1
def summarize(values):
    s=sorted(values)
    return dict(samples=len(s),max_deg=max(s),mean_deg=sum(s)/len(s),p95_deg=s[min(len(s)-1,int(len(s)*.95))])
if not frames:raise SystemExit('Unavailable: no coherent modifier entry/exit pairs')
report=dict(source=meta['dump'],sha256=meta['dump_sha256'],coherent_frames=frames,unmatched=unmatched,
            stages={label:sorted([dict(joint=j,**summarize(v))for j,v in enumerate(rows)],key=lambda r:r['max_deg'],reverse=True)for label,rows in stats.items()})
Path(sys.argv[2]).write_text(json.dumps(report,indent=1))
print(json.dumps(dict(frames=frames,unmatched=unmatched,top={k:v[:8]for k,v in report['stages'].items()}),indent=1))
